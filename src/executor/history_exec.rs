//! The history builtin over the session history list (builtins/history.def
//! operating on the shell's own list, not a host provider).
//!
//! GNU source ownership: builtins/history.def (history_builtin), with the
//! -p expansion going through histexpand.c history_expand.

use std::cell::RefCell;
use std::io::Write;
use std::rc::Rc;

use super::Executor;
use crate::history::SessionHistory;
use crate::history_expand::{HistChars, HistCtx};

/// The listing subcommands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HistoryMode {
    List,
    Delete,
    Print,
    Save,
    Append,
    Write,
    Read,
    ReadNew,
}

/// Build the expansion context (histchars plus posix mode) from the shell env.
pub(in crate::executor) fn hist_ctx(executor: &Executor) -> HistCtx {
    let chars = executor.get_env("histchars").unwrap_or("!^#");
    let mut it = chars.chars();
    HistCtx {
        chars: HistChars {
            expand: it.next().unwrap_or('!'),
            subst: it.next().unwrap_or('^'),
            comment: it.next().unwrap_or('#'),
        },
        posix: executor
            .get_env("__RUBASH_POSIX_MODE")
            .map(|v| v == "1")
            .unwrap_or(false),
    }
}

fn histsize_of(executor: &Executor) -> usize {
    executor
        .get_env("HISTSIZE")
        .and_then(|v| v.trim().parse::<usize>().ok())
        .unwrap_or(0)
}

