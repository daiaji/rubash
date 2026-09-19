//! Executor Module - Bash Command Executor
//!
//! Executes parsed AST commands.

pub(crate) mod arithmetic;
pub(crate) mod glob;
pub(crate) mod path;
pub(crate) mod types;
pub(crate) use types::*;
mod upstream_scripts;
use arithmetic::{
    arithmetic_division_by_zero_token, arithmetic_unbound_variable, eval_arith_value,
    eval_conditional_arith_value, eval_conditional_arith_value_categorized,
    eval_conditional_arith_value_with_writes,
};

mod arrays;
use arrays::*;
mod alias_arithmetic_for;
mod alias_case;
mod alias_loop_match;
mod alias_loops;
mod alias_reparse;
mod alias_select;
mod alias_set_builtins;
mod arithmetic_aliases;
mod array_assignment_exec;
mod assignment_dispatch;
mod assignment_expansion;
mod builtin_direct_command;
mod builtin_redirects;
mod command_dispatch;
mod command_dispatch_late;
mod command_dispatch_primary;
mod command_execute;
mod command_input_scope;
mod command_no_alias;
mod command_no_alias_late;
mod command_prepare;
mod command_substitution;
mod command_substitution_pipelines;
mod command_substitution_values;
mod command_words;
mod compound_exec;
mod history_exec;
pub(crate) mod substitution_metadata;

/// Decode raw input bytes (script source read from stdin, command output,
/// embedded data) into rubash shell text. Invalid UTF-8 bytes are preserved
/// as raw-byte marker code points so lexer/executor paths round-trip them
/// byte-exactly instead of Latin-1 widening them with `byte as char`.
/// Embedders feeding bytes from `Read` sources should use this rather than
/// `String::from_utf8_lossy`, which would corrupt non-UTF-8 input.
pub fn bytes_to_shell_text(bytes: &[u8]) -> String {
    substitution_metadata::bytes_to_shell_text(bytes)
}

use command_words::raw_word_has_unquoted_parameter_expansion;
use compound_exec::*;
mod declare_local;
mod dynamic_arrays;
mod exec_profile;
pub(in crate::executor) use dynamic_arrays::env_derived_dynamic_parameter_value;
mod embedded_mutations;
mod embedded_parameters;
mod expand_braced_indices;
mod expand_braced_ops;
mod expand_braced_patterns;
mod expand_braced_replacement;
mod expand_braced_special;
mod expand_word;
mod export_builtin;
mod external_file_builtins;
mod external_finish;
mod external_inner;
mod external_redirects;
mod external_setup;
mod fd_table;
mod function_calls;
mod function_locals;
mod getopts_enable;
mod init;
mod job_builtins;
mod limit_builtins;
mod lookup_paths;
mod loop_select;
mod mapfile_builtin;
mod mapfile_helpers;
mod option_builtins;
mod parameter_core;
mod parameter_errors;
mod parameter_patterns;
mod parameter_transforms;
mod parameter_words;
mod printf_path_builtins;
mod prompt_expansion;
mod public_accessors;
mod pwd_loop_builtins;
mod read_builtin;
mod read_io;
mod read_redirected_fd;
mod readonly_functions;
mod shell_options;
#[cfg(windows)]
mod sudo_builtin;

pub(crate) use shell_options::{write_stderr_bytes, write_stdout_bytes, GlobalStdout};

