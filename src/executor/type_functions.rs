use super::*;

impl Executor {
    /// `type NAME` (verbose) function description. The definition body is
    /// rendered by the GNU print_cmd.c port so `type` and `declare -f`
    /// agree byte-for-byte with upstream.
    pub(in crate::executor) fn write_function_description<W>(
        &self,
        name: &str,
        body: &[CommandNode],
        stdout: &mut W,
    ) -> Result<(), ExecuteError>
    where
        W: Write,
    {
        writeln!(stdout, "{name} is a function")?;
        let info = self.shell_state.function_def_infos.get(name);
        let text = crate::parser::ast_print::multiline_function_def_text_with(
            name,
            body,
            info.and_then(|info| info.body_kind),
            info.map(|info| info.def_redirects.as_slice())
                .unwrap_or(&[]),
        );
        writeln!(stdout, "{text}")?;
        Ok(())
    }

    pub(in crate::executor) fn print_function_description(&self, name: &str, body: &[CommandNode]) {
        // Output must go through the capture-aware global stdout so command
        // substitution and pipeline stages see `type foo` output. Plain
        // println! bypasses the thread-local capture and leaks into the
        // surrounding stdout (type2.sub: eval "$(type foo | sed 1d)").
        use crate::executor::shell_options::GlobalStdout;
        use std::io::Write;
        let mut stdout = GlobalStdout;
        let info = self.shell_state.function_def_infos.get(name);
        let text = crate::parser::ast_print::multiline_function_def_text_with(
            name,
            body,
            info.and_then(|info| info.body_kind),
            info.map(|info| info.def_redirects.as_slice())
                .unwrap_or(&[]),
        );
        let _ = write!(stdout, "{name} is a function\n{text}\n");
    }
}
