//! hash module.
//!
//! GNU Bash source ownership:
// - builtins/hash.def

use std::collections::HashMap;
use std::io::{self, Write};

const EXECUTION_SUCCESS: i32 = 0;
const EXECUTION_FAILURE: i32 = 1;
const EX_USAGE: i32 = 2;
const HASH_TABLE: &str = "__RUBASH_HASH_TABLE";

pub fn execute(args: &[String], env_vars: &mut HashMap<String, String>) -> io::Result<i32> {
    let mut stdout = io::stdout();
    let mut stderr = io::stderr();
    execute_with_io(args, env_vars, &mut stdout, &mut stderr)
}

pub(crate) fn execute_with_io<W, E>(
    args: &[String],
    env_vars: &mut HashMap<String, String>,
    stdout: &mut W,
    stderr: &mut E,
) -> io::Result<i32>
where
    W: Write,
    E: Write,
{
    // GNU builtins/hash.def:86-90: when hashing is disabled (set +h), the
    // entire `hash` builtin refuses with "hash: hashing disabled" and
    // returns failure, before any option parsing.
    if !crate::builtins::set::shell_option_enabled(env_vars, "hashall") {
        writeln!(stderr, "{}hash: hashing disabled", script_prefix(env_vars))?;
        return Ok(EXECUTION_FAILURE);
    }

    let mut print = args.is_empty();
    let mut delete = false;
    let mut pathname = None;
    let mut translate = false;
    let mut reusable = false;
    let mut names = Vec::new();
    let mut index = 0;

    while let Some(arg) = args.get(index) {
        if arg == "-p" || arg.starts_with("-p") {
            let value = if arg == "-p" {
                let Some(value) = args.get(index + 1).map(String::as_str) else {
                    writeln!(
                        stderr,
                        "{}hash: -p: option requires an argument",
                        script_prefix(env_vars)
                    )?;
                    writeln!(
                        stderr,
                        "hash: usage: hash [-lr] [-p pathname] [-dt] [name ...]"
                    )?;
                    return Ok(EX_USAGE);
                };
                index += 1;
                value
            } else {
                &arg[2..]
            };
            pathname = Some(value);
            if let Some(name) = args.get(index + 1) {
                names.push(name.as_str());
            }
            break;
        } else if let Some(options) = arg.strip_prefix('-') {
            for option in options.chars() {
                match option {
                    'r' => {
                        env_vars.remove(HASH_TABLE);
                        // GNU `hash -r` forgets all remembered locations; the
                        // internal lookup cache holds the same information, so
                        // it must be dropped too or misses/hits stay stale.
                        crate::executor::path::clear_command_lookup_cache();
                        return Ok(EXECUTION_SUCCESS);
                    }
                    'd' => delete = true,
                    't' => translate = true,
                    'l' => {
                        reusable = true;
                        print = true;
                    }
                    other => {
                        writeln!(
                            stderr,
                            "{}hash: -{other}: invalid option",
                            script_prefix(env_vars)
                        )?;
                        writeln!(
                            stderr,
                            "hash: usage: hash [-lr] [-p pathname] [-dt] [name ...]"
                        )?;
                        return Ok(EX_USAGE);
                    }
                }
            }
        } else {
            names.push(arg.as_str());
        }
        index += 1;
    }

    // GNU builtins/hash.def:124-128: hash -d/-t with no arguments reports
    // "hash: -d: option requires an argument" via sh_needarg.
    if names.is_empty() && (delete || translate) {
        let opt = if delete { "-d" } else { "-t" };
        writeln!(
            stderr,
            "{}hash: {opt}: option requires an argument",
            script_prefix(env_vars)
        )?;
        return Ok(EXECUTION_FAILURE);
    }

    // GNU builtins/hash.def:137-141 `hash` with no names and no -r prints the
    // table through hash_walk (hashlib.c:397) — bucket-array order, and
    // hash_insert prepends, so entries sharing a bucket list newest first.
    // print_hash_info (hash.def:239) shows times_found: 0 for `hash -p` /
    // `BASH_CMDS[k]=v` / `hash name` inserts (phash_insert found=0), bumped by
    // each phash_search hit (hashlib.c:254) and set to 1 by the PATH-search
    // insert in find_user_command_in_path's caller (findcmd.c:416).
    let mut table = hash_table(env_vars);
    if let Some(pathname) = pathname {
        // GNU builtins/hash.def:156-176: in a restricted shell `hash -p`
        // refuses an absolute pathname (sh_restricted -> "hash: <path>:
        // restricted") and requires a relative one to resolve through $PATH
        // (sh_notfound -> "hash: <name>: not found"); both fail the builtin.
        if crate::builtins::set::shell_option_enabled(env_vars, "restricted") {
            if pathname.contains('/') || pathname.contains('\\') {
                writeln!(
                    stderr,
                    "{}hash: {pathname}: restricted",
                    script_prefix(env_vars)
                )?;
                return Ok(EXECUTION_FAILURE);
            }
            if crate::executor::path::find_user_command(pathname, env_vars).is_none() {
                writeln!(
                    stderr,
                    "{}hash: {pathname}: not found",
                    script_prefix(env_vars)
                )?;
                return Ok(EXECUTION_FAILURE);
            }
        }
        if let Some(name) = names.first().copied() {
            if pathname == "/" {
                writeln!(
                    stderr,
                    "{}hash: {pathname}: Is a directory",
                    script_prefix(env_vars)
                )?;
                return Ok(EXECUTION_FAILURE);
            }
            // GNU hash.def:195 phash_insert(w, pathname, 0, 0): an existing
            // entry keeps its bucket position with times_found reset to 0.
            if let Some(entry) = table.iter_mut().find(|entry| entry.0 == name) {
                entry.1 = pathname.to_string();
                entry.2 = 0;
            } else {
                table.push((name.to_string(), pathname.to_string(), 0));
            }
            store_hash_table(env_vars, &table);
            // GNU hash.def `hash -p PATH NAME`: phash_insert(name, pathname)
            // makes subsequent lookups of `name` return `pathname` without a
            // PATH scan. Mirror that in the in-memory lookup cache so
            // find_user_command(name) returns the same path external_inner
            // would execute. Convert through shell_path_to_windows so the
            // cached PathBuf matches the Windows-native form produced by
            // find_user_command_uncached.
            let cached_path = crate::executor::path::shell_path_to_windows(pathname, env_vars);
            crate::executor::path::set_command_lookup_cache(name, Some(cached_path));
            return Ok(EXECUTION_SUCCESS);
        }
        print = true;
    }

    if delete {
        let mut status = EXECUTION_SUCCESS;
        for name in names {
            if !table.iter().any(|entry| entry.0 == name) {
                writeln!(stderr, "{}hash: {name}: not found", script_prefix(env_vars))?;
                status = EXECUTION_FAILURE;
            } else {
                table.retain(|entry| entry.0 != name);
                // GNU hash.def `hash -d NAME`: phash_remove(w) drops the entry
                // from the hash table. The internal lookup cache holds the
                // same information and must be invalidated in lockstep or the
                // next `find_user_command(name)` returns the stale path.
                crate::executor::path::remove_command_lookup_cache(name);
            }
        }
        store_hash_table(env_vars, &table);
        return Ok(status);
    }

    if translate {
        let mut status = EXECUTION_SUCCESS;
        for name in names {
            // GNU hash.def:284 print_hash_info runs phash_search, whose
            // hash_search hit bumps times_found (hashlib.c:254).
            if let Some(entry) = table.iter_mut().find(|entry| entry.0 == name) {
                entry.2 += 1;
                let path = entry.1.clone();
                if reusable {
                    writeln!(stdout, "builtin hash -p {path} {name}")?;
                } else {
                    writeln!(stdout, "{path}")?;
                }
            } else {
                writeln!(stderr, "{}hash: {name}: not found", script_prefix(env_vars))?;
                status = EXECUTION_FAILURE;
            }
        }
        store_hash_table(env_vars, &table);
        return Ok(status);
    }

    if print {
        if !table.is_empty() {
            if !reusable {
                writeln!(stdout, "hits\tcommand")?;
                for (_, path, hits) in bucket_ordered(table) {
                    writeln!(stdout, "{hits:4}\t{path}")?;
                }
                return Ok(EXECUTION_SUCCESS);
            }
            for (name, path, _) in bucket_ordered(table) {
                writeln!(stdout, "builtin hash -p {path} {name}")?;
            }
            return Ok(EXECUTION_SUCCESS);
        }
        // GNU hash.def prints the empty-table message on stdout.
        writeln!(stdout, "hash: hash table empty")?;
        return Ok(EXECUTION_SUCCESS);
    }

    // GNU hash.def bare-name form: `hash NAME...` re-resolves each NAME
    // against PATH and re-inserts the result. The flow is phash_remove(name)
    // + find_user_command(name) + phash_insert(name, path); a name that does
    // not resolve reports "hash: NAME: not found" and sets failure.
    if !names.is_empty() {
        let mut status = EXECUTION_SUCCESS;
        for name in names {
            // Drop any stale remembered location so the PATH scan is
            // authoritative.
            crate::executor::path::remove_command_lookup_cache(name);
            match crate::executor::path::find_user_command(name, env_vars) {
                Some(path) => {
                    // GNU hash.def:221-225 add_filename_to_hash:
                    // phash_remove + phash_insert(w, path, dot, 0) — the name
                    // re-enters its bucket as newest and times_found resets.
                    table.retain(|entry| entry.0 != name);
                    table.push((
                        name.to_string(),
                        path.to_string_lossy().to_string(),
                        0,
                    ));
                    // find_user_command already inserted the result into the
                    // internal cache, so no extra set_command_lookup_cache is
                    // needed here.
                }
                None => {
                    table.retain(|entry| entry.0 != name);
                    writeln!(stderr, "{}hash: {name}: not found", script_prefix(env_vars))?;
                    status = EXECUTION_FAILURE;
                }
            }
        }
        store_hash_table(env_vars, &table);
        return Ok(status);
    }

    Ok(EXECUTION_SUCCESS)
}

