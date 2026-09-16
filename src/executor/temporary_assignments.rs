use super::*;

impl Executor {
    /// Apply assignments from a command containing no command word. GNU Bash
    /// keeps these assignments in the current shell scope; they are not the
    /// temporary environment used by `name=value command`.
    pub(in crate::executor) fn apply_permanent_assignments(
        &mut self,
        assignments: &[(String, String)],
    ) {
        for (name, value) in assignments {
            let expanded_value = self.expand_assignment_value(value);
            self.apply_shell_assignment(name, expanded_value);
            // GNU variables.c make_variable_value: an integer-attribute
            // assignment that fails arithmetic evaluation (e.g. `i=0#4`
            // with `declare -i i`) reports evalerror and propagates exit
            // status 1. apply_shell_assignment resets exit_code to 0 on
            // success, so promote the arithmetic_expansion_error flag here.
            if self.arithmetic_expansion_error.get() {
                self.arithmetic_expansion_error.set(false);
                self.exit_code = 1;
            }
        }
    }

    pub(in crate::executor) fn apply_temporary_assignments(
        &mut self,
        assignments: &[(String, String)],
    ) -> Vec<(String, Option<String>, Option<crate::shell::Variable>)> {
        // TODO(execute_cmd.c/variables.c): Bash applies assignment words with
        // different persistence rules for special builtins, functions, POSIX
        // mode, and external command environments. For upstream builtins tests,
        // make prefix assignments visible while the command runs, then restore
        // the previous shell variable values (both the legacy env_vars value
        // and the typed shell_state.variables owner, so parameter expansion
        // does not keep seeing a leaked temporary value).
        let mut previous = Vec::new();
        if !assignments.is_empty() {
            previous.push((
                EXPORTED_VARS.to_string(),
                self.env_vars.get(EXPORTED_VARS).cloned(),
                self.shell_state.variables.get(EXPORTED_VARS).cloned(),
            ));
        }
        // GNU findcmd.c:356-365: a PATH in the temporary command environment
        // (PATH=foo cmd) bypasses the hash table entirely. Rubash's lookup
        // cache keys on a PATH fingerprint, which already prevents temp-PATH
        // results from polluting the normal cache, but GNU also skips the
        // hash read so a stale remembered path is never returned for a
        // temp-PATH command. Tag the temp-PATH state here so find_user_command
        // can bypass the cache; the tag is cleared in restore.
        let has_temp_path = assignments.iter().any(|(name, _)| {
            let base = name.split('[').next().unwrap_or(name);
            base == "PATH"
        });
        if has_temp_path {
            previous.push((
                "__RUBASH_TEMP_PATH".to_string(),
                self.env_vars.get("__RUBASH_TEMP_PATH").cloned(),
                self.shell_state
                    .variables
                    .get("__RUBASH_TEMP_PATH")
                    .cloned(),
            ));
            self.env_vars
                .insert("__RUBASH_TEMP_PATH".to_string(), "1".to_string());
        }
        for (name, value) in assignments {
            let expanded_value = self.expand_assignment_value(value);
            let (base_name, _) = assignment_name_and_append(name);
            previous.push((
                base_name.to_string(),
                self.env_vars.get(base_name).cloned(),
                self.shell_state.variables.get(base_name).cloned(),
            ));
            // GNU variables.c bind_variable (ASS_NAMEREF path): a temporary
            // assignment to a nameref writes the referenced variable, so the
            // restore must also capture the target's previous value or the
            // referenced variable keeps the temporary value after the command.
            let resolved_target: Option<String> = match self.nameref_resolution(base_name) {
                NamerefResolution::Target(ref target) if *target != *base_name => {
                    Some(target.clone())
                }
                _ => None,
            };
            if let Some(ref target) = resolved_target {
                previous.push((
                    target.clone(),
                    self.env_vars.get(target).cloned(),
                    self.shell_state.variables.get(target).cloned(),
                ));
            }
            self.apply_shell_assignment(name, expanded_value);
            // GNU variables.c bind_variable (ASS_NAMEREF tempenv path): the
            // temporary assignment lands on the referenced variable and it is
            // that variable which is exported for the command, never the
            // nameref itself (nameref14.sub: `ref=xxx typeset -p ref var`
            // prints `declare -x var` while ref stays unexported).
            let export_target = resolved_target
                .clone()
                .unwrap_or_else(|| base_name.to_string());
            self.mark_exported(&export_target);
        }
        previous
    }

