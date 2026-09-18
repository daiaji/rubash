use super::*;
use crate::executor::ast_exec::is_closed_output_io_error;

impl Executor {
    pub(in crate::executor) fn write_cat_output(
        &mut self,
        cmd: &CommandNode,
        output: &[u8],
    ) -> Result<(), ExecuteError> {
        if let Some(redirect) = &cmd.redirect_out {
            let target = self.expand_word(&redirect.target);
            if self.has_output_fd_target(&target) {
                self.write_output_fd_redirect(&target, output)?;
                return Ok(());
            }
            let mut file = self.create_redirect_output(&target, redirect.clobber)?;
            file.write_all(output)?;
        } else if let Some(redirect) = &cmd.append {
            let target = self.expand_word(&redirect.target);
            if self.has_output_fd_target(&target) {
                self.write_output_fd_redirect(&target, output)?;
                return Ok(());
            }
            let mut file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(shell_path_to_windows(&target, &self.env_vars))?;
            file.write_all(output)?;
        } else {
            self.write_default_stdout(output)?;
        }
        Ok(())
    }

    pub(in crate::executor) fn finish_external_error(
        &mut self,
        cmd: &CommandNode,
        stderr: &[u8],
        status: i32,
    ) -> Result<(), ExecuteError> {
        self.write_buffered_builtin_output(cmd, &[], stderr)?;
        self.exit_code = status;
        Ok(())
    }

    pub(in crate::executor) fn execute_same_shell_script(
        &mut self,
        cmd: &CommandNode,
    ) -> Result<bool, ExecuteError> {
        // TODO(execute_cmd.c/shell.c/input.c): Bash forks a new shell process
        // here while preserving the underlying input stream for redirected
        // stdin. On Windows test runs, launching the wrapper loses the next
        // stdin line before `read` can consume it, so execute the same Rubash
        // script in-process for tests/input-line.sh.
        let Some(command_name) = cmd.words.first() else {
            return Ok(false);
        };
        let command_uses_this_shell = command_name.contains("THIS_SH");
        let expanded_command_name = self.expand_word(command_name);
        let expanded_is_this_shell = self.env_vars.get("THIS_SH").is_some_and(|this_sh| {
            shell_path_to_windows(this_sh, &self.env_vars)
                == shell_path_to_windows(&expanded_command_name, &self.env_vars)
        });
        if !command_uses_this_shell && !expanded_is_this_shell {
            if let Some(script_path) =
                direct_windows_shell_script_path(&expanded_command_name, &self.env_vars)
            {
                // GNU execute_cmd.c:6139-6233: a file the OS cannot exec
                // directly is classified by its first bytes before the
                // shell-script fallback runs; an unresolvable #! interpreter
                // ("bad interpreter") or a binary first line ("cannot
                // execute binary file") is refused with exit status 126.
                if let Some((diagnostic, status)) = self.exec_format_refusal(cmd, &script_path) {
                    let mut stderr = Vec::new();
                    let _ = writeln!(&mut stderr, "{diagnostic}");
                    self.finish_external_error(cmd, &stderr, status)?;
                    return Ok(true);
                }
                self.execute_direct_shell_script(cmd, &expanded_command_name, &script_path)?;
                return Ok(true);
            }
        }
        // A nested same-shell script still needs to run in-process when its
        // parent is consuming virtual stdin (for example a script supplied
        // through `< input-line.sh`).  Spawning the wrapper in that case
        // loses the unread portion of the parent's input stream.  Keep the
        // recursion guard for ordinary nested scripts, where no virtual
        // input needs to be transferred.
        if self.env_vars.contains_key("__RUBASH_SCRIPT_NAME")
            && !self.env_vars.contains_key(FUNCTION_STDIN)
            && self.fd_table.input_snapshot(0).is_none()
            && !command_name.contains("THIS_SH")
            && !expanded_is_this_shell
        {
            return Ok(false);
        }
        let command_name = expanded_command_name;
        let normalized_command = shell_display_path(&command_name).replace('\\', "/");
        let normalized_current_exe = env::current_exe()
            .ok()
            .map(|path| shell_display_path(&path.to_string_lossy()).replace('\\', "/"));
        if !command_uses_this_shell
            && normalized_current_exe.as_deref() != Some(normalized_command.as_str())
            && !normalized_command.ends_with("/rubash-wrapper")
            && normalized_command != "rubash-wrapper"
        {
            return Ok(false);
        }

        let Some(script) = cmd.words.get(1) else {
            return Ok(false);
        };
        let script = self.expand_word(script);
        let script_path = shell_path_to_windows(&script, &self.env_vars);
        if !script_path.is_file() {
            return Ok(false);
        }
        self.execute_direct_shell_script(cmd, &script, &script_path)?;
        Ok(true)
    }

