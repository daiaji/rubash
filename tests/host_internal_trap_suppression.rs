//! Regression: commands an embedding host executes after the script
//! reached EOF (rc cleanup, hook probes) must not fire the user's
//! DEBUG/RETURN/ERR traps — GNU has no such housekeeping commands, so
//! every extra fire is a phantom trap line the product layer shows as a
//! suite diff (engine sinking list S2, dbg-support/dbg-support2).

use rubash::executor::Executor;
use rubash::lexer::tokenize;
use rubash::parser::parse;

fn line_count(path: &std::path::Path) -> usize {
    std::fs::read_to_string(path)
        .map(|body| body.lines().count())
        .unwrap_or(0)
}

#[test]
fn housekeeping_commands_after_script_eof_do_not_fire_traps() {
    let stamp = std::env::temp_dir().join(format!("rubash-s2-{}", std::process::id()));
    let dt = stamp.with_extension("dt");
    let rt = stamp.with_extension("rt");
    let _ = std::fs::remove_file(&dt);
    let _ = std::fs::remove_file(&rt);
    let dt_str = dt.to_string_lossy().replace('\\', "/");
    let rt_str = rt.to_string_lossy().replace('\\', "/");

    let mut executor = Executor::new();
    let script = format!(
        "trap 'echo DT >>{dt_str}' DEBUG\n\
         trap 'echo RT >>{rt_str}' RETURN\n\
         f() {{ echo x; }}\n\
         echo last\n"
    );
    let ast = parse(&tokenize(&script));
    executor.execute_ast(&ast).expect("script runs");

    // f() never ran, so only the three DEBUG fires exist (trap, f-def, echo).
    let dt_before = line_count(&dt);
    assert!(dt_before >= 2, "DEBUG trap should have fired: {dt_before}");

    // Unsuspended housekeeping keeps firing (control case).
    let ast = parse(&tokenize("declare -F nobody_home"));
    executor.execute_ast(&ast).expect("probe runs");
    assert_eq!(line_count(&dt), dt_before + 1);

    // Suspended housekeeping — the niubash shutdown-path pattern
    // (`unset HISTFILE`, `declare -F <hook>`) — must not fire at all.
    executor.with_traps_suspended(|exec| {
        let ast = parse(&tokenize("unset HISTFILE\ndeclare -F nobody_home\nf"));
        exec.execute_ast(&ast).expect("housekeeping runs");
    });
    assert_eq!(line_count(&dt), dt_before + 1, "DEBUG fired during suspend");
    assert_eq!(line_count(&rt), 0, "RETURN fired during suspend");

    // Nested suspension unwinds correctly: traps fire again afterwards.
    let ast = parse(&tokenize("declare -F nobody_home"));
    executor.execute_ast(&ast).expect("probe runs");
    assert_eq!(line_count(&dt), dt_before + 2);
}