/// Writer that sends output through the Win32 `WriteFile` stdout path,
/// matching the executor's `write_default_stdout` sink. Used by builtins
/// that need the same immediate, unbuffered write semantics as the main
/// output path when invoked without redirection.
pub(crate) struct WriteFileStdout;
impl std::io::Write for WriteFileStdout {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        write_stdout_bytes(buf).map(|()| buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Writer that sends output through the Win32 `WriteFile` stderr path,
/// matching the executor's `write_default_stderr` sink. Used by builtins
/// that need the same immediate, unbuffered write semantics as the main
/// diagnostic path when invoked without redirection.
pub(crate) struct WriteFileStderr;
impl std::io::Write for WriteFileStderr {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        write_stderr_bytes(buf).map(|()| buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

mod shift_echo_builtins;
mod source_type_state;
mod subscript_expansion;
pub(in crate::executor) use subscript_expansion::{
    IndexedSubscript, OperandSubscriptMode, SubscriptSource,
};
mod temporary_assignments;
mod trap_exec;
mod trap_stack_builtins;
mod type_builtin;
mod type_describe;
mod type_functions;
mod unset_arrays;
mod variable_state;

mod alias_helpers;
mod assignment_helpers;
mod ast_exec;
mod builtin_names;
mod command_subst_helpers;
mod command_text;
mod env_helpers;
mod execution_misc;
mod function_env;
mod local_helpers;
mod parameter_case;
mod parameter_decode;
mod parameter_ops;
mod parameter_replace;
mod parse_helpers;
mod pipeline_exec;
mod pipeline_stages;

mod read_helpers;
mod read_split;
mod redirect_inherit;
mod redirection;
mod select_exec;
mod support_names;

use crate::jobs::JobTable;
use crate::shell::state::ShellState;
use alias_helpers::*;
use assignment_helpers::*;
// Shared with builtins::declare for `declare -p` assoc rendering.
pub(crate) use assignment_helpers::{assoc_nbuckets, bash_assoc_order};
use builtin_names::*;
use command_subst_helpers::*;
use command_text::*;
pub(crate) use embedded_mutations::COMPOUND_EXPANSION_WS_TAG;
use env_helpers::*;
use execution_misc::*;
use external_setup::{
    command_needs_process_substitution_materialization, ProcessSubstitutionFiles,
};
use fd_table::{FdReadEndpoint, FdTable, FdWriteEndpoint, MaterializedRead};
use function_env::*;
use local_helpers::*;
use parameter_case::*;
use parameter_decode::*;
use parameter_ops::*;
use parameter_replace::*;
use parse_helpers::*;
use read_helpers::*;
pub(crate) use read_split::*;
use redirect_inherit::*;
use substitution_metadata::*;
use support_names::*;

pub(crate) mod conditional;
use conditional::{case_pattern_matches, case_pattern_matches_nocase, simple_grep_pattern_matches};

use crate::builtins::alias::Alias;
use crate::expand::tilde::tilde as tilde_expand;
use crate::lexer::TokenKind;
use crate::parser::{
    AndOrListCommand, ArithmeticCommand, ArithmeticExpressionMetadata, ArithmeticForCommand, Ast,
    BackgroundCommand, CaseClause, CaseCommand, CaseTerminator, CommandBodyKind, CommandNode,
    ConditionalCommand, ForCommand, FunctionBodyKind, FunctionCommand, IfCommand, InvertedCommand,
    LoopCommand, PipelineCommand, Redirect, SelectCommand, SubshellCommand, TimeCommand,
    WordMetadata,
};
use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::PathBuf;

pub struct HostExternalCommandOutput {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub status: i32,
}

struct HostExternalCommandHandler(
    Box<dyn FnMut(&[String], &HashMap<String, String>) -> Option<HostExternalCommandOutput>>,
);

impl std::fmt::Debug for HostExternalCommandHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("HostExternalCommandHandler(..)")
    }
}

#[cfg(windows)]
pub use crate::builtins::sudo::SudoMode;

#[cfg(windows)]
#[derive(Debug, Clone)]
pub struct ElevationRequest {
    pub command: Vec<String>,
    pub resolved_program: Option<PathBuf>,
    pub environment: HashMap<String, String>,
    pub current_dir: PathBuf,
    pub preserve_environment: bool,
    pub mode: SudoMode,
}

#[cfg(windows)]
pub struct ElevationOutput {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub status: i32,
}

#[cfg(windows)]
struct ElevationHandler(Box<dyn FnMut(ElevationRequest) -> Result<ElevationOutput, String>>);

#[cfg(windows)]
impl std::fmt::Debug for ElevationHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ElevationHandler(..)")
    }
}
use std::process::{Command, Stdio};
use std::rc::Rc;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use self::path::{
    apply_required_windows_child_environment, external_command_for_named_program, find_shell,
    find_user_command, shell_path_to_process, shell_path_to_windows, standard_path,
};

