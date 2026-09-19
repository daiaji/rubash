use super::*;

impl Executor {
    pub(in crate::executor) fn execute_declare_functions(
        &mut self,
        args: &[String],
        stdout: &mut impl Write,
        stderr: &mut impl Write,
    ) -> io::Result<i32> {
        // TODO(builtins/declare.def/execute_cmd.c): Bash prints the stored
        // function COMMAND tree. Rubash currently stores only parsed command
        // bodies, so render the simple function form used by builtins6.sub.
        // GNU declare.def declare_invalid_opts: att_function combined with
        // att_array/att_assoc/att_integer/att_nameref is a usage error.
        let function_flag_on = args
            .iter()
            .any(|arg| arg.starts_with('-') && arg.contains('f'));
        if function_flag_on {
            let mut bad_opt: Option<&str> = None;
            for arg in args {
                if !arg.starts_with('-') {
                    continue;
                }
                if arg.contains('n') {
                    bad_opt = Some("-n");
                    break;
                }
                if arg.contains('i') {
                    bad_opt = Some("-i");
                    break;
                }
                if arg.contains('A') {
                    bad_opt = Some("-A");
                    break;
                }
                if arg.contains('a') {
                    bad_opt = Some("-a");
                    break;
                }
            }
            if let Some(opt) = bad_opt {
                writeln!(
                    stderr,
                    "{}declare: {}: invalid option",
                    self.diagnostic_prefix(),
                    opt
                )?;
                return Ok(1);
            }
        }
        // GNU declare.def:465-473: when -f is used with an assignment
        // (name=value), find_function is called with the full name string
        // (including '=value'), which never matches a function name, so
        // "cannot use `-f' to make functions" is always reported.
        if function_flag_on {
            for arg in args {
                if !arg.starts_with('-') && !arg.starts_with('+') && arg.contains('=') {
                    writeln!(
                        stderr,
                        "{}declare: cannot use `-f' to make functions",
                        self.diagnostic_prefix()
                    )?;
                    return Ok(1);
                }
            }
        }
        let names: Vec<&str> = args
            .iter()
            .filter(|arg| !arg.starts_with('-') && !arg.starts_with('+'))
            .map(String::as_str)
            .collect();
        let print_not_found = args.iter().any(|arg| arg == "-p");
        let function_names_only = args
            .iter()
            .any(|arg| arg.starts_with('-') && arg.contains('F'));
        let function_definition_mode = args
            .iter()
            .any(|arg| (arg.starts_with('-') || arg.starts_with('+')) && arg.contains('f'));
        let set_export = args
            .iter()
            .any(|arg| arg.starts_with('-') && arg.contains('x'));
        let clear_export = args
            .iter()
            .any(|arg| arg.starts_with('+') && arg.contains('x'));
        let set_export_attribute = set_export && function_definition_mode;
        let clear_export_attribute = clear_export && function_definition_mode;
        let exported_only = set_export;
        let readonly = args
            .iter()
            .any(|arg| arg.starts_with('-') && arg.contains('r'));
        let clear_readonly = args
            .iter()
            .any(|arg| arg.starts_with('+') && arg.contains('r'));
        // GNU declare.def declares -t on functions: set/clear the trace
        // attribute (trace_p(var) in execute_cmd.c). A traced function
        // inherits the DEBUG and RETURN traps even with functrace off.
        let set_trace = args
            .iter()
            .any(|arg| arg.starts_with('-') && arg.contains('t') && arg.contains('f'));
        let clear_trace = args
            .iter()
            .any(|arg| arg.starts_with('+') && arg.contains('t') && arg.contains('f'));
        let print = args
            .iter()
            .any(|arg| arg.starts_with('-') && arg.contains('p'));
        let exported_functions = marked_env_names(&self.env_vars, EXPORTED_FUNCTIONS);
        let readonly_functions = marked_env_names(&self.env_vars, READONLY_FUNCTIONS);
        if names.is_empty() {
            let mut functions: Vec<_> = self.functions.iter().collect();
            functions.sort_by(|(left, _), (right, _)| left.cmp(right));
            for (name, body) in functions {
                if exported_only && !exported_functions.iter().any(|exported| *exported == *name) {
                    continue;
                }
                if readonly && !readonly_functions.iter().any(|rn| *rn == *name) {
                    continue;
                }
                if function_names_only {
                    let mut flags = String::from("-f");
                    if readonly {
                        flags.push('r');
                    }
                    if exported_only {
                        flags.push('x');
                    }
                    writeln!(stdout, "declare {flags} {name}")?;
                } else {
                    self.write_function_definition(name, &body.commands, exported_only, stdout)?;
                }
            }
            return Ok(0);
        }
        let mut status = 0;
        for name in names {
            let Some(body) = self.functions.get(name) else {
                if print_not_found {
                    writeln!(
                        stderr,
                        "{}declare: {name}: not found",
                        self.diagnostic_prefix()
                    )?;
                }
                status = 1;
                continue;
            };
            let is_exported = exported_functions.iter().any(|exported| exported == name);
            let is_readonly = readonly_functions.iter().any(|rn| rn == name);
            // GNU declare.def: clearing readonly on a readonly function is
            // an error ("readonly function"), not a silent success.
            if clear_readonly && is_readonly {
                writeln!(
                    stderr,
                    "{}declare: {name}: readonly function",
                    self.diagnostic_prefix()
                )?;
                status = 1;
                continue;
            }
            if exported_only && !is_exported && !set_export_attribute {
                continue;
            }
            if clear_export_attribute {
                unmark_env_name(&mut self.env_vars, EXPORTED_FUNCTIONS, name);
                if !print {
                    continue;
                }
            } else if set_export_attribute {
                mark_env_name(&mut self.env_vars, EXPORTED_FUNCTIONS, name);
                if !print && !function_names_only {
                    continue;
                }
            }
            if readonly {
                mark_env_name(&mut self.env_vars, READONLY_FUNCTIONS, name);
                if !print {
                    continue;
                }
            }
            if set_trace {
                mark_env_name(&mut self.env_vars, FUNC_TRACE_FUNCTIONS, name);
                if !print {
                    continue;
                }
            }
            if clear_trace {
                unmark_env_name(&mut self.env_vars, FUNC_TRACE_FUNCTIONS, name);
                if !print {
                    continue;
                }
            }
            if function_names_only {
                if exported_only {
                    writeln!(stdout, "declare -fx {name}")?;
                } else {
                    self.write_function_name(name, stdout)?;
                }
            } else {
                self.write_function_definition(
                    name,
                    &body.commands,
                    exported_only && is_exported,
                    stdout,
                )?;
            }
        }
        Ok(status)
    }