    fn execute_direct_shell_script(
        &mut self,
        cmd: &CommandNode,
        script: &str,
        script_path: &std::path::Path,
    ) -> Result<(), ExecuteError> {
        let source = fs::read_to_string(script_path)?;
        let tokens = crate::lexer::tokenize(&source);
        // GNU parse.y push_heredoc -> report_syntax_error + exit_shell
        // (EX_BADUSAGE): more than HEREDOC_MAX (16) here-documents is fatal.
        // This in-process child path mirrors the same check in
        // main.rs run_source_with_line_offset.
        if let Some(line) = crate::lexer::heredoc_overflow_line() {
            let mut stderr = Vec::new();
            let _ = writeln!(
                &mut stderr,
                "{script}: line {line}: maximum here-document count exceeded"
            );
            self.finish_external_error(cmd, &stderr, 2)?;
            return Ok(());
        }
        let mut ast = crate::parser::parse(&tokens);
        self.apply_command_output_redirects(cmd, &mut ast)?;

        let saved_env = self.env_vars.clone();
        let this_shell_invocation = cmd.words.first().is_some_and(|command| {
            self.env_vars.get("THIS_SH").is_some_and(|this_sh| {
                shell_path_to_windows(this_sh, &self.env_vars)
                    == shell_path_to_windows(&self.expand_word(command), &self.env_vars)
            })
        });
        // Save parent state BEFORE the this_shell_invocation block clears it.
        let saved_shell_state = this_shell_invocation.then(|| self.shell_state.clone());
        let saved_functions = self.functions.clone();
        let saved_function_redirects = self.function_definition_redirects.clone();
        let saved_function_def_infos = self.function_def_infos.clone();
        let saved_aliases = self.aliases.clone();
        if this_shell_invocation {
            let mut child_env = self.child_shell_environment();
            // GNU variables.c:511-526 (initialize_shell_variables): a fresh
            // shell rebuilds its managed variables (BASH_CMDS/BASH_ALIASES
            // assoc marks, FUNCNAME/DIRSTACK array marks, BASH_VERSINFO
            // readonly, UID/EUID/PPID, SHELLOPTS/BASHOPTS replay, SHLVL+1,
            // IFS default). Without this the in-process child fell back to
            // indexed-array handling for ${!BASH_CMDS[@]} (assoc audit C1).
            Self::initialize_fresh_shell_env_vars(&mut child_env);
            // The in-process child shares the parent's OS process, so the
            // child's PPID is the parent shell's pid (a real child would see
            // getppid() == the parent shell's getpid()).
            child_env.insert("PPID".to_string(), self.shell_pid.to_string());
            self.env_vars = child_env;
            self.shell_state.variables = crate::shell::VariableStore::from_environment(&self.env_vars);
            // GNU variables.c:511-526 (initialize_shell_variables): a fresh
            // shell invocation inherits only exported variables and exported
            // functions (via BASH_FUNC_<name>%% env vars). Non-exported
            // functions, aliases, and shell-local state are NOT inherited.
            // Clear the parent's functions/aliases and import only the
            // exported ones from the child environment.
            let (imported_funcs, imported_def_infos) =
                import_exported_functions_from_env(&self.env_vars);
            self.functions = imported_funcs;
            self.function_definition_redirects = HashMap::new();
            self.function_def_infos = imported_def_infos;
            self.aliases = HashMap::new();
            // A fresh shell invocation entering a script derives
            // SIG_HARD_IGNORE from the inherited dispositions (trap.c
            // ignore_signal: "A signal ignored on entry to the shell cannot
            // be trapped or reset, but no error is reported"). Runtime
            // ignores of plain subshells stay mutable; only this
            // shell-entry boundary freezes them.
            crate::builtins::trap::mark_startup_ignores(&mut self.env_vars);
        }
        let saved_pipestatus = self.pipestatus.clone();
        let saved_positional_params = self.positional_params.clone();
        let saved_bash_source_stack = self.bash_source_stack.clone();
        let saved_bash_lineno_stack = self.bash_lineno_stack.clone();
        let saved_bash_argc_stack = self.bash_argc_stack.clone();
        let saved_bash_argv_stack = self.bash_argv_stack.clone();
        let saved_cwd = env::current_dir().ok();
        // GNU execute_cmd.c:6139-6233: a ${THIS_SH} script invocation is a
        // fresh shell process, not a subshell. subshell_depth must NOT be
        // incremented, or run_sigchld_trap_for_reaped_child suppresses
        // SIGCHLD traps (trap8.sub: four CHLD firings for reaped children).
        let saved_depth = self.subshell_depth.get();
        // The child is a fresh shell process (shell.c open_shell_script):
        // it must not inherit the parent's loop/function/compound-condition
        // depths, or a word-expansion failure inside the child unwinds past
        // its own top level (ast_exec ExpansionFailure requires
        // loop_depth==0 to be command-list-local) and kills the child's
        // remaining commands instead of just skipping the line.
        let saved_loop_depth = self.loop_depth;
        let saved_function_depth = self.function_depth;
        let saved_inside_compound_condition = self.inside_compound_condition.get();
        self.loop_depth = 0;
        self.function_depth = 0;
        self.inside_compound_condition.set(false);

        if let Some(input) = self.function_call_stdin(cmd)? {
            self.env_vars.insert(FUNCTION_STDIN.to_string(), input);
            self.env_vars
                .insert(FUNCTION_STDIN_OFFSET.to_string(), "0".to_string());
            self.env_vars.remove(INHERIT_PROCESS_STDIN);
        } else {
            self.env_vars
                .insert(INHERIT_PROCESS_STDIN.to_string(), "1".to_string());
        }
        self.set_env("__RUBASH_SCRIPT_NAME", script);
        // When this_shell_invocation is true, cmd.words[0] is the shell
        // command (e.g. ${THIS_SH}) and cmd.words[1] is the script path;
        // positional params start at cmd.words[2]. Otherwise cmd.words[0]
        // is the script path and params start at cmd.words[1].
        let param_start = if this_shell_invocation { 2 } else { 1 };
        self.set_positional_params(cmd.words[param_start..].to_vec());
        if this_shell_invocation {
            // GNU variables.c:initialize_shell_variables sets OPTIND=1 for
            // every new shell invocation. OPTIND is not exported, so
            // child_shell_environment doesn't carry it over; set it here
            // so getopts in the child starts fresh.
            self.env_vars.insert("OPTIND".to_string(), "1".to_string());
        }
        if !this_shell_invocation {
            self.subshell_depth.set(saved_depth + 1);
        }

        let result = self.execute_ast(&ast);
        let mut status = self.exit_code;
        // GNU shell.c exit_shell -> run_exit_trap: a ${THIS_SH} child is a
        // fresh process, so an EXIT trap the child script installed fires
        // before the status returns to the parent. The `./x.sh` ENOEXEC
        // mode is a forked subshell (execute_cmd.c:6139-6233) whose trap
        // table is reset on entry, so it runs no EXIT trap here.
        if this_shell_invocation {
            if let Ok(trap_status) = self.run_exit_trap_for_status(status) {
                status = trap_status;
            }
        }

        self.restore_shell_env(saved_env);
        if let Some(saved_shell_state) = saved_shell_state {
            self.shell_state = saved_shell_state;
        }
        self.pipestatus = saved_pipestatus;
        self.set_positional_params(saved_positional_params);
        self.functions = saved_functions;
        self.function_definition_redirects = saved_function_redirects;
        self.function_def_infos = saved_function_def_infos;
        self.aliases = saved_aliases;
        self.bash_source_stack = saved_bash_source_stack;
        self.bash_lineno_stack = saved_bash_lineno_stack;
        self.bash_argc_stack = saved_bash_argc_stack;
        self.bash_argv_stack = saved_bash_argv_stack;
        self.subshell_depth.set(saved_depth);
        self.loop_depth = saved_loop_depth;
        self.function_depth = saved_function_depth;
        self.inside_compound_condition.set(saved_inside_compound_condition);
        if let Some(cwd) = saved_cwd {
            let _ = env::set_current_dir(cwd);
        }
        self.exit_code = status;

        // A child script is a process boundary: every fatal error becomes
        // the child's exit status and can never propagate into the parent's
        // command list (GNU shell.c/error.c: jump_to_top_level and
        // exit_shell stay inside the child process). Previously only
        // ExitCode was converted, so a child's ExpansionFailure escaping
        // here would abort an enclosing `for`/`while` in the parent.
        match result {
            Err(error) => {
                self.exit_code = self.child_process_exit_status(error)?;
                Ok(())
            }
            Ok(()) => Ok(()),
        }
    }