/// Execute the history builtin against the session list.
pub(in crate::executor) fn execute_history_session(
    executor: &Executor,
    args: &[String],
    session: Rc<RefCell<SessionHistory>>,
    stdout: &mut Vec<u8>,
    stderr: &mut Vec<u8>,
) -> Result<i32, ExecuteErrorAlias> {
    let mut clear = false;
    let mut mode = HistoryMode::List;
    let mut count: Option<usize> = None;
    let mut delete_offset: Option<String> = None;
    let mut operands: Vec<String> = Vec::new();
    let mut expecting_offset = false;
    let mut no_more_opts = false;
    // history.def:161-166: track how many of -a/-r/-w/-n were given; GNU
    // rejects "cannot use more than one of -anrw" when 2+ are set.
    let mut anrw_count: u8 = 0;

    let mut i = 0usize;
    while i < args.len() {
        let arg = &args[i];
        if expecting_offset {
            delete_offset = Some(arg.clone());
            expecting_offset = false;
            i += 1;
            continue;
        }
        if arg == "--" {
            no_more_opts = true;
            i += 1;
            continue;
        }
        if !no_more_opts && arg.len() >= 2 && arg.starts_with('-') {
            // A bare negative number is a listing count (history -5).
            if arg[1..].chars().all(|c| c.is_ascii_digit()) {
                count = arg[1..].parse::<usize>().ok();
                i += 1;
                continue;
            }
            let mut bad: Option<char> = None;
            for c in arg[1..].chars() {
                match c {
                    'c' => clear = true,
                    'd' => {
                        mode = HistoryMode::Delete;
                        expecting_offset = true;
                    }
                    'p' => {
                        mode = HistoryMode::Print;
                        no_more_opts = true;
                    }
                    's' => {
                        mode = HistoryMode::Save;
                        no_more_opts = true;
                    }
                    'a' => {
                        mode = HistoryMode::Append;
                        anrw_count += 1;
                    }
                    'w' => {
                        mode = HistoryMode::Write;
                        anrw_count += 1;
                    }
                    'r' => {
                        mode = HistoryMode::Read;
                        anrw_count += 1;
                    }
                    'n' => {
                        mode = HistoryMode::ReadNew;
                        anrw_count += 1;
                    }
                    other => bad = Some(other),
                }
            }
            if let Some(c) = bad {
                let _ = writeln!(
                    stderr,
                    "{}history: -{c}: invalid option",
                    executor.diagnostic_prefix()
                );
                let _ = writeln!(stderr, "history: usage: history [-c] [-d offset] [n] or history -anrw [filename] or history -ps arg [arg...]");
                return Ok(2);
            }
            i += 1;
            continue;
        }
        if mode == HistoryMode::List && count.is_none() && arg.chars().all(|c| c.is_ascii_digit()) {
            count = arg.parse::<usize>().ok();
            i += 1;
            continue;
        }
        operands.push(arg.clone());
        i += 1;
    }
    // history.def:161-166: reject when more than one of -a/-r/-w/-n is set.
    if anrw_count > 1 {
        let _ = writeln!(
            stderr,
            "{}history: cannot use more than one of -anrw",
            executor.diagnostic_prefix()
        );
        return Ok(1);
    }

    // GNU display_history -> get_numeric_arg: a non-numeric listing limit is
    // "numeric argument required" with EX_USAGE, not a full listing.
    if mode == HistoryMode::List {
        if let Some(bad) = operands.first() {
            if bad.parse::<i64>().is_err() {
                let _ = writeln!(
                    stderr,
                    "{}history: {bad}: numeric argument required",
                    executor.diagnostic_prefix()
                );
                return Ok(2);
            }
            count = bad.parse::<usize>().ok();
        }
    }

    let ctx = hist_ctx(executor);
    let histsize = histsize_of(executor);
    let mut shell = session.borrow_mut();

    if clear {
        shell.clear();
    }

    match mode {
        HistoryMode::Delete => {
            // GNU 5.3 history.def:190-262: `history -d start-end` deletes
            // the INCLUSIVE range. The separator scan starts after a
            // leading '-' (`-2-4` = start -2, end 4); negative numbers
            // count back from the end of the list (-1 is the last entry);
            // positive numbers are displayed numbers, offset by
            // history_base (bashhist.c). valid_number accepts strtoimax
            // base-0 spellings (0xaf). The C code NUL-terminates the
            // separator in place, so erange diagnostics print the
            // offending SIDE (start text on start errors, end text on
            // end errors); first > last fails silently
            // (remove_history_range, readline/history.c).
            let len = shell.entries.len() as i128;
            // GNU valid_number (general.c:248) uses strtoimax base 10 --
            // no hex/octal spellings ("0xaf" is INVALID and reports the
            // whole argument after restoring the '-' separator).
            let parse_pos = |text: &str| -> Option<i128> {
                let value: i128 = text.parse().ok()?;
                if text.starts_with('-') && value < 0 {
                    Some(value + len)
                } else if value > 0 {
                    Some(value - shell.base as i128)
                } else {
                    Some(0)
                }
            };
            if let Some(arg) = delete_offset.as_deref() {
                let search_from = if arg.starts_with('-') { 1 } else { 0 };
                let range_pos = arg[search_from..].find('-').map(|pos| pos + search_from);
                let ok = if let Some(pos) = range_pos {
                    let (start_text, end_text) = (&arg[..pos], &arg[pos + 1..]);
                    match (parse_pos(start_text), parse_pos(end_text)) {
                        (Some(start), Some(end))
                            if start >= 0
                                && end >= 0
                                && (start as usize) < shell.entries.len()
                                && (end as usize) < shell.entries.len()
                                && start <= end =>
                        {
                            shell.entries.drain(start as usize..=end as usize);
                            true
                        }
                        (Some(start), Some(end)) if start >= 0 && start < len => {
                            // start in range, end bad (or first > last
                            // with a bad end): GNU reports the end text
                            let bad = if end < 0 || end >= len {
                                end_text
                            } else {
                                // first > last, both in range: silent
                                return Ok(1);
                            };
                            let _ = writeln!(
                                stderr,
                                "{}history: {bad}: history position out of range",
                                executor.diagnostic_prefix()
                            );
                            return Ok(1);
                        }
                        (Some(start), _) if start < 0 || start >= len => {
                            let _ = writeln!(
                                stderr,
                                "{}history: {start_text}: history position out of range",
                                executor.diagnostic_prefix()
                            );
                            return Ok(1);
                        }
                        _ => {
                            // unparseable sides: GNU restores the '-' and
                            // reports the WHOLE argument
                            let _ = writeln!(
                                stderr,
                                "{}history: {arg}: history position out of range",
                                executor.diagnostic_prefix()
                            );
                            return Ok(1);
                        }
                    }
                } else {
                    match parse_pos(arg) {
                        Some(index) if index >= 0 && (index as usize) < shell.entries.len() => {
                            shell.entries.remove(index as usize);
                            true
                        }
                        _ => {
                            if arg.parse::<i64>().is_err() {
                                let _ = writeln!(
                                    stderr,
                                    "{}history: {arg}: invalid number",
                                    executor.diagnostic_prefix()
                                );
                            } else {
                                let _ = writeln!(
                                    stderr,
                                    "{}history: {arg}: history position out of range",
                                    executor.diagnostic_prefix()
                                );
                            }
                            return Ok(1);
                        }
                    }
                };
                if !ok {
                    return Ok(1);
                }
            }
        }
        HistoryMode::Save => {
            // history.def:425-448: `-s` pops the last entry only when it
            // was added by the command line currently executing —
            // remember_on_history (set -o history) && hist_last_line_added
            // && !hist_last_line_pushed. A non-interactive shell never
            // pops, and a second -s in one line never pops the first -s's
            // entry. The push goes through check_add_history (FORCE=1),
            // i.e. record() with HISTCONTROL/HISTIGNORE honored.
            let remember =
                crate::builtins::set::shell_option_enabled(&executor.shell_state.env_vars, "history");
            if remember
                && !shell.last_line_pushed
                && shell.last_line_added
                && !shell.entries.is_empty()
            {
                shell.entries.pop();
            }
            let command = operands.join(" ");
            if !command.is_empty() {
                let control = executor.get_env("HISTCONTROL").unwrap_or_default();
                let ignore = executor.get_env("HISTIGNORE").unwrap_or_default();
                shell.record(&command, &control, &ignore, histsize);
                shell.last_line_pushed = true;
            }
        }
        HistoryMode::Print => {
            // history.def:462-463: `-p` pops the current-line entry under
            // the same conditions as -s; failure to delete is a hard error.
            let remember =
                crate::builtins::set::shell_option_enabled(&executor.shell_state.env_vars, "history");
            if remember && !shell.last_line_pushed && shell.last_line_added {
                if shell.entries.is_empty() {
                    return Ok(1);
                }
                shell.entries.pop();
            }
            for operand in &operands {
                let result = shell.expand(operand, ctx);
                if result.status < 0 {
                    let _ = writeln!(
                        stderr,
                        "{}history: {operand}: history expansion failed",
                        executor.diagnostic_prefix()
                    );
                    continue;
                }
                let _ = writeln!(stdout, "{}", result.text);
            }
        }
        HistoryMode::Append | HistoryMode::Write | HistoryMode::Read | HistoryMode::ReadNew => {
            // history.def:159,273: filename is the first non-option argument
            // (loptend); fall back to $HISTFILE when none is given.
            let Some(path) = operands
                .first()
                .cloned()
                .or_else(|| executor.get_env("HISTFILE").map(String::from))
            else {
                let _ = writeln!(
                    stderr,
                    "{}history: filename not specified",
                    executor.diagnostic_prefix()
                );
                return Ok(2);
            };
            // Translate Git-Bash/POSIX spellings (/c/..., /dev/null) to the
            // Windows forms the file APIs need; children see the same file.
            let path = crate::executor::path::shell_path_to_windows(&path, &executor.shell_state.env_vars)
                .to_string_lossy()
                .to_string();
            let outcome = match mode {
                HistoryMode::Append => shell.append_file(&path).map(|_| 0),
                HistoryMode::Write => shell.write_file(&path).map(|_| {
                    shell.lines_this_session = 0;
                    0
                }),
                HistoryMode::Read => shell.read_file(&path, histsize).map(|_| 0),
                HistoryMode::ReadNew => shell.read_new_file(&path, histsize).map(|_| 0),
                _ => Ok(0),
            };
            if let Err(err) = outcome {
                let _ = writeln!(
                    stderr,
                    "{}history: {path}: cannot open: {err}",
                    executor.diagnostic_prefix()
                );
                return Ok(1);
            }
        }
        HistoryMode::List => {
            let base = shell.base;
            let entries = shell.entries.clone();
            let start = match count {
                Some(n) => entries.len().saturating_sub(n),
                None => 0,
            };
            for (index, entry) in entries.iter().enumerate().skip(start) {
                // GNU history.def:407-408: %5d%c %s with the continuation
                // marker "*" for entries carrying multi-line histdata.
                // histdata is only set for interactive continuations;
                // script entries always print the space marker (history.def:408).
                let mark = " ";
                let _ = writeln!(stdout, "{:>5}{} {}", base + index, mark, entry);
            }
        }
    }

    Ok(0)
}

use crate::executor::ExecuteError as ExecuteErrorAlias;