    fn write_function_name<W>(&self, name: &str, stdout: &mut W) -> io::Result<()>
    where
        W: Write,
    {
        if crate::builtins::shopt::option_enabled(&self.env_vars, "extdebug") {
            if let Some(location) = self.function_definition_locations.get(name) {
                return writeln!(stdout, "{} {} {}", name, location.line, location.source);
            }
        }
        writeln!(stdout, "{name}")
    }

    /// GNU declare.def:429,1004: a `declare name[sub]=value` operand carries
    /// W_ASSIGNMENT, so with `array_expand_once` the already-expanded
    /// subscript text is ASS_NOEXPAND verbatim data (a surviving
    /// `$name`/`$(...)` is expr.c "operand expected", not a second
    /// execution); without the option assign_array_element re-expands it via
    /// expand_arith_string. Rewrite each `name[sub](+)=value` operand LHS to
    /// its final `name[index]` / `name[\x1ekey]` form so the env-only
    /// builtin consumes resolved data. Err(()) means the operand-expected
    /// diagnostic + evalerror abort (array_expand_index ->
    /// jump_to_top_level DISCARD) were already raised; the caller supplies
    /// status 1.
    /// GNU general.c:480 assignment() applied to the RAW word token:
    /// whether the operand carried W_ASSIGNMENT. The name part must be
    /// legal variable characters — any quote or escape char rejects it — or
    /// a well-formed `name[subscript]` prefix before `=`/`+=`.
    pub(in crate::executor) fn raw_word_is_assignment(raw: &str) -> bool {
        let bytes = raw.as_bytes();
        match bytes.first() {
            Some(&c) if is_shell_name_start(c as char) => {}
            _ => return false,
        }
        let mut index = 0usize;
        while index < bytes.len() {
            match bytes[index] {
                b'=' => return index > 0,
                b'[' => {
                    let Some((sub_end, _)) =
                        crate::executor::subscript_expansion::scan_compound_subscript(raw, index)
                    else {
                        return false;
                    };
                    index = sub_end + 1;
                    if bytes.get(index) == Some(&b'+') && bytes.get(index + 1) == Some(&b'=') {
                        return true;
                    }
                    return bytes.get(index) == Some(&b'=');
                }
                b'+' if bytes.get(index + 1) == Some(&b'=') => return true,
                c if is_shell_name_char(c as char) => index += 1,
                _ => return false,
            }
        }
        false
    }

    pub(in crate::executor) fn rewrite_declare_operand_subscripts(
        &mut self,
        args: &[String],
        word_metadata: &[crate::parser::WordMetadata],
        command_name: &str,
    ) -> Result<Vec<String>, ()> {
        // Whether the operand list carries `-A` (set form); an unmarked
        // variable declared with -A takes the assoc element rules.
        let mut parse_options = true;
        let assoc_hint = args.iter().any(|arg| {
            if parse_options && arg == "--" {
                parse_options = false;
                return false;
            }
            parse_options && arg.starts_with('-') && arg != "-" && arg[1..].contains('A')
        });
        // Same scan for the indexed `-a` flag: GNU creating_array covers
        // both att_array and att_assoc (declare.def:813).
        let mut parse_options = true;
        let array_hint = args.iter().any(|arg| {
            if parse_options && arg == "--" {
                parse_options = false;
                return false;
            }
            parse_options && arg.starts_with('-') && arg != "-" && arg[1..].contains('a')
        });
        // Option words of this invocation, for replaying earlier operands
        // into the scratch environment below (the builtin applies `-a`/`-A`
        // to every operand it binds).
        let mut parse_options = true;
        let option_args: Vec<String> = args
            .iter()
            .filter(|arg| {
                if *arg == "--" {
                    parse_options = false;
                    return false;
                }
                parse_options
                    && (arg.starts_with('-') || arg.starts_with('+'))
                    && arg.as_str() != "-"
                    && arg.as_str() != "+"
            })
            .cloned()
            .collect();
        // GNU declare.def:697-831 runs the operand loop inside the builtin,
        // so a deferred compound operand expands with earlier operands
        // already bound (`declare -a a=('x y') d='($a)'` sees `a`). The
        // element resolution below needs the same ordering: replay the
        // already-rewritten earlier operands into a scratch env and expand
        // under it.
        let mut compound_env: Option<HashMap<String, String>> = None;
        let mut out_args: Vec<String> = Vec::with_capacity(args.len());
        for (index, arg) in args.iter().enumerate() {
            match self.rewrite_declare_operand(
                index,
                arg,
                word_metadata,
                command_name,
                assoc_hint,
                &option_args,
                &mut compound_env,
                &out_args,
                array_hint,
            ) {
                Ok(rewritten) => out_args.push(rewritten),
                // GNU declare.def operand loop: an evalexp failure inside a
                // `name[sub]` subscript aborts the REST of the operand list
                // (`declare -i a[$(bad)]=42 b=99` leaves b unset), but the
                // failed operand's base still goes through
                // making_array_special + VSETATTR (`declare -ai a=()`). The
                // builtin therefore sees a value-less `base[0]` hint for it
                // and nothing after.
                Err(()) => {
                    self.declare_compound_element_failed.set(true);
                    let lhs = arg.split_once('=').map(|(lhs, _)| lhs).unwrap_or(arg);
                    let base = lhs
                        .trim_end_matches('+')
                        .split('[')
                        .next()
                        .unwrap_or(lhs);
                    if !base.is_empty() {
                        // An existing array/assoc keeps its cell; a fresh
                        // name still materializes as an empty array
                        // (`declare -ai a=()`) because the assignment ran
                        // make_new_array_variable before the subscript
                        // failed (declare.def:953-962).
                        if is_marked_var(&self.env_vars, ASSOC_VARS, base)
                            || is_marked_var(&self.env_vars, ARRAY_VARS, base)
                            || self.env_vars.contains_key(base)
                        {
                            out_args.push(format!("{base}[0]"));
                        } else {
                            out_args.push(format!(
                                "{base}={COMPOUND_ASSIGNMENT_MARKER}()"
                            ));
                        }
                    }
                    return Ok(out_args);
                }
            }
        }
        Ok(out_args)
    }

