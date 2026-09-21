//! Real-handle POSIX fd table (governance doc 3.6, layer 3 — engine landing).
//!
//! Direct port of `experiments/fork-hybrid/fdtable-poc/src/lib.rs`: a slot
//! array where each slot holds one `FdEntry` carrying a raw Windows HANDLE.
//! Core semantic (empirically verified in the POC):
//! **DuplicateHandle duplicates share the source handle's file offset**
//! (both refer to the same kernel file object), which is exactly what GNU
//! bash's fork+dup2 fd semantics need (`dup` in POSIX shares the open file
//! description; `open` does not).
//!
//! Shell-level operations modeled:
//!   - `open`  — CreateFileW / pipe end installed in a slot
//!   - `dup`   — DuplicateHandle (n>&m / <&m / >&m)
//!   - `close` — CloseHandle
//!   - `query` — slot inspection
//!   - `fork_table` — subshell: the whole table duplicated handle-by-handle
//!     (fork copies the fd table, not the file objects)
//!
//! Windows edge case carried over from the POC: a drained anonymous pipe
//! whose write end is closed reports ERROR_BROKEN_PIPE (109) from ReadFile
//! instead of a zero-byte read — `read_n` maps 109 to logical EOF.

#![allow(non_snake_case, non_camel_case_types)]

use std::ffi::c_void;
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;

pub type HANDLE = isize; // isize so FdEntry/FdTable are Send+Sync
pub type BOOL = i32;
pub type DWORD = u32;

const INVALID_HANDLE_VALUE: HANDLE = -1;
const GENERIC_READ: DWORD = 0x8000_0000;
const GENERIC_WRITE: DWORD = 0x4000_0000;
const FILE_SHARE_READ: DWORD = 0x0000_0001;
const FILE_SHARE_WRITE: DWORD = 0x0000_0002;
const OPEN_EXISTING: DWORD = 3;
const CREATE_ALWAYS: DWORD = 2;
const FILE_ATTRIBUTE_TEMPORARY: DWORD = 0x0000_0100;
const FILE_BEGIN: DWORD = 0;
pub const HANDLE_FLAG_INHERIT: DWORD = 0x0000_0001;
const DUPLICATE_SAME_ACCESS: DWORD = 0x0000_0002;

#[link(name = "kernel32")]
extern "system" {
    fn GetCurrentProcess() -> HANDLE;
    fn CreateFileW(
        name: *const u16,
        access: DWORD,
        share: DWORD,
        sa: *const c_void,
        disposition: DWORD,
        flags: DWORD,
        template: HANDLE,
    ) -> HANDLE;
    fn CreatePipe(
        hRead: *mut HANDLE,
        hWrite: *mut HANDLE,
        sa: *const SECURITY_ATTRIBUTES,
        size: DWORD,
    ) -> BOOL;
    fn DuplicateHandle(
        hSrcProc: HANDLE,
        hSrc: HANDLE,
        hDstProc: HANDLE,
        lpTarget: *mut HANDLE,
        access: DWORD,
        inherit: BOOL,
        options: DWORD,
    ) -> BOOL;
    fn CloseHandle(h: HANDLE) -> BOOL;
    fn ReadFile(
        h: HANDLE,
        buf: *mut u8,
        n: DWORD,
        read: *mut DWORD,
        overlapped: *mut c_void,
    ) -> BOOL;
    fn WriteFile(
        h: HANDLE,
        buf: *const u8,
        n: DWORD,
        written: *mut DWORD,
        overlapped: *mut c_void,
    ) -> BOOL;
    fn SetFilePointer(h: HANDLE, dist: i32, dist_high: *mut i32, method: DWORD) -> DWORD;
    fn SetHandleInformation(h: HANDLE, mask: DWORD, flags: DWORD) -> BOOL;
    fn GetHandleInformation(h: HANDLE, flags: *mut DWORD) -> BOOL;
}

#[repr(C)]
pub struct SECURITY_ATTRIBUTES {
    pub nLength: DWORD,
    pub lpSecurityDescriptor: *mut c_void,
    pub bInheritHandle: BOOL,
}

