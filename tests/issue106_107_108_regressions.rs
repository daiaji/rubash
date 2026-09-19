//! Regressions for niubash issues #106 / #107 / #108 (v1.1.2, rubash
//! b4cbcadc). Each test pins the GNU bash 5.3.0(1) baseline the reports
//! quote.
//!
//! #106 — CRLF scripts: v1.1.1 (str::lines()) accepted CRLF; the GNU-fidelity
//! rework of the line reader started keeping the trailing '\r' as word text,
//! so `if`/`then`/`fi` in a CRLF file were executed as external commands.
//! The lexer now treats '\r\n' as a line terminator (a lone '\r' not
//! followed by '\n' is still ordinary word text).
//!
//! #107 — `niu -c -l '<script>'` used to take the option right after `-c`
//! as the command string; GNU reads commands from the FIRST NON-OPTION
//! argument (bash manual, "If the -c option is present ...").
//!
//! #108 — `$(type -t ls)` leaked `file` to the process stdout and assigned
//! an empty string: the `type` dispatch only used the capture-aware path
//! when the command carried an explicit output redirect.

use std::fs;
use std::path::Path;
use std::process::Command;

fn run_rubash(args: &[&str]) -> (String, String, Option<i32>) {
    let output = Command::new(env!("CARGO_BIN_EXE_rubash"))
        .args(args)
        .output()
        .expect("run rubash");
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
        output.status.code(),
    )
}

fn write_crlf_script(rel_name: &str, body: &str) -> std::path::PathBuf {
    let path = Path::new("target").join(rel_name);
    fs::create_dir_all("target").expect("create target dir");
    fs::write(&path, body.replace('\n', "\r\n")).expect("write CRLF script");
    path
}

// ---------------------------------------------------------------------------
// #106 — CRLF scripts must parse again
// ---------------------------------------------------------------------------

#[test]
fn crlf_compound_commands_are_not_treated_as_external_commands() {
    let path = write_crlf_script(
        "issue106-compound.niu",
        "if true; then\n  echo compound-ok\nelse\n  echo compound-bad\nfi\n",
    );
    let (stdout, stderr, code) = run_rubash(&[
        "-c",
        &format!(". '{}'", path.to_string_lossy().replace('\\', "/")),
    ]);
    let _ = fs::remove_file(&path);
    assert_eq!(code, Some(0), "stderr: {stderr}");
    assert_eq!(stdout, "compound-ok\n");
    assert!(
        !stderr.contains("command not found"),
        "compound keywords leaked as commands: {stderr}"
    );
}

#[test]
fn crlf_heredoc_delimiter_matches_and_body_is_clean() {
    let path = write_crlf_script(
        "issue106-heredoc.niu",
        "cat <<EOF\nhello\nEOF\necho heredoc-done\n",
    );
    let (stdout, stderr, code) = run_rubash(&[
        "-c",
        &format!(". '{}'", path.to_string_lossy().replace('\\', "/")),
    ]);
    let _ = fs::remove_file(&path);
    assert_eq!(code, Some(0), "stderr: {stderr}");
    assert_eq!(stdout, "hello\nheredoc-done\n");
}

#[test]
fn lone_carriage_return_without_newline_is_still_word_text() {
    // The GNU-fidelity case the v1.1.2 line-reader change was made for:
    // a '\r' NOT followed by '\n' stays part of the word.
    let (stdout, _stderr, code) = run_rubash(&[
        "-c",
        "set \"\"$'\\r'; [ -z \"$1\" ] || echo got-nonempty; echo rc-$?",
    ]);
    assert_eq!(code, Some(0));
    // GNU bash: `set ""<CR>` makes $1 a bare CR (non-empty).
    assert_eq!(stdout, "got-nonempty\nrc-0\n");
}

// ---------------------------------------------------------------------------
// #107 — `bash -c` option parsing follows GNU "first non-option argument"
// ---------------------------------------------------------------------------

#[test]
fn c_flag_skips_leading_login_option() {
    let (stdout, stderr, code) = run_rubash(&["-c", "-l", "echo hi"]);
    assert_eq!(code, Some(0), "stderr: {stderr}");
    assert_eq!(stdout, "hi\n");
}

#[test]
fn c_flag_skips_leading_posix_option() {
    let (stdout, stderr, code) = run_rubash(&["-c", "--posix", "echo posix-ok"]);
    assert_eq!(code, Some(0), "stderr: {stderr}");
    assert_eq!(stdout, "posix-ok\n");
}

#[test]
fn c_flag_without_command_string_is_a_usage_error() {
    // GNU 5.3.0: `bash -c` -> "bash: -c: option requires an argument", rc 2.
    let (stdout, stderr, code) = run_rubash(&["-c"]);
    assert_eq!(code, Some(2));
    assert!(stdout.is_empty());
    assert_eq!(stderr, "bash: -c: option requires an argument\n");
}

#[test]
fn c_flag_command_string_still_binds_argv0_and_positionals() {
    let (stdout, stderr, code) = run_rubash(&["-c", "echo $0,$1,$2", "prog", "one", "two"]);
    assert_eq!(code, Some(0), "stderr: {stderr}");
    assert_eq!(stdout, "prog,one,two\n");
}

// ---------------------------------------------------------------------------
// #108 — `type` output must reach the command-substitution capture
// ---------------------------------------------------------------------------

#[test]
fn type_t_in_command_substitution_is_captured() {
    let (stdout, stderr, code) = run_rubash(&["-c", "A=$(type -t ls); echo \"A=[$A]\""]);
    assert_eq!(code, Some(0), "stderr: {stderr}");
    assert_eq!(stdout, "A=[file]\n");
}

#[test]
fn type_t_builtin_target_in_command_substitution_is_captured() {
    let (stdout, stderr, code) = run_rubash(&["-c", "A=$(type -t cd); echo \"A=[$A]\""]);
    assert_eq!(code, Some(0), "stderr: {stderr}");
    assert_eq!(stdout, "A=[builtin]\n");
}

#[test]
fn type_t_explicit_redirect_still_works() {
    let output_path = Path::new("target").join("issue108-type-redirect.txt");
    let _ = fs::remove_file(&output_path);
    let (stdout, stderr, code) = run_rubash(&[
        "-c",
        &format!(
            "type -t ls > {}",
            output_path.to_string_lossy().replace('\\', "/")
        ),
    ]);
    assert_eq!(code, Some(0), "stderr: {stderr}");
    assert!(
        stdout.is_empty(),
        "redirected type leaked to stdout: {stdout}"
    );
    let contents = fs::read_to_string(&output_path).expect("read redirect target");
    let _ = fs::remove_file(&output_path);
    assert_eq!(contents, "file\n");
}