    fn rewrite_declare_operand(
        &mut self,
        index: usize,
        arg: &str,
        word_metadata: &[crate::parser::WordMetadata],
        command_name: &str,
        assoc_hint: bool,
        option_args: &[String],
        compound_env: &mut Option<HashMap<String, String>>,
        prior_args: &[String],
        array_hint: bool,
    ) -> Result<String, ()> {
        {
                // GNU declare.def:429: assoc_noexpand requires W_ASSIGNMENT
                // on the operand word, and parse.y:5787 only sets it on an
                // UNQUOTED assignment-shaped token (general.c:480
                // assignment() rejects quoted/escaped name parts). A quoted
                // `declare "a[$x]=v"` operand therefore re-expands its
                // subscript in both option modes.
                let raw = word_metadata
                    .iter()
                    .find(|m| m.word_index == index + 1)
                    .map(|m| m.raw.as_str());
                let w_assignment = match raw {
                    Some(raw) => Self::raw_word_is_assignment(raw),
                    None => Self::raw_word_is_assignment(arg),
                };
                let Some((lhs, value)) = arg.split_once('=') else {
                    // `declare name[sub]` without `=` is a size-hint
                    // declaration; GNU discards the subscript text without
                    // evaluating it (declare.def:605 making_array_special).
                    return Ok(arg.to_string());
                };
                let (lhs, append) = lhs
                    .strip_suffix('+')
                    .map(|lhs| (lhs, true))
                    .unwrap_or((lhs, false));
                let compound = value
                    .strip_prefix(COMPOUND_ASSIGNMENT_MARKER)
                    .unwrap_or(value);
                let paren_value = compound.starts_with('(') && compound.ends_with(')');
                // GNU declare.def:935-951 (shell_compatibility_level > 43):
                // a `(...)` operand value is a compound assignment only when
                // the operand word carried W_COMPASSIGN (an unquoted `=(` —
                // the  deferred marker identifies a whole-quoted RHS
                // instead), or when the target is an existing array/assoc or
                // -a/-A is creating one. A quoted/expanded `(...)` RHS on a
                // scalar stays a literal string: `declare -- a='(1 2)'`
                // binds "(1 2)", `declare a1=$v` with v='(7 8)' likewise.
                let deferred = paren_value
                    && compound[1..]
                        .starts_with(crate::executor::types::DEFERRED_COMPOUND_BODY);
                let w_compassign =
                    value.starts_with(COMPOUND_ASSIGNMENT_MARKER) && !deferred;
                let creating_array = assoc_hint || array_hint;
                let array_exists = is_marked_var(&self.env_vars, ASSOC_VARS, lhs)
                    || is_marked_var(&self.env_vars, ARRAY_VARS, lhs);
                let is_compound =
                    paren_value && (w_compassign || creating_array || array_exists);
                if std::env::var_os("RUBASH_DEBUG_CA").is_some() {
                    eprintln!("[gate] arg={arg:?} value={value:?} paren={paren_value} def={deferred} wc={w_compassign} create={creating_array} exists={array_exists} -> {is_compound}");
                }
                if paren_value && !is_compound {
                    // Scalar verbatim: keep the compound text (and its
                    // protected carriers) as the literal value. The 
                    // deferred marker is parse-internal; drop it so the
                    // sq'd body stores the plain characters.
                    let inner = &compound[1..compound.len() - 1];
                    let inner = inner
                        .strip_prefix(crate::executor::types::DEFERRED_COMPOUND_BODY)
                        .unwrap_or(inner);
                    return Ok(format!(
                        "{lhs}{}=({inner})",
                        if append { "+" } else { "" }
                    ));
                }
                if lhs.contains('[') {
                    // GNU subst.c:3599-3605: `name[sub]=(list)` fails
                    // "cannot assign list to array member" before the
                    // subscript is ever evaluated — leave it alone.
                    if is_compound {
                        return Ok(arg.to_string());
                    }
                    // GNU declare.def:592-600 + tokenize_array_reference
                    // (arrayfunc.c:1288): the operand name's subscript must
                    // be a complete matched pair — `declare 'm[x[y]=a'`
                    // expands to `m[x[y]=a`, whose nested `[` leaves the
                    // subscript unterminated, so GNU reports
                    // `not a valid identifier`. Pass the operand through
                    // untouched so valid_declare_name reaches the same
                    // diagnostic (and later operands still run).
                    // declare.def:429,439: assoc_noexpand requires BOTH
                    // array_expand_once and W_ASSIGNMENT on the raw operand
                    // word; only then does assignment(name, 2) close the
                    // expanded subscript at the first `]`
                    // (`declare myarray["foo[bar"]=v` stores key `foo[bar`
                    // under assoc_expand_once). Otherwise
                    // tokenize_array_reference requires the matched-pair
                    // close at the end of the name.
                    let expand_once = w_assignment
                        && crate::builtins::shopt::option_enabled(
                            &self.env_vars,
                            "array_expand_once",
                        );
                    let subscript_ok = lhs.find('[').is_some_and(|open| {
                        if expand_once {
                            lhs[open + 1..]
                                .find(']')
                                .is_some_and(|close| open + 1 + close == lhs.len() - 1)
                        } else {
                            crate::executor::subscript_expansion::scan_compound_subscript(lhs, open)
                                .is_some_and(|(close, _)| close == lhs.len() - 1)
                        }
                    });
                    if !subscript_ok {
                        return Ok(arg.to_string());
                    }
                    // GNU declare.def:639-642,953-962: `declare name[sub]=v`
                    // (no -A flag) binds through the variable declare
                    // actually creates — at function scope that is a fresh
                    // LOCAL indexed array (making_array_special ->
                    // make_local_array_variable), which shadows even a
                    // global assoc of the same name, so the subscript is
                    // evaluated arithmetically. Only `-A` or an existing
                    // same-frame local assoc keeps the assoc path; at
                    // global scope the existing variable's type rules.
                    let base = lhs.split('[').next().unwrap_or(lhs);
                    let operand_assoc = if assoc_hint {
                        true
                    } else if self.function_depth > 0 {
                        is_marked_var(&self.env_vars, ASSOC_VARS, base)
                            && self
                                .local_var_scopes
                                .last()
                                .is_some_and(|scope| scope.contains_key(base))
                    } else {
                        is_marked_var(&self.env_vars, ASSOC_VARS, base)
                    };
                    let mode = if w_assignment {
                        OperandSubscriptMode::ExpandedOnce
                    } else {
                        OperandSubscriptMode::AlwaysExpand
                    };
                    let rewritten =
                        self.rewrite_operand_subscript_typed(lhs, mode, Some(operand_assoc))?;
                    if rewritten.ends_with("[]") {
                        // GNU arrayfunc.c array_expand_index: an empty
                        // subscript reports `name[]: bad array subscript`
                        // but the operand still ran making_array_special +
                        // VSETATTR — the variable materializes as an empty
                        // array (`declare -ar c=()`), the command fails,
                        // and later operands keep processing.
                        eprintln!(
                            "{}{rewritten}: bad array subscript",
                            self.diagnostic_prefix()
                        );
                        self.declare_compound_element_failed.set(true);
                        let base = rewritten.trim_end_matches("[]");
                        return Ok(format!("{base}={COMPOUND_ASSIGNMENT_MARKER}()"));
                    }
                    return Ok(format!(
                        "{rewritten}{}={value}",
                        if append { "+" } else { "" }
                    ));
                }
                if !is_compound {
                    return Ok(arg.to_string());
                }
                // `declare [-aA] name=(...)`: element subscripts inside the
                // stored compound text resolve through
                // expand_compound_array_assignment +
                // assign_compound_array_list (arrayfunc.c:557-836) — the
                // declare argument text was not pre-expanded by assignment
                // word expansion on this path, so the resolver runs its
                // non-preexpanded (declare) model.
                let assoc = assoc_hint || is_marked_var(&self.env_vars, ASSOC_VARS, lhs);
                let scratch = compound_env.get_or_insert_with(|| self.env_vars.clone());
                // Replay the earlier operands so the compound element
                // expansion sees their bindings, like declare.def's
                // left-to-right operand loop.
                let mut replay_args = option_args.to_vec();
                replay_args.extend(
                    prior_args
                        .iter()
                        .filter(|arg| {
                            !(arg.starts_with('-') || arg.starts_with('+'))
                                || arg.as_str() == "-"
                                || arg.as_str() == "+"
                        })
                        .cloned(),
                );
                let mut sink_out = Vec::new();
                let mut sink_err = Vec::new();
                let _ = crate::builtins::declare::execute_with_io_named_in_context(
                    command_name,
                    &replay_args,
                    scratch,
                    &mut sink_out,
                    &mut sink_err,
                    self.function_depth > 0,
                    &[],
                );
                std::mem::swap(&mut self.env_vars, scratch);
                let rewritten = match self.rewrite_compound_element_subscripts(
                    lhs, compound, assoc, false,
                ) {
                    Ok(rewritten) => rewritten,
                    // GNU assign_compound_array_list breaks on the failing
                    // element but binds the ones processed before it; the
                    // operand still carries the partial list. The command
                    // fails overall — recorded on the executor for the
                    // caller to merge into the exit status.
                    Err(partial) => {
                        self.declare_compound_element_failed.set(true);
                        partial
                    }
                };
                std::mem::swap(&mut self.env_vars, scratch);
                let marker = if value.starts_with(COMPOUND_ASSIGNMENT_MARKER) {
                    COMPOUND_ASSIGNMENT_MARKER
                } else {
                    ""
                };
                Ok(format!(
                    "{lhs}{}={marker}{rewritten}",
                    if append { "+" } else { "" }
                ))
        }
    }