fn to_wide(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FdEntry {
    pub handle: HANDLE,
    /// inheritable flag mirrors bash's close-on-exec-adjacent bookkeeping;
    /// precise inheritance whitelists use it.
    pub inheritable: bool,
}

impl FdEntry {
    fn mark_inheritable(&mut self) -> Result<(), String> {
        let ok = unsafe { SetHandleInformation(self.handle, HANDLE_FLAG_INHERIT, HANDLE_FLAG_INHERIT) };
        if ok == 0 {
            return Err("SetHandleInformation failed".into());
        }
        self.inheritable = true;
        Ok(())
    }

    pub fn is_inheritable(&self) -> bool {
        let mut flags: DWORD = 0;
        unsafe { GetHandleInformation(self.handle, &mut flags) != 0 && (flags & HANDLE_FLAG_INHERIT) != 0 }
    }
}

/// POSIX-style fd table: slots 0.. carry `Option<FdEntry>`.
#[derive(Debug, Default)]
pub struct FdTable {
    slots: Vec<Option<FdEntry>>,
}

impl FdTable {
    pub fn new() -> Self {
        Self { slots: Vec::new() }
    }

    fn grow_to(&mut self, slot: usize) {
        if self.slots.len() <= slot {
            self.slots.resize(slot + 1, None);
        }
    }

    /// Install an already-open handle in a slot (replacing any prior entry,
    /// which is closed — POSIX `dup2` semantics).
    pub fn install(&mut self, slot: usize, handle: HANDLE) -> Result<(), String> {
        if handle == INVALID_HANDLE_VALUE || handle == 0 {
            return Err("install: invalid handle".into());
        }
        self.grow_to(slot);
        if let Some(old) = self.slots[slot].take() {
            unsafe { CloseHandle(old.handle) };
        }
        self.slots[slot] = Some(FdEntry { handle, inheritable: false });
        Ok(())
    }

    /// `N<file`: open a file for reading into `slot`.
    pub fn open_read(&mut self, slot: usize, path: &str) -> Result<(), String> {
        let wide = to_wide(path);
        let h = unsafe {
            CreateFileW(
                wide.as_ptr(),
                GENERIC_READ,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                std::ptr::null(),
                OPEN_EXISTING,
                FILE_ATTRIBUTE_TEMPORARY,
                0,
            )
        };
        if h == INVALID_HANDLE_VALUE {
            return Err(format!("open_read({path}): CreateFileW failed"));
        }
        self.install(slot, h)
    }

    /// Create a file for writing into `slot`.
    pub fn open_write(&mut self, slot: usize, path: &str) -> Result<(), String> {
        let wide = to_wide(path);
        let h = unsafe {
            CreateFileW(
                wide.as_ptr(),
                GENERIC_WRITE,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                std::ptr::null(),
                CREATE_ALWAYS,
                FILE_ATTRIBUTE_TEMPORARY,
                0,
            )
        };
        if h == INVALID_HANDLE_VALUE {
            return Err(format!("open_write({path}): CreateFileW failed"));
        }
        self.install(slot, h)
    }

    /// Anonymous pipe ends installed in two slots.
    pub fn open_pipe(&mut self, read_slot: usize, write_slot: usize) -> Result<(), String> {
        let sa = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as DWORD,
            lpSecurityDescriptor: std::ptr::null_mut(),
            bInheritHandle: 1,
        };
        let (mut r, mut w): (HANDLE, HANDLE) = (0, 0);
        if unsafe { CreatePipe(&mut r, &mut w, &sa, 0) } == 0 {
            return Err("CreatePipe failed".into());
        }
        self.install(read_slot, r)?;
        self.install(write_slot, w)?;
        Ok(())
    }

    /// `N>&M` / `N<&M`: duplicate the open file description into `slot`.
    /// Both slots then share the kernel file object (and thus the offset).
    pub fn dup(&mut self, from: usize, to: usize) -> Result<(), String> {
        let src = self.query(from).ok_or(format!("dup: fd {from} not open"))?;
        let mut target: HANDLE = 0;
        let ok = unsafe {
            DuplicateHandle(
                GetCurrentProcess(),
                src,
                GetCurrentProcess(),
                &mut target,
                0,
                0,
                DUPLICATE_SAME_ACCESS,
            )
        };
        if ok == 0 {
            return Err(format!("dup {from}->{to}: DuplicateHandle failed"));
        }
        self.install(to, target)
    }

    /// `N<&-`: close a slot. Only this slot's handle is closed; duplicates
    /// in other slots (or other tables) keep the file object alive.
    pub fn close(&mut self, slot: usize) -> Result<(), String> {
        match self.slots.get_mut(slot).and_then(|s| s.take()) {
            Some(entry) => {
                if unsafe { CloseHandle(entry.handle) } == 0 {
                    return Err(format!("close {slot}: CloseHandle failed"));
                }
                Ok(())
            }
            None => Err(format!("close {slot}: fd not open")),
        }
    }

    pub fn query(&self, slot: usize) -> Option<HANDLE> {
        self.slots.get(slot).and_then(|s| s.as_ref().map(|e| e.handle))
    }

    pub fn entry(&self, slot: usize) -> Option<&FdEntry> {
        self.slots.get(slot).and_then(|s| s.as_ref())
    }

    pub fn is_open(&self, slot: usize) -> bool {
        self.query(slot).is_some()
    }

    /// Subshell / background job: duplicate every open slot into a fresh
    /// table. The child's handles are duplicates of the same kernel objects,
    /// so offsets stay shared across the boundary — POSIX fork semantics.
    pub fn fork_table(&self) -> Result<FdTable, String> {
        let mut child = FdTable::new();
        child.grow_to(self.slots.len().saturating_sub(1));
        for (i, slot) in self.slots.iter().enumerate() {
            if let Some(entry) = slot {
                let mut target: HANDLE = 0;
                let ok = unsafe {
                    DuplicateHandle(
                        GetCurrentProcess(),
                        entry.handle,
                        GetCurrentProcess(),
                        &mut target,
                        0,
                        0,
                        DUPLICATE_SAME_ACCESS,
                    )
                };
                if ok == 0 {
                    return Err(format!("fork_table: DuplicateHandle fd {i} failed"));
                }
                child.slots[i] = Some(FdEntry {
                    handle: target,
                    inheritable: entry.inheritable,
                });
            }
        }
        Ok(child)
    }

    /// Handles of the open slots marked inheritable — the spawn whitelist
    /// fed to PROC_THREAD_ATTRIBUTE_HANDLE_LIST.
    pub fn inheritable_handles(&self) -> Vec<HANDLE> {
        self.slots
            .iter()
            .filter_map(|s| s.as_ref())
            .filter(|e| e.inheritable)
            .map(|e| e.handle)
            .collect()
    }

    pub fn mark_inheritable(&mut self, slot: usize) -> Result<(), String> {
        self.grow_to(slot);
        match self.slots[slot].as_mut() {
            Some(e) => e.mark_inheritable(),
            None => Err(format!("mark_inheritable {slot}: fd not open")),
        }
    }

    // ---- convenience I/O ----

    /// Read up to `n` bytes from a slot (advancing the shared offset).
    pub fn read_n(&self, slot: usize, n: usize) -> Result<Vec<u8>, String> {
        let h = self.query(slot).ok_or(format!("read: fd {slot} not open"))?;
        let mut buf = vec![0u8; n];
        let mut got: DWORD = 0;
        let ok = unsafe { ReadFile(h, buf.as_mut_ptr(), n as DWORD, &mut got, std::ptr::null_mut()) };
        if ok == 0 {
            // A drained anonymous pipe whose write end is closed reports
            // ERROR_BROKEN_PIPE instead of a zero-byte read: that is EOF.
            const ERROR_BROKEN_PIPE: DWORD = 109;
            if std::io::Error::last_os_error().raw_os_error() == Some(ERROR_BROKEN_PIPE as i32) {
                buf.truncate(0);
                return Ok(buf);
            }
            return Err(format!("read fd {slot}: ReadFile failed: {}", std::io::Error::last_os_error()));
        }
        buf.truncate(got as usize);
        Ok(buf)
    }

    pub fn write_all(&self, slot: usize, bytes: &[u8]) -> Result<(), String> {
        let h = self.query(slot).ok_or(format!("write: fd {slot} not open"))?;
        let mut off = 0usize;
        while off < bytes.len() {
            let mut n: DWORD = 0;
            let ok = unsafe {
                WriteFile(h, bytes[off..].as_ptr(), (bytes.len() - off) as DWORD, &mut n, std::ptr::null_mut())
            };
            if ok == 0 {
                return Err(format!("write fd {slot}: WriteFile failed"));
            }
            off += n as usize;
        }
        Ok(())
    }

    /// Explicit seek on the shared file object (SetFilePointer).
    pub fn seek(&self, slot: usize, pos: i32) -> Result<(), String> {
        let h = self.query(slot).ok_or(format!("seek: fd {slot} not open"))?;
        let r = unsafe { SetFilePointer(h, pos, std::ptr::null_mut(), FILE_BEGIN) };
        if r == 0xFFFF_FFFF {
            return Err(format!("seek fd {slot}: SetFilePointer failed"));
        }
        Ok(())
    }
}