// NOTE: The executor's shared constants (env-var markers, fd-table key
// prefixes, etc.) live in `types.rs` and are re-exported via
// `pub(crate) use types::*;` above. Do not redeclare them here.

static EXECUTION_LOCK: Mutex<()> = Mutex::new(());

thread_local! {
    static EXECUTION_LOCK_DEPTH: Cell<usize> = const { Cell::new(0) };
}

enum NamerefResolution {
    Target(String),
    Circular,
    /// GNU variables.c:2022 find_variable_nameref: the chain exceeded
    /// NAMEREF_MAX (8) levels without resolving — reported as
    /// "maximum nameref depth (8) exceeded", not "circular".
    MaxDepth,
    /// GNU variables.c:2023-2026 find_variable_nameref: a marked nameref
    /// whose cell is missing, empty, or not a resolvable name resolves to
    /// nothing — the variable reads as unset for value lookups, while
    /// name-resolution contexts (assignment, unset) keep the variable
    /// itself (find_variable_nameref_for_assignment returns the nameref).
    Unresolved,
    NotNameref,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TypeDescribeMode {
    Verbose,
    Reusable,
    TypeOnly,
    PathOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LoopControlKind {
    Break,
    Continue,
}

type FunctionBody = Rc<Ast>;

/// Print/roundtrip metadata for a defined function that the body AST alone
/// does not carry. GNU keeps the whole FUNCTION_DEF command for
/// `declare -f`/`type NAME` rendering (print_cmd.c named_function_string /
/// print_function_def): the body kind decides whether the stored body is
/// wrapped in `( )` (cm_subshell body), and redirections attached to the
/// function definition itself (`f () { ... } 1>&2`) print after the closing
/// brace and travel in the exportstr so subshell children re-import them.
#[derive(Clone, Debug, Default)]
pub(in crate::executor) struct FunctionDefInfo {
    pub body_kind: Option<crate::parser::FunctionBodyKind>,
    pub def_redirects: Vec<crate::parser::Redirect>,
}

#[derive(Clone, Debug)]
struct FunctionDefinitionLocation {
    line: usize,
    source: String,
    /// GNU reports the body group's line for DEBUG fires inside a traced
    /// function (trap.tests `func2[43] debug`: `func2()` on 42, `{` on 43),
    /// so the plain-call path needs this in addition to the definition line.
    body_open_line: Option<usize>,
}

impl LoopControlKind {
    fn name(self) -> &'static str {
        match self {
            LoopControlKind::Break => "break",
            LoopControlKind::Continue => "continue",
        }
    }
}

/// Execution error
#[derive(Debug)]
pub enum ExecuteError {
    CommandNotFound(String),
    /// A direct host-side function dispatch requested a function that is not
    /// defined in the executor.
    FunctionNotFound(String),
    IoError(std::io::Error),
    ExitCode(i32),
    /// A word-expansion failure that aborts only the current command
    /// list: function frames absorb it as an early return carrying this
    /// status, `( )` frames end just the subshell, and a top-level list
    /// ends the noninteractive run (GNU probes f3/f4, 2026-08-24).
    ExpansionFailure(i32),
    /// A fatal function-definition error (GNU execute_cmd.c
    /// execute_intern_function with posixly_correct: last_command_exit_value
    /// = EX_BADUSAGE and jump_to_top_level(ERREXIT)). Under POSIX mode an
    /// invalid function name aborts the current subshell (parent continues)
    /// or, at script top level, ends the noninteractive run.
    FatalFunctionError(i32),
    Break(usize),
    Continue(usize),
    Return(i32),
    UnknownBuiltin(String),
    /// `exit N` called from a lastpipe stage (shopt -s lastpipe) runs in
    /// the current shell, so it must exit the current shell — not just set
    /// the pipeline's exit status.  This variant propagates through
    /// `execute_simple_pipeline` / `execute_pipeline_command` without being
    /// caught by the pipeline-status match in `execute_ast_inner`, and the
    /// top-level caller converts it back to `ExitCode(N)`.  GNU reference:
    /// execute_cmd.c:2758 `execute_command_internal(cmd, ...)` runs the
    /// lastpipe stage in the current shell; `exit` longjmps to top level.
    LastpipeExit(i32),
}

impl std::fmt::Display for ExecuteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExecuteError::CommandNotFound(cmd) => write!(f, "rubash: {}: command not found", cmd),
            ExecuteError::FunctionNotFound(name) => {
                write!(f, "rubash: {}: function not found", name)
            }
            ExecuteError::IoError(e) => write!(f, "rubash: {}", crate::posix_errors::message(e)),
            ExecuteError::ExitCode(code) => write!(f, "exit code: {}", code),
            ExecuteError::ExpansionFailure(code) => write!(f, "exit code: {}", code),
            ExecuteError::FatalFunctionError(code) => write!(f, "exit code: {}", code),
            ExecuteError::Break(level) => write!(f, "break {}", level),
            ExecuteError::Continue(level) => write!(f, "continue {}", level),
            ExecuteError::Return(status) => write!(f, "return {}", status),
            ExecuteError::UnknownBuiltin(name) => {
                write!(f, "rubash: {}: builtin command not found", name)
            }
            ExecuteError::LastpipeExit(code) => write!(f, "exit code: {}", code),
        }
    }
}