    pub(in crate::executor) fn execute_declare(
        &mut self,
        cmd: &CommandNode,
    ) -> Result<i32, ExecuteError> {
        self.sync_dynamic_assoc_vars();
        // GNU materializes the DIRSTACK cell only when the command names
        // DIRSTACK itself; a bare list-all `declare -a` must keep the cell
        // empty (variables.c get_dirstack runs on named access only).
        if declare_words_name_dirstack(&cmd.words[1..]) {
            self.sync_dirstack_cell();
        }
        let mut args = self.expand_declare_assignment_args(&cmd.words[1..]);
        self.declare_compound_element_failed.set(false);
        let command_name = cmd.words.first().map(String::as_str).unwrap_or("declare");
        let mut args = match self.rewrite_declare_operand_subscripts(
            &args,
            &cmd.word_metadata,
            command_name,
        ) {
            Ok(args) => args,
            // array_expand_index -> evalexp failure: diagnostic + evalerror
            // abort already raised; GNU discards the rest of the list.
            Err(()) => return Ok(1),
        };
        let compound_element_failed = self.declare_compound_element_failed.replace(false);
        if declare_args_request_integer(&args) {
            args = self.evaluate_declare_integer_assignment_args(&args);
        }
        // GNU variables.c:2651-2665 (make_local_variable): a local may not
        // shadow a readonly binding at context 0 — "disallow local copies of
        // readonly global variables ... Readonly copies of calling function
        // local variables are OK". declare.def:669-673 then drops the whole
        // operand (any_failed++ + NEXT_VARIABLE): no local and no assignment.
        let local_blocked: Vec<String> = if self.function_depth > 0
            && !declare_args_force_global(&args)
            && !declare_args_request_print(&args)
        {
            self.global_readonly_local_blocks(&args)
        } else {
            Vec::new()
        };
        if !local_blocked.is_empty() {
            args.retain(|arg| {
                local_assignment_name(arg)
                    .map_or(true, |name| !local_blocked.iter().any(|b| b == name))
            });
        }
        // Names that were already local at this frame BEFORE this command's
        // save_local_names ran -- the `var->context == variable_context` test
        // in declare.def:655/850. Empty at global scope.
        let mut frame_locals: Vec<String> = Vec::new();
        if self.function_depth > 0
            && !declare_args_force_global(&args)
            && !declare_args_request_print(&args)
        {
            let prefix_assignment_names = cmd
                .assignment_keys()
                .map(|name| assignment_name_and_append(name).0.to_string())
                .collect::<Vec<_>>();
            let scope_keys_before_save: Vec<String> = self
                .local_var_scopes
                .last()
                .map(|scope| scope.keys().cloned().collect())
                .unwrap_or_default();
            let mut pre_existing: Vec<String> = scope_keys_before_save.clone();
            // GNU variables.c:3564-3578 assign_in_env + declare.def:659-668:
            // names bound through `name=value cmd` tempenv assignments are
            // live at this command's variable context, so `declare -n r`
            // sees a tempenv `r` as an existing variable (its value becomes
            // the candidate cell and is validated) rather than resetting a
            // fresh empty local.
            pre_existing.extend(self.tempenv_names.iter().cloned());
            frame_locals.clone_from(&pre_existing);
            self.save_local_names(&args);
            self.promote_tempenv_locals(&args, &prefix_assignment_names, &scope_keys_before_save);
            if !local_args_request_inherit(&args) {
                self.initialize_non_inherited_locals(
                    &args,
                    &prefix_assignment_names,
                    &pre_existing,
                );
            }
        }
        let global_local_values = self.begin_global_declare_for_local_names(&args);
        let posix_function_export_unsets = self.posix_function_declare_unset_export_names(&args);
        let command_name = cmd.words.first().map(String::as_str).unwrap_or("declare");
        // GNU builtins/declare.def:704-806: a declare/typeset assignment whose
        // name is a nameref (and which does not itself carry -n) writes the
        // referenced variable, not the nameref. Capture the resolved targets
        // before the builtin runs so the typed owner can mirror them below.
        let (nameref_flag, _unset_nameref_flag) =
            crate::builtins::declare::declare_nameref_flags(&args);
        let nameref_assign_targets = if nameref_flag {
            // -n keeps the assignment on the nameref cell itself; only +n and
            // plain declarations follow the chain to the referenced variable.
            Vec::new()
        } else {
            crate::builtins::declare::nameref_assignment_targets(&args, &self.env_vars)
        };
        // GNU declare.def:623-660 declare_transform_name +
        // make_local_variable: at function scope an assignment operand that
        // resolves through a nameref binds a LOCAL variable at the current
        // context -- `declare r=/` on r->x creates local x and the global x
        // is restored when the frame returns (nameref20.sub f() cases).
        if self.function_depth > 0 && !declare_args_force_global(&args) {
            for (_, target) in &nameref_assign_targets {
                let local_name = target.split('[').next().unwrap_or(target);
                if !local_name.is_empty() {
                    self.save_frame_local_name(local_name);
                }
            }
            // Attribute-only operands (`declare -a ref`, bare `declare ref`)
            // take the same transform: the resolved name is localized before
            // the attribute pass marks it.
            if !nameref_flag {
                for target in
                    crate::builtins::declare::nameref_resolved_operand_names(&args, &self.env_vars)
                {
                    let local_name = target.split('[').next().unwrap_or(&target);
                    if !local_name.is_empty() {
                        self.save_frame_local_name(local_name);
                    }
                }
            }
        }

        let result = (|| -> Result<i32, ExecuteError> {
            let mut stdout = Vec::new();
            let mut stderr = Vec::new();
            for name in &local_blocked {
                writeln!(
                    stderr,
                    "{}{command_name}: {name}: readonly variable",
                    self.diagnostic_prefix()
                )?;
            }
            // Every operand blocked: GNU skips the operand loop entirely —
            // no list-mode output even though only option args remain.
            let status = if !local_blocked.is_empty() && local_names(&args).is_empty() {
                1
            } else {
                let status = crate::builtins::declare::execute_with_io_named_in_context(
                    command_name,
                    &args,
                    &mut self.env_vars,
                    &mut stdout,
                    &mut stderr,
                    self.function_depth > 0,
                    &frame_locals,
                )?;
                if local_blocked.is_empty() {
                    status
                } else {
                    status.max(1)
                }
            };
            let stderr = if self.stdout_capture.is_some()
                && declare_args_request_print(&args)
                && !args.iter().any(|arg| {
                    (arg.starts_with('-') || arg.starts_with('+'))
                        && (arg.contains('f') || arg.contains('F'))
                })
                && status != 0
            {
                Vec::new()
            } else {
                stderr
            };
            self.write_buffered_builtin_output(cmd, &stdout, &stderr)?;
            // GNU declare.def any_failed: an element failure inside a
            // compound operand still fails the command even though the
            // earlier elements bound.
            Ok(if compound_element_failed && status == 0 {
                1
            } else {
                status
            })
        })();
        if result.as_ref().is_ok_and(|status| *status == 0) {
            // Mirror the through-the-nameref assignments into the typed owner:
            // parameter expansion reads shell_state.variables first, so a
            // stale typed target would shadow the new value written to
            // env_vars (probe: typeset +n foo=other then echo $bar).
            for (_, target) in &nameref_assign_targets {
                if let Some(value) = self.env_vars.get(target).cloned() {
                    match self.shell_state.variables.get_mut(target) {
                        Some(variable) => {
                            variable.value = crate::shell::ShellValue::Scalar(value);
                        }
                        None => {
                            let _ = self.shell_state.variables.set_scalar(target, value);
                        }
                    }
                }
            }
            crate::builtins::declare::sync_typed_assignments(
                &args,
                &self.env_vars,
                &mut self.shell_state.variables,
            );
            crate::builtins::declare::sync_typed_attributes(
                &args,
                &self.env_vars,
                &mut self.shell_state.variables,
            );
            self.apply_posix_function_declare_unset_export(posix_function_export_unsets);
            if crate::builtins::set::shell_option_enabled(&self.env_vars, "allexport") {
                // set -a (allexport): a typeset/declare assignment exports the
                // variable (variables.c do_export / set -a semantics), even
                // when the declare invocation itself carries no -x flag.
                for arg in &args {
                    if arg.contains('=') && !(arg.starts_with('-') || arg.starts_with('+')) {
                        let (raw_name, _) = arg.split_once('=').unwrap_or((arg, ""));
                        let (base, _) = assignment_name_and_append(raw_name);
                        self.mark_exported(base);
                    }
                }
            }
        }
        self.finish_global_declare_for_local_names(global_local_values);
        result
    }