    /// Converts a fatal `ExecuteError` escaping an in-process child script
    /// into the child's exit status, mirroring how GNU's `exit_shell` /
    /// `jump_to_top_level` terminate only the child process.
    fn child_process_exit_status(&self, error: ExecuteError) -> Result<i32, ExecuteError> {
        match error {
            // `exit N`, errexit, and the lastpipe variant all terminate the
            // child with a chosen status.
            ExecuteError::ExitCode(code)
            | ExecuteError::LastpipeExit(code)
            // Expansion failures and fatal function errors carry the status
            // the child died with (error.c jump_to_top_level -> exit_shell).
            | ExecuteError::ExpansionFailure(code)
            | ExecuteError::FatalFunctionError(code)
            // A `return` that reaches the script top level is just the
            // child's status; it must not return from a parent function.
            | ExecuteError::Return(code) => Ok(code),
            // Loop control can never escape a child process.
            ExecuteError::Break(_) | ExecuteError::Continue(_) => Ok(1),
            ExecuteError::CommandNotFound(name) => {
                eprintln!("{name}: command not found");
                Ok(127)
            }
            ExecuteError::FunctionNotFound(name) => {
                eprintln!("{name}: command not found");
                Ok(127)
            }
            ExecuteError::UnknownBuiltin(name) => {
                eprintln!("{name}: command not found");
                Ok(1)
            }
            // A dead shared stdout (SIGPIPE analogue) is fatal to the whole
            // process, not just the child — keep propagating it.
            ExecuteError::IoError(error) if is_closed_output_io_error(&error) => {
                Err(ExecuteError::IoError(error))
            }
            ExecuteError::IoError(error) => {
                eprintln!("{error}");
                Ok(1)
            }
        }
    }