/// GNU hashcmd.c phash_insert: insert `name=path` with times_found `found`.
/// An existing entry keeps its bucket-chain position and has its
/// times_found reset to `found` (hashcmd.c:117); a new name prepends to its
/// bucket, which the insertion-order list models as "newest".
pub(crate) fn insert_hashed_path(
    env_vars: &mut HashMap<String, String>,
    name: &str,
    path: &str,
    found: u32,
) {
    let mut table = hash_table(env_vars);
    if let Some(entry) = table.iter_mut().find(|entry| entry.0 == name) {
        entry.1 = path.to_string();
        entry.2 = found;
    } else {
        table.push((name.to_string(), path.to_string(), found));
    }
    store_hash_table(env_vars, &table);
    // BASH_CMDS[name]=value mirrors `hash -p value name`; keep the internal
    // lookup cache in sync so find_user_command(name) returns value.
    let cached_path = crate::executor::path::shell_path_to_windows(path, env_vars);
    crate::executor::path::set_command_lookup_cache(name, Some(cached_path));
}

pub(crate) fn set_hashed_path(env_vars: &mut HashMap<String, String>, name: &str, path: &str) {
    insert_hashed_path(env_vars, name, path, 0);
}

pub(crate) fn remove_hashed_path(env_vars: &mut HashMap<String, String>, name: &str) {
    let mut table = hash_table(env_vars);
    table.retain(|entry| entry.0 != name);
    store_hash_table(env_vars, &table);
    // unset 'BASH_CMDS[name]' mirrors `hash -d name`; drop the internal cache
    // entry so the next lookup re-scans PATH.
    crate::executor::path::remove_command_lookup_cache(name);
}