    pub(in crate::executor) fn execute_declare_command(
        &mut self,
        cmd: &CommandNode,
    ) -> Result<(), ExecuteError> {
        if cmd.words[1..].iter().any(|word| {
            (word.starts_with('-') || word.starts_with('+'))
                && (word.contains('f') || word.contains('F'))
        }) {
            let mut stdout = Vec::new();
            let mut stderr = Vec::new();
            self.exit_code =
                self.execute_declare_functions(&cmd.words[1..], &mut stdout, &mut stderr)?;
            self.write_buffered_builtin_output(cmd, &stdout, &stderr)?;
            return Ok(());
        }
        self.exit_code = self.execute_declare(cmd)?;
        Ok(())
    }

    pub(in crate::executor) fn execute_local(
        &mut self,
        cmd: &CommandNode,
    ) -> Result<i32, ExecuteError> {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let status = if self.function_depth == 0 {
            writeln!(
                stderr,
                "{}local: can only be used in a function",
                self.diagnostic_prefix()
            )?;
            1
        } else if let Err(option) = validate_local_options(&cmd.words[1..]) {
            writeln!(
                stderr,
                "{}local: -{option}: invalid option",
                self.diagnostic_prefix()
            )?;
            writeln!(stderr, "local: usage: local [option] name[=value] ...")?;
            2
        } else {
            let mut args = self.expand_declare_assignment_args(&cmd.words[1..]);
            let local_command_name =
                cmd.words.first().map(String::as_str).unwrap_or("local");
            let mut args = match self.rewrite_declare_operand_subscripts(
                &args,
                &cmd.word_metadata,
                local_command_name,
            ) {
                Ok(args) => args,
                Err(()) => return Ok(1),
            };
            if declare_args_request_integer(&args) {
                args = self.evaluate_declare_integer_assignment_args(&args);
            }
            // GNU declare.def:443-455: a bare `-` operand creates a local `-`
            // variable whose value is the current `set -o` option bitmap;
            // pop_var_context applies it through set_current_options
            // (variables.c:5271-5275). Outside -p mode the operand is handled
            // here and stripped — the declare parser would treat it as a flag
            // bundle. In -p mode it stays for show_localname_attributes.
            let mut had_dash_operand = false;
            if !declare_args_request_print(&args) && args.iter().any(|arg| arg == "-") {
                had_dash_operand = true;
                // "no duplicate instances" (declare.def:451): a second
                // `local -` in the same frame keeps the first snapshot.
                if self
                    .local_var_scopes
                    .last()
                    .is_none_or(|scope| !scope.contains_key("-"))
                {
                    self.save_frame_local_name("-");
                    let bitmap = self.current_options_bitmap();
                    self.env_vars.insert("-".to_string(), bitmap.clone());
                    let _ = self.shell_state.variables.set_scalar("-", bitmap);
                }
                args.retain(|arg| arg != "-");
            }
            let mut dash_printed = false;
            // GNU local / local -p with no name arguments prints ONLY the
            // variables declared local to the current function frame (sorted,
            // declare -- form) -- never the whole variable table. Rewrite
            // the args so the shared declare printer renders just those names.
            if !had_dash_operand && local_names(&args).is_empty() {
                let local_names: Vec<String> = self
                    .local_var_scopes
                    .last()
                    .map(|scope| {
                        scope
                            .keys()
                            .filter(|name| !name.starts_with("__RUBASH_"))
                            .cloned()
                            .collect()
                    })
                    .unwrap_or_default();
                let local_names = {
                    let mut names = local_names;
                    names.sort();
                    names
                };
                if local_names.is_empty() {
                    // No locals declared in this frame: GNU prints nothing.
                    self.write_buffered_builtin_output(cmd, &stdout, &stderr)?;
                    return Ok(0);
                }
                // GNU setattr.def:375-378: the `-` local prints as `local -`,
                // not a declare-style assignment line.
                let mut local_names = local_names;
                if let Some(position) = local_names.iter().position(|name| name == "-") {
                    local_names.remove(position);
                    writeln!(stdout, "local -")?;
                    dash_printed = true;
                }
                if !local_names.is_empty() {
                    args.clear();
                    args.push("-p".to_string());
                    args.extend(local_names);
                } else {
                    args.clear();
                }
            }
            // GNU setattr.def:555-570 show_localname_attributes:
            // `local -p name` reports the operand only when it is a local at
            // the CURRENT variable context (local_p && var->context ==
            // variable_context) — an outer frame's local is "not found", so
            // `local -p s` inside a function that declared no s reports an
            // error even when the caller's local s is dynamically visible
            // (varenv25.sub init_vars).
            let mut print_missing: Vec<String> = Vec::new();
            if declare_args_request_print(&args) && !local_names(&args).is_empty() {
                let innermost = self.local_var_scopes.last();
                args.retain(|arg| {
                    // GNU setattr.def:564-568: a `-` operand to `local -p`
                    // prints `local -` when the frame holds the option-snapshot
                    // local (created by `local -` above).
                    if arg == "-" {
                        let found = innermost.is_some_and(|scope| scope.contains_key("-"));
                        if found {
                            writeln!(stdout, "local -").ok();
                            dash_printed = true;
                        } else {
                            print_missing.push("-".to_string());
                        }
                        return false;
                    }
                    if arg.starts_with('-') || arg.starts_with('+') {
                        return true;
                    }
                    let (base, _) = assignment_name_and_append(arg);
                    let found = innermost.is_some_and(|scope| scope.contains_key(base));
                    if !found {
                        print_missing.push(base.to_string());
                    }
                    found
                });
            }
            for name in &print_missing {
                writeln!(
                    stderr,
                    "{}local: {name}: not found",
                    self.diagnostic_prefix()
                )?;
            }
            // GNU variables.c:2651-2665 (make_local_variable): readonly
            // global bindings reject local creation — drop those operands
            // before the frame save so neither a local nor an assignment
            // happens (declare.def:669-673 any_failed + NEXT_VARIABLE).
            let local_blocked: Vec<String> = if !declare_args_request_print(&args) {
                self.global_readonly_local_blocks(&args)
            } else {
                Vec::new()
            };
            if !local_blocked.is_empty() {
                args.retain(|arg| {
                    local_assignment_name(arg)
                        .map_or(true, |name| !local_blocked.iter().any(|b| b == name))
                });
            }
            let mut frame_locals: Vec<String> = Vec::new();
            if !declare_args_request_print(&args) {
                let prefix_assignment_names = cmd
                    .assignment_keys()
                    .map(|name| assignment_name_and_append(name).0.to_string())
                    .collect::<Vec<_>>();
                let scope_keys_before_save: Vec<String> = self
                    .local_var_scopes
                    .last()
                    .map(|scope| scope.keys().cloned().collect())
                    .unwrap_or_default();
                let mut pre_existing: Vec<String> = scope_keys_before_save.clone();
                // Same tempenv visibility as the declare path above: names
                // bound by `name=value cmd` are live at this context.
                pre_existing.extend(self.tempenv_names.iter().cloned());
                frame_locals.clone_from(&pre_existing);
                self.save_local_names(&args);
                self.promote_tempenv_locals(
                    &args,
                    &prefix_assignment_names,
                    &scope_keys_before_save,
                );
                if !local_args_request_inherit(&args) {
                    self.initialize_non_inherited_locals(
                        &args,
                        &prefix_assignment_names,
                        &pre_existing,
                    );
                }
            }
            self.write_local_compound_readonly_assignment_errors(&args, &mut stderr)?;
            // GNU declare.def:565 uses variable_context to decide between the
            // global-scope self-reference error and the function-scope
            // circular-reference warning; local always runs in a function, so
            // `local -n a=$1` with a=$1 warns and continues.
            for name in &local_blocked {
                writeln!(
                    stderr,
                    "{}local: {name}: readonly variable",
                    self.diagnostic_prefix()
                )?;
            }
            let (status, builtin_status) = if local_names(&args).is_empty()
                && (had_dash_operand
                    || dash_printed
                    || !local_blocked.is_empty()
                    || !print_missing.is_empty())
            {
                (
                    if local_blocked.is_empty() && print_missing.is_empty() {
                        0
                    } else {
                        1
                    },
                    0,
                )
            } else {
                let builtin_status = crate::builtins::declare::execute_with_io_named_in_context(
                    "local",
                    &args,
                    &mut self.env_vars,
                    &mut stdout,
                    &mut stderr,
                    true,
                    &frame_locals,
                )?;
                let status = if local_blocked.is_empty() && print_missing.is_empty() {
                    builtin_status
                } else {
                    builtin_status.max(1)
                };
                (status, builtin_status)
            };
            if builtin_status == 0 {
                // Plain scalar locals must shadow the outer value in the typed
                // owner as well: parameter expansion reads shell_state.variables
                // first, so `local OPTERR=1` inside a function has to replace the
                // stale global there (getopts5.sub: getop must print OPTERR=1,
                // and the frame restore in restore_function_locals puts the
                // saved outer value back).
                for arg in &args {
                    let Some((raw_name, _)) = arg.split_once('=') else {
                        continue;
                    };
                    let name = raw_name.strip_suffix('+').unwrap_or(raw_name);
                    let (base, _) = assignment_name_and_append(name);
                    if is_marked_var(&self.env_vars, ARRAY_VARS, base)
                        || is_marked_var(&self.env_vars, ASSOC_VARS, base)
                        || is_marked_var(&self.env_vars, NAMEREF_VARS, base)
                    {
                        continue;
                    }
                    match self.env_vars.get(base) {
                        Some(value) => match self.shell_state.variables.get_mut(base) {
                            Some(variable) => {
                                variable.value = crate::shell::ShellValue::Scalar(value.clone());
                            }
                            None => {
                                let _ = self.shell_state.variables.set_scalar(base, value.clone());
                            }
                        },
                        None => {
                            self.shell_state.variables.remove(base);
                        }
                    }
                }
            }
            status
        };
        let stderr = local_stderr_from_declare(stderr);
        self.write_buffered_builtin_output(cmd, &stdout, &stderr)?;
        Ok(status)
    }