    /// GNU bind_variable with a nameref cell naming an array element: the
    /// value (and its integer evaluation when either the nameref or the
    /// referenced array carries the integer attribute) is written to that
    /// element of the referenced array (nameref23.sub: declare -in b="a[0]";
    /// b+=1 increments a[0]).
    fn apply_nameref_array_element_assignment(
        &mut self,
        elem_base: &str,
        subscript: &str,
        value: &str,
        append: bool,
        integer: bool,
    ) -> bool {
        let current = self.env_vars.get(elem_base).cloned().unwrap_or_default();
        if is_marked_var(&self.env_vars, ASSOC_VARS, elem_base) {
            let key = self.assoc_subscript_key(subscript);
            let mut entries = assoc_entries(&current);
            let existing = entries
                .iter()
                .rev()
                .find_map(|(entry_key, entry_value)| {
                    (entry_key == &key).then_some(entry_value.clone())
                })
                .unwrap_or_default();
            let element = if append {
                if integer {
                    // Real evaluator: resolves shell variables like GNU's
                    // expr.c evaluation (flix=9 -> 9, not the storage-shape 0).
                    (self.eval_integer_assignment_value(&existing)
                        + self.eval_integer_assignment_value(value))
                    .to_string()
                } else {
                    append_scalar_value(&existing, value)
                }
            } else if integer {
                self.eval_integer_assignment_value(value).to_string()
            } else {
                value.to_string()
            };
            if let Some((_, entry_value)) = entries
                .iter_mut()
                .rev()
                .find(|(entry_key, _)| entry_key == &key)
            {
                *entry_value = element;
            } else {
                entries.push((key, element));
            }
            let new_value = format!(
                "({})",
                entries
                    .into_iter()
                    .map(|(key, value)| {
                        format!(
                            "[{}]={}",
                            quote_assoc_key(&key),
                            quote_assoc_storage_value(&value)
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(" ")
            );
            self.env_vars.insert(elem_base.to_string(), new_value);
            self.exit_code = 0;
            return true;
        }
        let Ok(index) = self
            .eval_integer_assignment_value(subscript)
            .to_string()
            .parse::<usize>()
        else {
            self.exit_code = 1;
            return false;
        };
        let mut entries = indexed_array_entries(&current);
        let current_element = entries.get(&index).cloned().unwrap_or_default();
        let element = if append {
            if integer {
                (self.eval_integer_assignment_value(&current_element)
                    + self.eval_integer_assignment_value(value))
                .to_string()
            } else {
                append_scalar_value(&current_element, value)
            }
        } else if integer {
            self.eval_integer_assignment_value(value).to_string()
        } else {
            value.to_string()
        };
        entries.insert(index, element);
        self.env_vars
            .insert(elem_base.to_string(), format_indexed_array_storage(entries));
        self.exit_code = 0;
        true
    }

    pub(in crate::executor) fn apply_shell_assignment(
        &mut self,
        name: &str,
        value: String,
    ) -> bool {
        // TODO(variables.c/arrayfunc.c): Bash stores append assignment state
        // separately on WORD_DESC/ASSIGNMENT_WORD. This narrow path handles
        // scalar `name+=value` until SHELL_VAR attributes and arrays own it.
        let (base_name, append) = assignment_name_and_append(name);
        let target_name = match self.nameref_resolution(base_name) {
            NamerefResolution::Target(target) => target,
            NamerefResolution::Circular => {
                // GNU writes each diagnostic with one write(2). eprintln!
                // fragments the message into one syscall per format piece and
                // those pieces race with stdout under the WSL interop relay,
                // splitting the message across unrelated lines; emit one
                // pre-formatted buffer instead.
                let line = format!(
                    "{}warning: {}: circular name reference\n",
                    self.diagnostic_prefix(),
                    base_name
                );
                let _ = std::io::stderr().write_all(line.as_bytes());
                // GNU variables.c:2036-2046 find_variable_nameref + the
                // bind_variable maxloop path: inside a function a circula
                // nameref assignment writes the GLOBAL namesake of the name
                // that closed the loop, while the local nameref keeps its
                // cell (nameref8.sub f1, nameref15.sub xxx_func).
                let circular_value = if append {
                    let current = self.circular_fallback_value(base_name).unwrap_or_default();
                    format!("{current}{value}")
                } else {
                    value.clone()
                };
                self.assign_circular_fallback(base_name, circular_value);
                self.exit_code = 0;
                return true;
            }
            NamerefResolution::NotNameref => base_name.to_string(),
        };
        // GNU variables.c bind_variable_internal: when a nameref has an
        // empty cell (valueless, created by `declare -n name` without a
        // value), an assignment with a valid shell name or array subscript
        // sets the nameref target; an invalid value is rejected with
        // sh_invalidid.  A nameref whose cell is already invalid (not
        // empty, not a valid name) is left unchanged on any assignment
        // (nameref12.sub: r=^ against an invalid cell).
        if is_marked_var(&self.env_vars, NAMEREF_VARS, base_name) {
            let cell = self.env_vars.get(base_name).cloned().unwrap_or_default();
            let cell_valid = is_shell_name(&cell) || parse_array_subscript(&cell).is_some();
            if !append && !cell_valid {
                // Distinguish valueless (empty) from already-invalid cells.
                let value_valid = is_shell_name(value.as_str())
                    || parse_array_subscript(value.as_str()).is_some();
                if cell.is_empty() && value_valid {
                    // Valueless nameref: set the target to the new value.
                    self.env_vars.insert(base_name.to_string(), value.clone());
                    self.exit_code = 0;
                    return true;
                }
                let offender = if cell.is_empty() {
                    value.as_str()
                } else {
                    cell.as_str()
                };
                let line = format!(
                    "{}`{offender}': not a valid identifier\n",
                    self.diagnostic_prefix()
                );
                let _ = std::io::stderr().write_all(line.as_bytes());
                self.exit_code = 1;
                return false;
            }
        }
        let base_name = target_name.as_str();
        // GNU arrayfunc.c/variables.c: a nameref whose cell is an array
        // element (declare -in b="a[0]"; b+=1) binds through to that element
        // of the referenced array instead of creating a variable literally
        // named a[0] (nameref23.sub).
        if let Some((elem_base, subscript)) = base_name.split_once('[') {
            if let Some(subscript) = subscript.strip_suffix(']') {
                // GNU builtins/common.c:949 builtin_bind_variable: any valid
                // array reference (name[subscript]) goes through
                // assign_array_element -> find_or_make_array_variable,
                // which creates the array on demand. This applies to `read
                // x[1]` (read.def:1151 bind_read_variable) and direct
                // `x[1]=value` assignments alike, even when the variable is
                // not previously declared as an array (array.tests:80).
                if is_marked_var(&self.env_vars, ARRAY_VARS, elem_base)
                    || is_marked_var(&self.env_vars, ASSOC_VARS, elem_base)
                {
                    if is_marked_var(&self.env_vars, "__RUBASH_READONLY_VARS", elem_base) {
                        let line = format!(
                            "{}{}: readonly variable\n",
                            self.diagnostic_prefix(),
                            elem_base
                        );
                        let _ = std::io::stderr().write_all(line.as_bytes());
                        self.exit_code = 1;
                        return false;
                    }
                    let integer = is_marked_var(&self.env_vars, INTEGER_VARS, base_name)
                        || is_marked_var(&self.env_vars, INTEGER_VARS, elem_base);
                    return self.apply_nameref_array_element_assignment(
                        elem_base, subscript, &value, append, integer,
                    );
                }
                // Variable not yet declared as an array: create it as an
                // indexed array on demand (GNU find_or_make_array_variable
                // at arrayfunc.c:453). Skip empty subscripts and [@]/[*]
                // which are not valid for element assignment.
                if !subscript.is_empty()
                    && subscript != "@"
                    && subscript != "*"
                    && is_shell_name(elem_base)
                {
                    if is_marked_var(&self.env_vars, "__RUBASH_READONLY_VARS", elem_base) {
                        let line = format!(
                            "{}{}: readonly variable\n",
                            self.diagnostic_prefix(),
                            elem_base
                        );
                        let _ = std::io::stderr().write_all(line.as_bytes());
                        self.exit_code = 1;
                        return false;
                    }
                    let integer = is_marked_var(&self.env_vars, INTEGER_VARS, base_name)
                        || is_marked_var(&self.env_vars, INTEGER_VARS, elem_base);
                    return self.apply_nameref_array_element_assignment(
                        elem_base, subscript, &value, append, integer,
                    );
                }
            }
        }
        if is_marked_var(&self.env_vars, "__RUBASH_READONLY_VARS", base_name) {
            let line = format!(
                "{}{}: readonly variable\n",
                self.diagnostic_prefix(),
                base_name
            );
            let _ = std::io::stderr().write_all(line.as_bytes());
            self.exit_code = 1;
            return false;
        }
        if base_name == "OPTIND" && !append {
            self.env_vars.remove("__RUBASH_GETOPTS_OFFSET");
        }
        if base_name == "SECONDS" && !append {
            let assigned = value.trim().parse::<i64>().unwrap_or(0);
            let start = self
                .env_vars
                .get(SHELL_START_EPOCH)
                .and_then(|value| value.parse::<i64>().ok())
                .unwrap_or_else(current_epoch_seconds);
            let elapsed = current_epoch_seconds() - start;
            self.env_vars
                .insert(SECONDS_OFFSET.to_string(), (assigned - elapsed).to_string());
            set_process_env(base_name, assigned.to_string());
            return true;
        }
        if base_name == "RANDOM" && !append {
            self.random_state
                .set(value.trim().parse::<u32>().unwrap_or(0));
            set_process_env(base_name, value);
            return true;
        }
        if base_name == "SRANDOM" && !append {
            return true;
        }
        if base_name == "BASHPID" && !append {
            return true;
        }
        if base_name == "BASH_SUBSHELL" && !append {
            return true;
        }
        if base_name == "FUNCNAME" && !append {
            return true;
        }
        if base_name == "LINENO" && !append {
            return true;
        }
        if base_name == "BASH_COMMAND" && !append {
            return true;
        }
        if is_noassign_bash_array(base_name) && !append {
            return true;
        }
        let compound_assignment = value.starts_with(COMPOUND_ASSIGNMENT_MARKER);
        let value = value
            .strip_prefix(COMPOUND_ASSIGNMENT_MARKER)
            .unwrap_or(&value)
            .to_string();
        let value = if append {
            let current = self.env_vars.get(base_name).cloned().unwrap_or_default();
            if is_marked_var(&self.env_vars, ASSOC_VARS, base_name) {
                if value.starts_with('(') && value.ends_with(')') {
                    append_assoc_value(
                        &current,
                        &value,
                        is_marked_var(&self.env_vars, INTEGER_VARS, base_name),
                        &self.env_vars,
                    )
                } else {
                    append_assoc_scalar_value(&current, &value)
                }
            } else if is_array_storage(&current)
                || is_marked_var(&self.env_vars, ARRAY_VARS, base_name)
            {
                append_array_value(
                    &current,
                    &value,
                    is_marked_var(&self.env_vars, INTEGER_VARS, base_name),
                    self.env_vars.get("IFS").map(String::as_str),
                    &self.env_vars,
                )
            } else if is_marked_var(&self.env_vars, INTEGER_VARS, base_name) {
                let current = self.eval_integer_assignment_value(&current);
                let value = self.eval_integer_assignment_value(&value);
                (current + value).to_string()
            } else {
                append_scalar_value(&current, &value)
            }
        } else if compound_assignment
            && value.starts_with('(')
            && value.ends_with(')')
            && is_marked_var(&self.env_vars, ASSOC_VARS, base_name)
        {
            let bare_elements = assoc_bare_elements(&value);
            // GNU assign_compound_array_list (arrayfunc.c:838-843): a bare
            // element in an assoc compound assignment reports an error and
            // breaks the loop, but elements already processed ARE stored.
            // Store the valid elements first, then report the error.
            let stored = append_assoc_value(
                "()",
                &value,
                is_marked_var(&self.env_vars, INTEGER_VARS, base_name),
                &self.env_vars,
            );
            self.env_vars.insert(base_name.to_string(), stored.clone());
            for bare in &bare_elements {
                eprintln!(
                    "{}{}: {}: must use subscript when assigning associative array",
                    self.diagnostic_prefix(),
                    base_name,
                    bare
                );
            }
            if !bare_elements.is_empty() {
                self.exit_code = 1;
            }
            stored
        } else if compound_assignment
            && value.starts_with('(')
            && value.ends_with(')')
            && !is_marked_var(&self.env_vars, ASSOC_VARS, base_name)
            && is_marked_var(&self.env_vars, INTEGER_VARS, base_name)
            && integer_compound_assignment_is_scalar(&value)
        {
            // Bash keeps `typeset -i x; x=(1+2)` scalar.  A compound
            // assignment becomes an array only when it contains indexed o
            // multiple elements; the single arithmetic expression is still
            // assigned through the integer attribute.
            self.eval_integer_assignment_value(&value[1..value.len() - 1])
                .to_string()
        } else if compound_assignment
            && value.starts_with('(')
            && value.ends_with(')')
            && !is_marked_var(&self.env_vars, ASSOC_VARS, base_name)
        {
            // variables.c/arrayfunc.c: a compound `name=(...)` assignment
            // always makes an array, even when the variable previously had
            // the integer attribute (`typeset -i x; x=([0]=7+11)` becomes an
            // integer array with x[0]=18, not a scalar arithmetic result).
            append_array_value(
                "()",
                &value,
                is_marked_var(&self.env_vars, INTEGER_VARS, base_name),
                self.env_vars.get("IFS").map(String::as_str),
                &self.env_vars,
            )
        } else if is_marked_var(&self.env_vars, INTEGER_VARS, base_name) {
            // GNU variables.c make_variable_value with integer attribute calls
            // evalexp -> strlong, which reports "invalid number" / "invalid
            // arithmetic base" etc. via evalerror. We mirror that here: if the
            // arithmetic evaluation fails, report the error and store empty.
            let (result, _category) =
                eval_conditional_arith_value_categorized(&value, &self.env_vars);
            if result.is_none() {
                if let Some(msg) =
                    crate::executor::arithmetic::arithmetic_error_message(&value, false, &self.env_vars)
                {
                    eprintln!("{}{}", self.diagnostic_prefix(), msg);
                }
                self.arithmetic_expansion_error.set(true);
                String::new()
            } else {
                result.unwrap_or(0).to_string()
            }
        } else {
            value
        };
        let value = self.apply_case_assignment_attributes(base_name, value);
        let protocol_scalar = self.pending_scalar_assignment;
        self.pending_scalar_assignment = false;
        if value.starts_with('\x1d')
            && !protocol_scalar
            && !is_marked_var(&self.env_vars, ASSOC_VARS, base_name)
        {
            mark_env_name(&mut self.env_vars, ARRAY_VARS, base_name);
        }
        unmark_env_name(&mut self.env_vars, DECLARED_UNSET_VARS, base_name);
        let is_array = compound_assignment
            || is_marked_var(&self.env_vars, ARRAY_VARS, base_name)
            || is_marked_var(&self.env_vars, ASSOC_VARS, base_name);
        // GNU variables.c:3128-3142 bind_variable_internal: when the
        // variable is already an array, a scalar assignment sets array[0]
        // without clearing other elements (array.tests:171-174:
        // x[4]=bbb; x=abde keeps x[4]=bbb). Only compound `x=(...)` or
        // append `x+=...` should replace/extend the whole array.
        if !compound_assignment
            && !append
            && is_array
            && !is_marked_var(&self.env_vars, ASSOC_VARS, base_name)
            && !value.starts_with('\x1d')
        {
            let current = self.env_vars.get(base_name).cloned().unwrap_or_default();
            let mut entries = indexed_array_entries(&current);
            entries.insert(0, value.clone());
            let storage = format_indexed_array_storage(entries);
            self.env_vars.insert(base_name.to_string(), storage);
            mark_env_name(&mut self.env_vars, ARRAY_VARS, base_name);
            if crate::builtins::set::shell_option_enabled(&self.env_vars, "allexport") {
                set_process_env(base_name, self.env_vars[base_name].clone());
            }
            self.exit_code = 0;
            return true;
        }
        if !is_array {
            if let Some(variable) = self.shell_state.variables.get_mut(base_name) {
                if let crate::shell::ShellValue::Scalar(current) = &mut variable.value {
                    *current = value.clone();
                }
            } else {
                let _ = self
                    .shell_state
                    .variables
                    .set_scalar(base_name, value.clone());
            }
        }
        // GNU variables.c:3139-3140: for an assoc array, a scalar assignment
        // stores the value at key "0" via assign_func. Without this, a value
        // like `([a]=1)` is stored raw and later misinterpreted as assoc
        // storage format (assoc.tests:191 T='([a]=1)' -> ${T[@]} is `([a]=1)`,
        // not `1`).
        if !compound_assignment
            && !append
            && is_marked_var(&self.env_vars, ASSOC_VARS, base_name)
            && !value.starts_with('\x1d')
        {
            let storage = format!("([\"0\"]={})", quote_assoc_storage_value(&value));
            self.env_vars.insert(base_name.to_string(), storage);
            if crate::builtins::set::shell_option_enabled(&self.env_vars, "allexport") {
                self.mark_exported(base_name);
            }
            self.exit_code = 0;
            return true;
        }
        self.env_vars.insert(base_name.to_string(), value.clone());
        if crate::builtins::set::shell_option_enabled(&self.env_vars, "allexport") {
            self.mark_exported(base_name);
        }
        sync_shell_assignment_process_env(&self.env_vars, base_name, value);
        true
    }
}

fn integer_compound_assignment_is_scalar(value: &str) -> bool {
    let Some(inner) = value.strip_prefix('(').and_then(|v| v.strip_suffix(')')) else {
        return false;
    };
    !inner.is_empty() && !inner.chars().any(|ch| ch.is_whitespace()) && !inner.contains(['[', ']'])
}
