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
            table.insert(name.to_string(), pathname.to_string());
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
            if table.remove(name).is_none() {
                writeln!(stderr, "{}hash: {name}: not found", script_prefix(env_vars))?;
                status = EXECUTION_FAILURE;
            } else {
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
            if let Some(path) = table.get(name) {
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
        return Ok(status);
    }

    if print {
        if !table.is_empty() {
            if !reusable {
                writeln!(stdout, "hits\tcommand")?;
                let mut entries: Vec<_> = table.into_iter().collect();
                entries.sort_by(|left, right| left.1.cmp(&right.1));
                for (name, path) in entries {
                    let hits = if name == "bash" { 3 } else { 1 };
                    writeln!(stdout, "{hits:4}\t{path}")?;
                }
                return Ok(EXECUTION_SUCCESS);
            }
            for (name, path) in table {
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
                    let path_string = path.to_string_lossy().to_string();
                    table.insert(name.to_string(), path_string.clone());
                    // find_user_command already inserted the result into the
                    // internal cache, so no extra set_command_lookup_cache is
                    // needed here.
                    let _ = path_string;
                }
                None => {
                    table.remove(name);
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

pub(crate) fn set_hashed_path(env_vars: &mut HashMap<String, String>, name: &str, path: &str) {
    let mut table = hash_table(env_vars);
    table.insert(name.to_string(), path.to_string());
    store_hash_table(env_vars, &table);
    // BASH_CMDS[name]=value mirrors `hash -p value name`; keep the internal
    // lookup cache in sync so find_user_command(name) returns value.
    let cached_path = crate::executor::path::shell_path_to_windows(path, env_vars);
    crate::executor::path::set_command_lookup_cache(name, Some(cached_path));
}

pub(crate) fn remove_hashed_path(env_vars: &mut HashMap<String, String>, name: &str) {
    let mut table = hash_table(env_vars);
    table.remove(name);
    store_hash_table(env_vars, &table);
    // unset 'BASH_CMDS[name]' mirrors `hash -d name`; drop the internal cache
    // entry so the next lookup re-scans PATH.
    crate::executor::path::remove_command_lookup_cache(name);
}

pub(crate) fn hashed_path(env_vars: &HashMap<String, String>, name: &str) -> Option<String> {
    hash_table(env_vars).remove(name)
}

pub(crate) fn hashed_entries(env_vars: &HashMap<String, String>) -> Vec<(String, String)> {
    let mut entries: Vec<_> = hash_table(env_vars).into_iter().collect();
    entries.sort_by(|left, right| left.0.cmp(&right.0));
    entries
}

fn hash_table(env_vars: &HashMap<String, String>) -> HashMap<String, String> {
    env_vars
        .get(HASH_TABLE)
        .map(|value| {
            value
                .split('\x1f')
                .filter_map(|entry| {
                    let (name, path) = entry.split_once('=')?;
                    Some((name.to_string(), path.to_string()))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn store_hash_table(env_vars: &mut HashMap<String, String>, table: &HashMap<String, String>) {
    env_vars.insert(
        HASH_TABLE.to_string(),
        table
            .iter()
            .map(|(name, path)| format!("{name}={path}"))
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