    pub(in crate::executor) fn write_local_compound_readonly_assignment_errors<W>(
        &self,
        args: &[String],
        stderr: &mut W,
    ) -> io::Result<()>
    where
        W: Write,
    {
        for arg in args {
            let Some((name, value)) = split_assignment_word(arg) else {
                continue;
            };
            if !value.starts_with(COMPOUND_ASSIGNMENT_MARKER) {
                continue;
            }
            let (name, _) = assignment_name_and_append(name);
            if is_marked_var(&self.env_vars, READONLY_VARS, name) {
                writeln!(
                    stderr,
                    "{}{}: readonly variable",
                    self.diagnostic_prefix(),
                    name
                )?;
            }
        }
        Ok(())
    }

    /// GNU variables.c:2579-2631 make_local_variable (was_tmpvar path): a
    /// `declare`/`typeset`/`local` operand whose name is bound by this
    /// command's own `name=value` prefix finds the tempenv binding
    /// (find_variable -> tempvar) and promotes it to a frame local in
    /// place, keeping its value and exported attribute — `z=y typeset z`
    /// leaves a live exported local z=y for the rest of the frame, and
    /// `z=y typeset z=w` leaves the local the assignment created. The
    /// binding is NOT popped with the command's tempenv. For a name that
    /// was not already a frame local, the frame-restore snapshot must hold
    /// the value BEFORE the prefix applied (recorded by
    /// apply_temporary_assignments in tempenv_previous), otherwise the
    /// tempenv value leaks into the global scope when the frame returns.
    pub(in crate::executor) fn promote_tempenv_locals(
        &mut self,
        args: &[String],
        prefix_assignment_names: &[String],
        scope_keys_before_save: &[String],
    ) {
        if self.function_depth == 0 || prefix_assignment_names.is_empty() {
            return;
        }
        for name in local_names(args) {
            if !prefix_assignment_names.iter().any(|prefix| prefix == &name) {
                continue;
            }
            // The prefix binding may have been rejected (readonly target) or
            // routed through a nameref to a different cell; only a binding
            // that actually landed on this name is promoted.
            if !self.env_vars.contains_key(&name) {
                continue;
            }
            if !self
                .tempenv_promoted_names
                .iter()
                .any(|saved| saved == &name)
            {
                self.tempenv_promoted_names.push(name.clone());
            }
            // A name that was already a frame local keeps the snapshot its
            // earlier declaration recorded (the caller's value); only a
            // first declaration's snapshot gets the pre-prefix value.
            if scope_keys_before_save.iter().any(|saved| saved == &name) {
                continue;
            }
            if let Some((env_value, typed_value, attrs)) = self.tempenv_previous.get(&name).cloned()
            {
                if let Some(scope) = self.local_var_scopes.last_mut() {
                    scope.insert(name.clone(), env_value);
                }
                if let Some(typed_scope) = self.local_typed_scopes.last_mut() {
                    typed_scope.insert(name.clone(), typed_value);
                }
                if let Some(attr_scope) = self.local_attr_scopes.last_mut() {
                    attr_scope.insert(name.clone(), attrs);
                }
            }
        }
    }

