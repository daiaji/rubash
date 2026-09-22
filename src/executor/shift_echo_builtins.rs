use super::*;

impl Executor {
    pub(in crate::executor) fn execute_recho_command(
        &mut self,
        cmd: &CommandNode,
    ) -> Result<(), ExecuteError> {
        let output = self.recho_output(&cmd.words[1..]);
        self.write_buffered_builtin_output(cmd, output.as_bytes(), &[])?;
        self.exit_code = 0;
        Ok(())
    }

    pub(in crate::executor) fn recho_output(&self, args: &[String]) -> String {
        let mut output = String::new();
        for (index, arg) in args.iter().enumerate() {
            output.push_str(&format!(
                "argv[{}] = <{}>\n",
                index + 1,
                recho_display_arg(arg)
            ));
        }
        output
    }

    // support/zecho.c main(): bare-bones echo used by the upstream test
    // suite -- print the arguments separated by single spaces with one
    // trailing newline, no option or escape processing.
    pub(in crate::executor) fn execute_zecho_command(
        &mut self,
        cmd: &CommandNode,
    ) -> Result<(), ExecuteError> {
        let output = self.zecho_output(&cmd.words[1..]);
        self.write_buffered_builtin_output(cmd, output.as_bytes(), &[])?;
        self.exit_code = 0;
        Ok(())
    }

    pub(in crate::executor) fn zecho_output(&self, args: &[String]) -> String {
        let mut output = args.join(" ");
        output.push('\n');
        output
    }

    pub(in crate::executor) fn execute_shift_command(
        &mut self,
        cmd: &CommandNode,
    ) -> Result<(), ExecuteError> {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let action =
            crate::builtins::shift::execute_with_io(&cmd.words[1..], &mut stdout, &mut stderr)?;
        self.apply_shift_action_with_stderr(action, &mut stderr)?;
        self.write_buffered_builtin_output(cmd, &stdout, &stderr)
    }

    pub(in crate::executor) fn apply_shift_action_with_stderr<W: Write>(
        &mut self,
        action: crate::builtins::shift::ShiftAction,
        stderr: &mut W,
    ) -> Result<(), ExecuteError> {
        match action {
            crate::builtins::shift::ShiftAction::Complete(status) => {
                self.exit_code = status;
            }
            crate::builtins::shift::ShiftAction::Shift(amount) => {
                if amount > self.shell_state.positional_params.len() {
                    if crate::builtins::shopt::option_enabled(
                        &self.shell_state.env_vars,
                        "shift_verbose",
                    ) {
                        writeln!(
                            stderr,
                            "{}shift: {amount}: shift count out of range",
                            self.diagnostic_prefix()
                        )?;
                    }
                    self.exit_code = 1;
                    return Ok(());
                }
                let mut positional = self.shell_state.positional_params.clone();
                positional.drain(0..amount);
                self.set_positional_params(positional);
                self.exit_code = 0;
            }
        }
        Ok(())
    }

    pub(in crate::executor) fn execute_time_command_node(
        &mut self,
        cmd: &CommandNode,
    ) -> Result<(), ExecuteError> {
        let mut index = 1;
        let mut inverted = false;
        while let Some(word) = cmd.words.get(index).map(String::as_str) {
            match word {
                "-p" | "--" => index += 1,
                "!" => {
                    inverted = !inverted;
                    index += 1;
                }
                _ => break,
            }
        }
        if index >= cmd.words.len() {
            let started = time_command_started();
            print_time(
                &self.shell_state.env_vars,
                cmd.words.iter().skip(1).any(|word| word == "-p"),
                started,
            );
            self.exit_code = 0;
            return Ok(());
        }

        let mut timed = cmd.clone();
        timed.words = cmd.words[index..].to_vec();
        if cmd.word_kinds.len() == cmd.words.len() {
            timed.word_kinds = cmd.word_kinds[index..].to_vec();
        }
        let started = time_command_started();
        self.execute_command(&timed)?;
        print_time(
            &self.shell_state.env_vars,
            cmd.words.iter().skip(1).any(|word| word == "-p"),
            started,
        );
        if inverted {
            self.exit_code = invert_exit_status(self.exit_code);
        }
        Ok(())
    }