impl std::error::Error for ExecuteError {}

impl From<std::io::Error> for ExecuteError {
    fn from(e: std::io::Error) -> Self {
        ExecuteError::IoError(e)
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct VarAttrs {
    exported: bool,
    readonly: bool,
    integer: bool,
    uppercase: bool,
    lowercase: bool,
    nameref: bool,
    array: bool,
    assoc: bool,
    trace: bool,
    declared_unset: bool,
}

#[derive(Debug)]
struct SavedGlobalDeclareLocal {
    name: String,
    scope_index: usize,
    local_value: Option<String>,
    local_attrs: VarAttrs,
    local_typed: Option<crate::shell::Variable>,
}

/// Command executor
#[derive(Debug)]
pub struct Executor {
    shell_state: ShellState,
    fd_table: FdTable,
    job_table: JobTable,
    exit_code: i32,
    parse_error_occurred: bool,
    /// GNU exit.def:52 (sourced_logout): ~/.bash_logout runs at most once
    /// per shell process (bash_logout, exit.def:156-166).
    bash_logout_sourced: bool,
    env_vars: HashMap<String, String>,
    aliases: HashMap<String, Alias>,
    functions: HashMap<String, FunctionBody>,
    function_definition_redirects: HashMap<String, CommandNode>,
    function_def_infos: HashMap<String, FunctionDefInfo>,
    function_definition_locations: HashMap<String, FunctionDefinitionLocation>,
    positional_params: Vec<String>,
    pipestatus: Vec<i32>,
    function_name_stack: Vec<String>,
    bash_argc_stack: Vec<String>,
    bash_argv_stack: Vec<String>,
    bash_lineno_stack: Vec<String>,
    bash_source_stack: Vec<String>,
    local_var_scopes: Vec<HashMap<String, Option<String>>>,
    local_attr_scopes: Vec<HashMap<String, VarAttrs>>,
    local_typed_scopes: Vec<HashMap<String, Option<crate::shell::Variable>>>,
    expanding_aliases: Vec<String>,
    loop_depth: usize,
    pub(crate) function_depth: usize,
    /// GNU source.def: dollar vars changed by the set builtin during a
    /// sourced script (ARGS_SETBLTIN); gates whether source restores them.
    pub(crate) dollar_vars_changed_by_set: bool,
    random_state: RandomGen,
    shell_pid: u32,
    subshell_depth: Cell<usize>,
    owns_signal_mailbox: bool,
    last_background_pid: Option<u32>,
    background_children: HashMap<u32, std::process::Child>,
    background_jobs: HashMap<u32, String>,
    background_job_order: Vec<u32>,
    coproc_stdin_writers: HashMap<u32, std::io::PipeWriter>,
    /// GNU execute_cmd.c Coproc.c_name: the coproc name actually stored,
    /// after find_variable_nameref_for_create may rewrite it to the nameref
    /// cell. coproc_unsetvars (execute_cmd.c:2450) unbinds <c_name>_PID and
    /// check_unbind_variable(c_name) at reap time — even when coproc_bind
    /// failed (invalid identifier, readonly), so the name must be tracked
    /// independently of whether the *_PID variable exists.
    coproc_names: HashMap<u32, String>,
    coproc_stdout_readers: HashMap<u32, std::io::PipeReader>,
    coproc_stderr_forwarders: HashMap<u32, std::thread::JoinHandle<Result<(), std::io::Error>>>,
    assignment_output_process_substitutions: HashMap<String, String>,
    pending_scalar_assignment: bool,
    suppress_errexit: usize,
    debug_trap_running: bool,
    return_trap_running: bool,
    signal_trap_running: bool,
    /// GNU trap.c SIG_INPROGRESS: _run_trap_internal marks a trap
    /// in-progress before executing its action, and run_error_trap refuses
    /// to re-enter while it is set — an ERR trap action that itself fails
    /// must not recursively re-trigger the ERR trap (niubash #121).
    error_trap_running: bool,
    /// Child-death notifications that arrived while the SIGCHLD trap action
    /// was already running (a re-entrant reap drops them otherwise; bash
    /// re-runs the trap once per pending notification — trap.tests expects
    /// three "caught a child death" lines for three reaped background jobs).
    sigchld_notifications_pending: std::cell::Cell<usize>,
    /// GNU builtins/source.def:208-216 unsets the DEBUG trap for the
    /// duration of a sourced file when function_trace_mode is off; the
    /// unwind-protect only restores it after source_file's run_return_trap
    /// (evalfile.c:395), so the sourced file's top-level commands and the
    /// RETURN-trap action's own DEBUG fire are suppressed together
    /// (dbg-support.tests:98 emits only `debug lineno: 98 main`).
    source_debug_suppressed: bool,
    debug_trap_command: std::cell::RefCell<Option<String>>,
    debug_trap_function_line: Option<usize>,
    arithmetic_expansion_error: Cell<bool>,
    arithmetic_nonfatal_error: Cell<bool>,
    arithmetic_fatal_error: Cell<bool>,
    /// `set -u` unbound-variable error raised during arithmetic evaluation.
    /// A Cell because word-expansion paths hold `&self` (GNU expr.c raises
    /// FORCE_EOF; the shell exits 127 in -c mode).
    arithmetic_nounset_error: Cell<bool>,
    /// Error category reported by the most recent arithmetic evaluation that
    /// used the real shell environment (GNU expr.c reports fatality from the
    /// actual evaluation, not from a re-evaluation in a fresh environment).
    arithmetic_last_error_category:
        Cell<Option<crate::executor::arithmetic::ArithmeticErrorCategory>>,
    /// Expanded form of the offset/length expression from the last failed
    /// substring arithmetic evaluation. `report_substring_arithmetic_error`
    /// uses this so the error token reflects the post-expansion text (GNU
    /// evalexp runs after parameter/command substitution).
    arithmetic_last_error_expression: std::cell::RefCell<String>,
    /// Expanded+normalized expression text from the most recent
    /// `(( ... ))` evaluation — the exact string `evalexp` saw. GNU
    /// execute_arith_command (execute_cmd.c:3937) runs
    /// expand_arith_string before evalexp, so diagnostics echo the
    /// post-expansion text (`$A` shows its value, not the literal).
    arithmetic_last_eval_input: std::cell::RefCell<String>,
    /// GNU error.c builtin_error -> error_prolog: diagnostics raised while a
    /// builtin runs carry `this_command_name` (`declare: ...`, `getopts: ...`).
    /// Assignment paths deep inside apply_shell_assignment consult this so a
    /// failed `builtin name=value` names the builtin, while a bare
    /// `name=value` command prints no command segment.
    assignment_command_name: Option<String>,
    /// GNU builtins emit assignment diagnostics through builtin_error, which
    /// honors the builtin's redirected stderr and ordering. Builtin callers
    /// (getopts OPTARG/name binds) set buffer_assignment_diagnostics and drain
    /// pending_assignment_diagnostics into their buffered stderr so a
    /// preceding buffered diagnostic (e.g. `illegal option -- h`) keeps its
    /// position relative to the bind error.
    buffer_assignment_diagnostics: bool,
    pending_assignment_diagnostics: Vec<u8>,
    /// GNU subst.c: an assignment performed by `${var:=word}`/`${var=word}`
    /// that fails (invalid nameref target value, readonly, ...) is a word
    /// expansion error — expand_word_error -> exp_jump_to_top_level(DISCARD)
    /// abandons the rest of the command list. The := applier runs inside
    /// word expansion where it cannot return an error, so it latches here
    /// and execute_prepared_command raises ExpansionFailure(1).
    parameter_assignment_failure: Cell<bool>,
    /// GNU variables.c:3536 assign_in_env: names bound through `name=value
    /// cmd` temporary-environment assignments are live at the command's
    /// variable context while it runs — a function-local `declare -n r`
    /// sees a tempenv `r` as an existing variable (validating its value as
    /// the new cell), not as a fresh empty local. Tracked as a stack of the
    /// bound names so nested commands restore correctly.
    tempenv_names: Vec<String>,
    /// Frame marks pairing apply_temporary_assignments /
    /// restore_temporary_assignments so nested commands truncate the
    /// tempenv-name stack to the boundary recorded by the outer call.
    tempenv_marks: Vec<usize>,
    /// GNU variables.c:2604-2620 make_local_variable (was_tmpvar): a
    /// `declare`/`typeset`/`local` operand whose name is bound by this
    /// command's own `name=value` prefix promotes the tempenv binding to a
    /// frame local in place — `z=y typeset z` leaves a live exported local
    /// z=y for the rest of the frame instead of an empty local that pops
    /// with the command. Names listed here skip the command-end tempenv
    /// restore in restore_temporary_assignments.
    tempenv_promoted_names: Vec<String>,
    /// Pre-tempenv snapshots (env value, typed cell, attribute set) for the
    /// names bound by the innermost apply_temporary_assignments mark. The
    /// declare/local promotion path consults it so a promoted local's
    /// saved-previous is the value before the prefix applied, not the
    /// tempenv value itself (which would otherwise leak back into the
    /// global scope at frame restore).
    tempenv_previous: HashMap<String, (Option<String>, Option<crate::shell::Variable>, VarAttrs)>,
    /// GNU variables.c:4485-4525 (push_posix_temp_var): a posix special
    /// builtin's tempenv merged into a function context gets att_propagate
    /// when the bound cell is not a local, so the binding descends into the
    /// caller's context when the frame pops instead of dying with it. Names
    /// listed here skip function-call tempenv restores (like promoted
    /// names) until no live function tempenv covers them any more.
    tempenv_propagated_names: Vec<(String, VarAttrs)>,
    /// Base names bound by each active function call's own tempenv prefix —
    /// the function's variable context in GNU terms (variables.c
    /// push_context). Lets the posix merge path distinguish a function
    /// tempenv binding (propagates on frame pop) from a plain command
    /// tempenv binding (dies with its command scope).
    function_tempenv_names: Vec<Vec<String>>,
    /// GNU expr.c evalerror -> jump_to_top_level (DISCARD): an arithmetic
    /// evaluation failure — including a failed array-subscript evaluation —
    /// discards the rest of the command list that contained the failing
    /// command (`a[$x]=v; echo after` never prints `after`). Rubash models
    /// the discarded tail as the commands sharing the failing reader-level
    /// command's source line (`CommandNode::line`).
    pub(crate) evalerror_pending: Cell<bool>,
    /// Source line of the reader-level command whose list tail is being
    /// discarded; set lazily when the reader-level loop first observes
    /// `evalerror_pending`.
    evalerror_line: Cell<Option<usize>>,
    /// Nesting depth of `execute_ast_inner`: a nested list (function body,
    /// loop body, sourced text, ...) unwinds silently while an evalerror
    /// abort is pending; only the reader-level loop skips to the next
    /// source line.
    evalerror_exec_depth: Cell<usize>,
    /// Source line of the command currently executing at reader level —
    /// becomes the abort boundary when `evalerror_pending` is observed.
    reader_command_line: Cell<Option<usize>>,
    /// True while an if/elif condition list is executing: word-expansion
    /// failures must pierce function frames so the enclosing compound
    /// command can abandon itself entirely (GNU probe f4).
    pub(crate) inside_compound_condition: Cell<bool>,
    /// True while a scalar assignment RHS is expanding: GNU param_expand
    /// carries PF_ASSIGNRHS into `${!arr[@]}` so the unquoted `@` key list
    /// takes the dollar_at path (elements quoted, space-joined, never
    /// field-split) instead of the dollar_star IFS[0] join used for plain
    /// command words (subst.c string_list_pos_params). Fresh Executor
    /// instances (command substitution, subshells) start false, matching
    /// GNU dropping PF_ASSIGNRHS across a nested substitution boundary.
    pub(crate) inside_assignment_rhs: Cell<bool>,
    last_command_substitution_status: Cell<Option<i32>>,
    /// Tracks the source of the last heredoc EOF warning emitted from
    /// command_substitution_heredoc_output_mut_typed, to avoid duplicate
    /// warnings when the same comsub is expanded through multiple paths
    /// (expand_assignment_value_inner → expand_command_substitution_mut_typed
    /// and expand_embedded_parameters_mut → expand_command_substitution_mut).
    last_heredoc_warning_source: RefCell<Option<String>>,
    /// Leading newlines in the comsub source (before trimming). Used to
    /// adjust heredoc warning line numbers: when `$(` is at end of line,
    /// the comsub body starts on the next line.
    comsub_leading_newlines: Cell<usize>,
    /// A current-shell (`${ ...; }` / `${| ...; }`) body that ran `exit N`
    /// aborts the enclosing (sub)shell with status N (GNU subst.c: the
    /// nofork body shares the shell's exit path). The walker records the
    /// status here; command_execute converts it into ExecuteError::ExitCode
    /// after word expansion so the enclosing context unwinds.
    current_shell_substitution_exit: Cell<Option<i32>>,
    last_command_substitution_parse_error: Cell<bool>,
    /// GNU execute_cmd.c:4887-4888 sets `special_builtin_failed = 1` when a
    /// POSIX special builtin returns an error status (> EX_SHERRBASE = 256).
    /// After the command (execute_cmd.c:1004-1017), if `posixly_correct &&
    /// interactive == 0 && special_builtin_failed`, the shell exits with the
    /// converted status. Rubash sets this flag in each special builtin that
    /// returns EX_USAGE/EX_UTILERROR/etc., and checks it in
    /// `execute_materialized_command` to exit the noninteractive POSIX shell.
    special_builtin_failed: Cell<bool>,
    stdout_capture: Option<Vec<u8>>,
    stderr_capture: Option<Vec<u8>>,
    host_external_command_handler: Option<HostExternalCommandHandler>,
    #[cfg(windows)]
    elevation_handler: Option<ElevationHandler>,
    external_file_builtins_enabled: bool,
    process_env_snapshot: HashMap<String, String>,
    history_provider: Option<crate::history::SharedHistoryProvider>,
    /// The shell's own session history (bashhist.c the_history) for scripts
    /// that turn history on; None when history was never enabled.
    pub(crate) session_history: Option<Rc<RefCell<crate::history::SessionHistory>>>,
    last_notified_job_ids: HashSet<usize>,
    completion_specs: crate::builtins::complete::CompletionRegistry,
}

#[cfg(test)]
mod tests;
