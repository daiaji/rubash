//! Regressions for the removed C-class hardcoded stubs
//! (docs/admission-guard-audit-20260922.md C1/C5/C6/C7): each scenario must
//! produce GNU Bash 5.3.0 behavior through the real semantic path — signal
//! table lookup, the BRE `sed` engine, parser error propagation, and history
//! expansion quote state — with no source-text fingerprints.

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

fn write_script(dir: &Path, body: &str) -> std::path::PathBuf {
    fs::create_dir_all(dir).expect("mkdir");
    let path = dir.join("probe.sh");
    fs::write(&path, body).expect("write script");
    path
}

// ---------------------------------------------------------------------------
// C1: `kill -l` must translate through the real signal table for every spec
// form, not return a canned "HUP" for any word containing 128/+.
// ---------------------------------------------------------------------------

#[test]
fn kill_l_translates_arithmetic_and_names() {
    let dir = Path::new("target").join("cstub-kill");
    let script = write_script(
        &dir,
        "kill -l 1\nkill -l 129\nkill -l $((128 + 1))\nkill -l INT\nkill -l SIGINT\nkill -l 0x81\necho last_rc=$?\n",
    );
    let (stdout, stderr, code) = run_rubash(&[script.to_str().unwrap()]);
    assert_eq!(code, Some(0), "stderr: {stderr}");
    // GNU kill.def explain(): 129 -> "HUP" (fatal signals print without SIG),
    // a signal NAME prints its number (INT/SIGINT -> 2), and 0x81 is not a
    // decimal spec -> "invalid signal specification" rc=1.
    assert_eq!(stdout, "HUP\nHUP\nHUP\n2\n2\nlast_rc=1\n");
    assert!(
        stderr.contains("invalid signal specification"),
        "stderr: {stderr}"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn kill_l_does_not_hijack_lookalike_words() {
    // The removed stub keyed on `kill -l` + `128` + `+`: an arithmetic body
    // that merely CONTAINS 128/+ must evaluate normally.
    let (stdout, stderr, code) = run_rubash(&["-c", "echo $((1280 + 1)); kill -l $((128 + 9))"]);
    assert_eq!(code, Some(0), "stderr: {stderr}");
    assert_eq!(stdout, "1281\nKILL\n");
}

// ---------------------------------------------------------------------------
// C5: `sed` substitutions run through a real BRE engine — `\( \)` groups,
// backreferences, anchors, classes, `*` and the `g` flag — not per-test
// pattern fingerprints.
// ---------------------------------------------------------------------------

#[test]
fn sed_bre_groups_and_backrefs() {
    let dir = Path::new("target").join("cstub-sed");
    // The exact aliasconv shape, plus variants the fingerprint stub missed.
    let script = write_script(
        &dir,
        "printf 'foo\\tbar baz\\n' | sed \"s/^\\([a-zA-Z0-9_-]*\\)\\t\\(.*\\)$/mkalias \\1 '\\2'/\"\n\
         printf 'x9y\\tzz\\n' | sed \"s/^\\([a-z0-9]*\\)\\t\\(.*\\)$/got=\\1:\\2/\"\n",
    );
    let (stdout, stderr, code) = run_rubash(&[script.to_str().unwrap()]);
    assert_eq!(code, Some(0), "stderr: {stderr}");
    assert_eq!(stdout, "mkalias foo 'bar baz'\ngot=x9y:zz\n");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn sed_bre_anchors_classes_star_and_g() {
    let (stdout, stderr, code) = run_rubash(&[
        "-c",
        concat!(
            "echo 'a.b.c' | sed 's/\\..*$/T/';",
            "echo 'a.b.c' | sed 's/^.*\\./T/';",
            "echo 'ab1cb2c' | sed 's/[0-9]/X/g';",
            "echo 'hello' | sed 's/l\\+/L/'"
        ),
    ]);
    assert_eq!(code, Some(0), "stderr: {stderr}");
    // GNU sed: `^.*\.' is greedy (eats through the last dot -> "Tc");
    // `l\+' matches the whole `ll' run once -> "heLo".
    assert_eq!(stdout, "aT\nTc\nabXcbXc\nheLo\n");
}

#[test]
fn sed_bre_no_match_and_first_only() {
    let (stdout, stderr, code) = run_rubash(&[
        "-c",
        "echo 'aaa' | sed 's/a/b/'; echo 'qqq' | sed 's/z/Z/'; echo rc=$?",
    ]);
    assert_eq!(code, Some(0), "stderr: {stderr}");
    // s/// without /g replaces only the FIRST occurrence (GNU semantics;
    // the previous literal-replace arm rewrote all).
    assert_eq!(stdout, "baa\nqqq\nrc=0\n");
}

// ---------------------------------------------------------------------------
// C6: eval syntax errors propagate from the parser — stray `{` after a
// redirection, and `}` lost inside `${` — with eval line numbering.
// ---------------------------------------------------------------------------

#[test]
fn eval_reports_stray_brace_after_redirect() {
    let (stdout, stderr, code) =
        run_rubash(&["-c", "eval 'x() { _;}>_[$($())] { echo vuln;}'; echo rc=$?"]);
    assert_eq!(stdout, "rc=2\n");
    assert!(
        stderr.contains("syntax error near unexpected token `{'"),
        "stderr: {stderr}"
    );
    assert!(
        stderr.contains("x() { _;}>_[$($())] { echo vuln;}"),
        "stderr: {stderr}"
    );
    assert_eq!(code, Some(0));
}

#[test]
fn eval_reports_unclosed_dollar_brace_eof() {
    let (stdout, stderr, code) = run_rubash(&[
        "-c",
        "eval 'foo() { _; } >_[${ $() }] ;{ echo eval ok; }'; echo rc=$?",
    ]);
    assert_eq!(stdout, "rc=2\n");
    assert!(
        stderr.contains("unexpected EOF while looking for matching `}'"),
        "stderr: {stderr}"
    );
    assert_eq!(code, Some(0));
}

// ---------------------------------------------------------------------------
// C7: `\!` inside double quotes keeps the backslash (history.c: `\!` is only
// special unquoted); outside quotes it is quote-removed to `!`. Carried by
// real quote-state, not a raw-text rescan.
// ---------------------------------------------------------------------------

#[test]
fn history_bang_backslash_quote_state() {
    let (stdout, stderr, code) = run_rubash(&[
        "-c",
        "set -o history\nset -H\necho \"\\!\"\necho \\!\necho \"$( echo \"\\!\" )\"\necho 'a\\!b'\n",
    ]);
    assert_eq!(code, Some(0), "stderr: {stderr}");
    assert_eq!(stdout, "\\!\n!\n\\!\na\\!b\n");
}