    pub(in crate::executor) fn initialize_non_inherited_locals(
        &mut self,
        args: &[String],
        preserve_names: &[String],
        pre_existing: &[String],
    ) {
        if crate::builtins::shopt::option_enabled(&self.env_vars, "localvar_inherit") {
            return;
        }
        for name in local_names(args) {
            if preserve_names.iter().any(|preserve| preserve == &name) {
                continue;
            }
            // GNU builtins/declare.def:659-668: re-declaring a variable that
            // is already local at the SAME variable context keeps it (var =
            // refvar), so a valueless re-declaration of an existing
            // same-frame nameref preserves its cell (nameref12/nameref13.sub)
            // instead of resetting a fresh empty local. pre_existing lists
            // the frame snapshot BEFORE this command saved its own names, so
            // a first-time `declare -a a` still resets the fresh local
            // (assoc.tests: f: declare -a a prints an empty local).
            if pre_existing.iter().any(|existing| existing == &name) {
                continue;
            }
            let was_exported = is_marked_var(&self.env_vars, EXPORTED_VARS, &name);
            if was_exported {
                if let Some(value) = self.env_vars.get(&name).cloned() {
                    set_local_export_env_value(&mut self.env_vars, &name, value);
                }
            }
            self.env_vars.remove(&name);
            // A fresh local shadows the outer variable in the typed owner too:
            // parameter expansion reads shell_state.variables first, so a
            // stale global scalar would keep leaking through (bash: `local X`
            // makes ${X-unset} report unset until the frame returns).
            self.shell_state.variables.remove(&name);
            set_var_attrs(&mut self.env_vars, &name, VarAttrs::default());
            // GNU variables.c:2729 (make_local_variable): a non-inheriting
            // local still inherits the export attribute — and only that
            // attribute — from the variable it shadows.
            if was_exported {
                mark_env_name(&mut self.env_vars, EXPORTED_VARS, &name);
            }
        }
    }