/// Raw ReadFile on a bare HANDLE — child-side accessor for precisely
/// inherited handles passed via whitelists.
///
/// # Safety
/// `h` must be a valid readable HANDLE; `buf`/`n` a valid region.
pub unsafe fn raw_read(h: HANDLE, buf: *mut u8, n: DWORD, got: *mut DWORD) -> BOOL {
    ReadFile(h, buf, n, got, std::ptr::null_mut())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn make_file(name: &str, contents: &[u8]) -> String {
        let dir = std::env::temp_dir().join("fdtable-engine-tests");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(contents).unwrap();
        path.to_str().unwrap().to_string()
    }

    const PAYLOAD: &[u8] = b"abcdefghij";

    #[test]
    fn dup_shares_file_offset() {
        let path = make_file("offset.txt", PAYLOAD);
        let mut t = FdTable::new();
        t.open_read(3, &path).unwrap();
        assert_eq!(t.read_n(3, 4).unwrap(), b"abcd");
        t.dup(3, 4).unwrap();
        // slot 4 continues where slot 3 left off - the core hypothesis.
        assert_eq!(t.read_n(4, 3).unwrap(), b"efg");
        // and slot 3 continues where 4 left off.
        assert_eq!(t.read_n(3, 3).unwrap(), b"hij");
        assert_eq!(t.read_n(4, 1).unwrap(), b"" /* EOF, shared offset */);
    }

    #[test]
    fn seek_via_one_slot_moves_other() {
        let path = make_file("seek.txt", PAYLOAD);
        let mut t = FdTable::new();
        t.open_read(3, &path).unwrap();
        t.dup(3, 5).unwrap();
        t.seek(5, 8).unwrap();
        assert_eq!(t.read_n(3, 2).unwrap(), b"ij");
    }

    #[test]
    fn close_one_slot_other_slot_alive() {
        let path = make_file("close.txt", PAYLOAD);
        let mut t = FdTable::new();
        t.open_read(3, &path).unwrap();
        t.dup(3, 4).unwrap();
        t.close(3).unwrap();
        assert!(!t.is_open(3));
        assert!(t.is_open(4));
        assert_eq!(t.read_n(4, 4).unwrap(), b"abcd", "duplicate keeps the object alive");
        t.close(4).unwrap();
        assert!(t.read_n(4, 1).is_err());
    }

    #[test]
    fn fork_table_close_isolation() {
        let path = make_file("fork.txt", PAYLOAD);
        let mut parent = FdTable::new();
        parent.open_read(3, &path).unwrap();
        let mut child = parent.fork_table().unwrap();
        // child closes its fd: `( exec 3<&- )` must not touch the parent
        child.close(3).unwrap();
        assert!(parent.is_open(3));
        assert_eq!(parent.read_n(3, 4).unwrap(), b"abcd");
        // and the child's duplicate was properly closed, too
        assert!(child.read_n(3, 1).is_err());
    }

    #[test]
    fn fork_table_offsets_stay_shared_with_parent() {
        let path = make_file("forkshare.txt", PAYLOAD);
        let mut parent = FdTable::new();
        parent.open_read(3, &path).unwrap();
        let child = parent.fork_table().unwrap();
        assert_eq!(child.read_n(3, 4).unwrap(), b"abcd");
        // child's read advanced the shared object: parent continues at 'e'
        assert_eq!(parent.read_n(3, 3).unwrap(), b"efg");
    }

    #[test]
    fn pipe_buffer_shared_across_dup() {
        let mut t = FdTable::new();
        t.open_pipe(3, 9).unwrap();
        t.write_all(9, b"first\nsecond\n").unwrap();
        t.close(9).unwrap(); // EOF for readers; payload stays buffered
        t.dup(3, 4).unwrap();
        assert_eq!(t.read_n(3, 6).unwrap(), b"first\n");
        // duplicate read end consumes the same stream, not a copy
        assert_eq!(t.read_n(4, 7).unwrap(), b"second\n");
        assert_eq!(t.read_n(3, 1).unwrap(), b"");
    }

    #[test]
    fn install_closes_replaced_entry() {
        let p1 = make_file("inst1.txt", PAYLOAD);
        let p2 = make_file("inst2.txt", b"XYZ");
        let mut t = FdTable::new();
        t.open_read(3, &p1).unwrap();
        t.open_read(3, &p2).unwrap(); // replaces fd 3; old handle closed
        assert_eq!(t.read_n(3, 3).unwrap(), b"XYZ");
    }

    #[test]
    fn inheritable_flag_roundtrip() {
        let path = make_file("inh.txt", PAYLOAD);
        let mut t = FdTable::new();
        t.open_read(3, &path).unwrap();
        assert!(!t.entry(3).unwrap().is_inheritable());
        t.mark_inheritable(3).unwrap();
        assert!(t.entry(3).unwrap().is_inheritable());
        assert_eq!(t.inheritable_handles(), vec![t.query(3).unwrap()]);
    }
}