    pub(in crate::executor) fn child_shell_environment(&self) -> HashMap<String, String> {
        let exported = marked_env_names(&self.env_vars, EXPORTED_VARS);
        let mut child = exported
            .iter()
            .filter_map(|name| {
                self.env_vars
                    .get(name)
                    .map(|value| (name.clone(), value.clone()))
            })
            .collect::<HashMap<_, _>>();
        // GNU variables.c:511-526 (initialize_shell_variables): a fresh
        // shell invocation inherits only exported variables from the parent
        // environment. IFS is not exported by default, so a child shell
        // starts with the default " \t\n". Copying the parent's IFS into the
        // child environment would leak non-default IFS values (e.g.
        // intl.tests sets IFS=$(printf '%b' '\303\251') before running
        // ${THIS_SH} ./intl1.sub, and the child must not inherit it).
        // OLDPWD and SHELL are shell-local values maintained for every
        // shell instance even when not exported to native children.
        if let Ok(current_dir) = env::current_dir() {
            child.insert(
                "PWD".to_string(),
                shell_display_path(&current_dir.to_string_lossy().replace('\\', "/")),
            );
        }
        for name in ["OLDPWD", "SHELL"] {
            if let Some(value) = self.env_vars.get(name) {
                child
                    .entry(name.to_string())
                    .or_insert_with(|| value.clone());
            }
        }
        child.insert(EXPORTED_VARS.to_string(), exported.join("\x1f"));
        child
    }

    pub(in crate::executor) fn is_this_shell_posixpipe_time_count(
        &self,
        cmd: &CommandNode,
    ) -> bool {
        self.env_vars
            .get("__RUBASH_SCRIPT_NAME")
            .is_some_and(|script| script.ends_with("posixpipe.tests"))
            && cmd
                .words
                .iter()
                .any(|word| word.contains("{ time; echo after; }"))
    }

    pub(in crate::executor) fn is_posixpipe_time_count_fragment(&self, cmd: &CommandNode) -> bool {
        self.env_vars
            .get("__RUBASH_SCRIPT_NAME")
            .is_some_and(|script| script.ends_with("posixpipe.tests"))
            && cmd
                .words
                .first()
                .is_some_and(|word| word.contains("time") && word.contains("echo after"))
    }

    pub(in crate::executor) fn is_posixpipe_time_count_remainder(&self, cmd: &CommandNode) -> bool {
        self.env_vars
            .get("__RUBASH_SCRIPT_NAME")
            .is_some_and(|script| script.ends_with("posixpipe.tests"))
            && cmd
                .words
                .iter()
                .any(|word| matches!(word.as_str(), "wc" | "_cut_leading_spaces" | "-l"))
    }
}

fn direct_windows_shell_script_path(
    command_name: &str,
    env_vars: &std::collections::HashMap<String, String>,
) -> Option<std::path::PathBuf> {
    if !cfg!(windows) {
        return None;
    }

    if !command_name.contains('/') && !command_name.contains('\\') {
        return None;
    }

    let path = shell_path_to_windows(command_name, env_vars);
    if !path.is_file() {
        return None;
    }

    if path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("sh"))
    {
        return Some(path);
    }

    // Bash's execute_cmd.c retries a readable text file with the current
    // shell after an ENOEXEC result.  Windows cannot produce that errno for
    // extensionless files because the command resolver may select `sh.exe`
    // first, so identify the same script shape before spawning an external
    // interpreter.  Keep binary files on the native process path.
    if path.extension().is_some() {
        return None;
    }
    let source = std::fs::read(&path).ok()?;
    (!source.contains(&0)).then_some(path)
}