    pub(in crate::executor) fn execute_echo(
        &mut self,
        cmd: &CommandNode,
    ) -> Result<(), ExecuteError> {
        self.exit_code = 0;
        // TODO(redir.c/execute_cmd.c/builtins/echo.def): Generalize builtin
        // redirection. This covers upstream source tests that create sourced
        // files with `echo ... > file`.
        let echo_args = echo_args_without_background_marker(&cmd.words[1..]);
        let mut output = Vec::new();
        crate::builtins::echo::write_echo_decoded(
            echo_args.iter().map(String::as_str),
            &mut output,
        )?;
        if self.write_ordered_command_output(cmd, &output, &[])? {
            return Ok(());
        }

        if let Some(redirect) = &cmd.redirect_out {
            let target = self.expand_word(&redirect.target);
            if self.has_output_fd_target(&target) {
                self.write_output_fd_redirect(&target, &output)?;
                return Ok(());
            }
            if target == "&2" {
                crate::builtins::echo::write_echo_decoded(
                    echo_args.iter().map(String::as_str),
                    &mut std::io::stderr().lock(),
                )?;
                return Ok(());
            }
            if is_closed_redirect_target(&target) || is_null_device(&target) {
                crate::builtins::echo::write_echo_decoded(
                    echo_args.iter().map(String::as_str),
                    &mut std::io::sink(),
                )?;
                return Ok(());
            }
            let mut file = self.create_redirect_output(&target, redirect.clobber)?;
            crate::builtins::echo::write_echo_decoded(
                echo_args.iter().map(String::as_str),
                &mut file,
            )?;
            return Ok(());
        }

        if let Some(redirect) = &cmd.append {
            let target = self.expand_word(&redirect.target);
            if self.has_output_fd_target(&target) {
                let mut output = Vec::new();
                crate::builtins::echo::write_echo_decoded(
                    echo_args.iter().map(String::as_str),
                    &mut output,
                )?;
                self.write_output_fd_redirect(&target, &output)?;
                return Ok(());
            }
            if target == "&2" {
                crate::builtins::echo::write_echo_decoded(
                    echo_args.iter().map(String::as_str),
                    &mut std::io::stderr().lock(),
                )?;
                return Ok(());
            }
            if target == "&1" {
                crate::builtins::echo::write_echo_decoded(
                    echo_args.iter().map(String::as_str),
                    &mut std::io::stdout().lock(),
                )?;
                return Ok(());
            }
            if is_closed_redirect_target(&target) {
                crate::builtins::echo::write_echo_decoded(
                    echo_args.iter().map(String::as_str),
                    &mut std::io::sink(),
                )?;
                return Ok(());
            }
            let mut file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(shell_path_to_windows(&target, &self.shell_state.env_vars))?;
            crate::builtins::echo::write_echo_decoded(
                echo_args.iter().map(String::as_str),
                &mut file,
            )?;
            return Ok(());
        }

        self.write_default_stdout(&output)?;
        self.exit_code = 0;
        Ok(())
    }
}

fn recho_display_arg(arg: &str) -> String {
    // GNU support/recho.c strprint iterates over raw bytes: bytes < 0x20
    // become ^X, 0x7f becomes ^?, and all other bytes (including >= 0x80)
    // pass through verbatim. Rubash words carry bytes >= 0x80 and certain
    // C0 control bytes as U+E000 raw-byte marker pairs; iterate over the
    // original string so marker pairs are decoded to their byte values
    // without String::from_utf8_lossy replacing lone high bytes with
    // U+FFFD (nquote4.tests $'ab\x{cd}e' → ab<0xCD>e, not ab<FFFD>e).
    // High bytes are re-encoded as marker pairs so the output String stays
    // valid UTF-8 and write_buffered_builtin_output decodes them back to
    // raw bytes at the output boundary.
    use crate::executor::substitution_metadata::{
        encode_raw_byte_marker, RAW_BYTE_MARKER_ESCAPE, RAW_BYTE_MARKER_FIRST, RAW_BYTE_MARKER_LAST,
    };
    let mut output = String::new();
    let mut chars = arg.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch as u32 == RAW_BYTE_MARKER_ESCAPE {
            let peeked = chars.peek().copied();
            match peeked {
                Some(next_ch) if next_ch as u32 == RAW_BYTE_MARKER_ESCAPE => {
                    // Doubled sentinel: literal U+E000 in payload text.
                    chars.next();
                    output.push(ch);
                }
                Some(next_ch)
                    if (RAW_BYTE_MARKER_FIRST..=RAW_BYTE_MARKER_LAST)
                        .contains(&(next_ch as u32)) =>
                {
                    chars.next();
                    let byte = (next_ch as u32 - RAW_BYTE_MARKER_FIRST) as u8;
                    if byte < 0x20 {
                        output.push('^');
                        output.push((byte + 0x40) as char);
                    } else if byte == 0x7f {
                        output.push_str("^?");
                    } else {
                        // Re-encode high byte for write_buffered_builtin_output.
                        output.push_str(&encode_raw_byte_marker(byte));
                    }
                }
                _ => {
                    output.push(ch);
                }
            }
        } else if ch == '\x7f' {
            output.push_str("^?");
        } else if ch.is_ascii_control() {
            output.push('^');
            output.push(((ch as u8) + 0x40) as char);
        } else {
            output.push(ch);
        }
    }
    output
}