pub(crate) fn hashed_path(env_vars: &HashMap<String, String>, name: &str) -> Option<String> {
    hash_table(env_vars)
        .into_iter()
        .find(|entry| entry.0 == name)
        .map(|entry| entry.1)
}

/// GNU hashlib.c:254: a hash_search hit increments times_found — every
/// phash_search (`type`, `command -v`, exec resolution, `hash -t`) counts.
pub(crate) fn bump_hashed_path_hit(env_vars: &mut HashMap<String, String>, name: &str) {
    let mut table = hash_table(env_vars);
    if let Some(entry) = table.iter_mut().find(|entry| entry.0 == name) {
        entry.2 += 1;
        store_hash_table(env_vars, &table);
    }
}

/// GNU findcmd.c:365-426: a command resolution that hits hashed_filenames
/// bumps times_found (phash_search -> hashlib.c:254); one resolved through
/// PATH enters the table with times_found=1 (phash_insert found=1).
/// Called from the external-command dispatch once `find_user_command`
/// resolved `name` to `shell_path`.
pub(crate) fn record_command_resolution(
    env_vars: &mut HashMap<String, String>,
    name: &str,
    shell_path: &str,
) {
    let mut table = hash_table(env_vars);
    if let Some(entry) = table.iter_mut().find(|entry| entry.0 == name) {
        entry.2 += 1;
    } else {
        table.push((name.to_string(), shell_path.to_string(), 1));
    }
    store_hash_table(env_vars, &table);
}