    /// GNU variables.c:2651-2665 (make_local_variable): creating a local for a
    /// name whose visible binding is readonly at context 0 is rejected — the
    /// comment calls local copies of readonly globals a security hole, while
    /// readonly bindings in a caller's frame (context > 0) or the live
    /// tempenv (context == variable_context) may still be shadowed. Returns
    /// the operand names to drop; declare.def:669-673 counts each as a
    /// failure and skips the operand entirely.
    fn global_readonly_local_blocks(&self, args: &[String]) -> Vec<String> {
        local_names(args)
            .into_iter()
            .filter(|name| {
                let base = name.split('[').next().unwrap_or(name.as_str());
                let readonly = is_marked_var(&self.env_vars, READONLY_VARS, base)
                    || self
                        .shell_state
                        .variables
                        .get(base)
                        .is_some_and(|variable| variable.readonly);
                readonly
                    && !self.tempenv_names.iter().any(|saved| saved == base)
                    && !self
                        .local_var_scopes
                        .iter()
                        .any(|scope| scope.contains_key(base))
            })
            .collect()
    }
}

/// True when a declare/typeset/local command line explicitly names DIRSTACK
/// (as an operand or assignment target). GNU variables.c get_dirstack runs
/// the dynamic getter -- materializing the stored array cell -- only on
/// named access; a bare list-all `declare -a` shows the last materialized
/// cell, which stays empty when DIRSTACK was never named.
fn declare_words_name_dirstack(words: &[String]) -> bool {
    let mut options_ended = false;
    for word in words {
        if !options_ended {
            if word == "--" {
                options_ended = true;
                continue;
            }
            if word.starts_with('-') || word.starts_with('+') {
                continue;
            }
        }
        let base = word.split('=').next().unwrap_or(word);
        let base = base.split('[').next().unwrap_or(base);
        if base == "DIRSTACK" {
            return true;
        }
    }
    false
}