/// Entries in GNU hash_walk order (hashlib.c:397-410): bucket index
/// ascending — `hash_string(name) & (FILENAME_HASH_BUCKETS-1)` — and within
/// a bucket newest first, since hash_insert prepends (hashlib.c:338-339).
fn bucket_ordered(table: Vec<(String, String, u32)>) -> Vec<(String, String, u32)> {
    let mut entries: Vec<_> = table.into_iter().rev().collect();
    entries.sort_by_key(|entry| gnu_hash_bucket(&entry.0));
    entries
}

/// GNU hashlib.c:208-225 hash_string — the FNV-1a variant — masked by
/// FILENAME_HASH_BUCKETS (hashcmd.h:24, 256) for the filename table.
fn gnu_hash_bucket(name: &str) -> u32 {
    // hashlib.c:197/208 — 32-bit FNV_OFFSET and `unsigned int` arithmetic.
    let mut hash: u32 = 2166136261;
    for byte in name.bytes() {
        hash = hash.wrapping_add(
            (hash << 1)
                .wrapping_add(hash << 4)
                .wrapping_add(hash << 7)
                .wrapping_add(hash << 8)
                .wrapping_add(hash << 24),
        );
        hash ^= u32::from(byte);
    }
    hash & 255
}

/// BASH_CMDS materializes through build_hashcmd (variables.c:1708), which
/// hash_walks the same filename table — same bucket order as `hash`.
pub(crate) fn hashed_entries(env_vars: &HashMap<String, String>) -> Vec<(String, String)> {
    bucket_ordered(hash_table(env_vars))
        .into_iter()
        .map(|(name, path, _)| (name, path))
        .collect()
}

/// (name, path, times_found) triples in first-insert order — GNU's
/// BUCKET_CONTENTS chain order is bucket-prepend, which the listing path
/// reproduces via `bucket_ordered`.
fn hash_table(env_vars: &HashMap<String, String>) -> Vec<(String, String, u32)> {
    env_vars
        .get(HASH_TABLE)
        .map(|value| {
            value
                .split('\x1f')
                .filter_map(|entry| {
                    let (name, rest) = entry.split_once('=')?;
                    let (path, hits) = rest
                        .split_once('\x1e')
                        .map(|(path, hits)| (path, hits.parse::<u32>().unwrap_or(0)))
                        .unwrap_or((rest, 0));
                    Some((name.to_string(), path.to_string(), hits))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn store_hash_table(env_vars: &mut HashMap<String, String>, table: &[(String, String, u32)]) {
    env_vars.insert(
        HASH_TABLE.to_string(),
        table
            .iter()
            .map(|(name, path, hits)| format!("{name}={path}\x1e{hits}"))
            .collect::<Vec<_>>()
            .join("\x1f"),
    );
}

fn script_prefix(env_vars: &HashMap<String, String>) -> String {
    if let (Some(script), Some(line)) = (
        env_vars.get("__RUBASH_SCRIPT_NAME"),
        env_vars.get("__RUBASH_CURRENT_LINE"),
    ) {
        return format!("{script}: line {line}: ");
    }
    "rubash: ".to_string()
}
