//! Shell semantic state — the single isolation boundary for subshells.
//!
//! GNU reference: `execute_cmd.c:1576 execute_in_subshell` runs `( list )`
//! and `subst.c:7143 command_substitute` runs `$( )` in forked children, so
//! every mutable shell datum — variables, aliases, functions, positional
//! parameters, local scopes, completion specs, the history list, job
//! bookkeeping — is isolated by the fork copy and discarded on child exit.
//! Rubash executes flat subshells and command substitutions in-process, so
//! the same boundary is expressed as `ShellState::clone`: cloning produces
//! the child's state, and assigning the parent's saved clone back restores
//! it. New semantic shell state MUST be added here — never as an Executor
//! field — so isolation stays automatic instead of depending on
//! hand-maintained save/restore lists that drift (the alias/function leak
//! that motivated this structure came from exactly such a list).

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use crate::builtins::alias::Alias;
use crate::builtins::complete::CompletionRegistry;
use crate::executor::{
    FunctionBody, FunctionDefInfo, FunctionDefinitionLocation, RandomGen, VarAttrs,
};
use crate::history::SessionHistory;
use crate::parser::CommandNode;
use crate::shell::Variable;

use super::variables::VariableStore;

#[derive(Debug)]
pub struct ShellState {
    /// variables.c shell_variables — typed store carrying attributes,
    /// arrays, and assoc maps.
    pub(crate) variables: VariableStore,
    /// variables.c shell_variables flat mirror — env/option carrier map.
    /// Kept in sync with `variables` at mutation sites during the ongoing
    /// typed-store migration.
    pub(crate) env_vars: HashMap<String, String>,
    /// alias.c aliases.
    pub(crate) aliases: HashMap<String, Alias>,
    /// variables.c shell_functions.
    pub(crate) functions: HashMap<String, FunctionBody>,
    /// Redirect words parsed at function definition time, applied at call.
    pub(crate) function_definition_redirects: HashMap<String, CommandNode>,
    /// Definition-shape metadata (body kind, def redirects).
    pub(crate) function_def_infos: HashMap<String, FunctionDefInfo>,
    /// Definition source locations for DEBUG trap reporting.
    pub(crate) function_definition_locations: HashMap<String, FunctionDefinitionLocation>,
    /// variables.c positional_params ($1..$#, $*, $@).
    pub(crate) positional_params: Vec<String>,
    /// execute_cmd.c/jobs.c pipeline status vector ($PIPESTATUS).
    pub(crate) pipestatus: Vec<i32>,
    /// variables.c FUNCNAME stack.
    pub(crate) function_name_stack: Vec<String>,
    /// variables.c BASH_ARGC/BASH_ARGV stacks.
    pub(crate) bash_argc_stack: Vec<String>,
    pub(crate) bash_argv_stack: Vec<String>,
    /// variables.c BASH_LINENO/BASH_SOURCE stacks.
    pub(crate) bash_lineno_stack: Vec<String>,
    pub(crate) bash_source_stack: Vec<String>,
    /// variables.c variable_context — local scope stacks.
    pub(crate) local_var_scopes: Vec<HashMap<String, Option<String>>>,
    pub(crate) local_attr_scopes: Vec<HashMap<String, VarAttrs>>,
    pub(crate) local_typed_scopes: Vec<HashMap<String, Option<Variable>>>,
    /// alias.c expansion recursion guard stack.
    pub(crate) expanding_aliases: Vec<String>,
    /// execute_cmd.c loop_level — break/continue scope boundary.
    pub(crate) loop_depth: usize,
    /// variables.c funcnest / function call depth.
    pub(crate) function_depth: usize,
    /// variables.c dollar_vars_changed bookkeeping for `set` scope.
    pub(crate) dollar_vars_changed_by_set: bool,
    /// lib/sh/random.c RANDOM/SRANDOM generator state.
    pub(crate) random_state: RandomGen,
    /// execute_cmd.c subshell_level.
    pub(crate) subshell_depth: Cell<usize>,
    /// jobs.c bookkeeping — descriptions only; process handles are
    /// Executor resources and never cloned.
    pub(crate) last_background_pid: Option<u32>,
    pub(crate) background_jobs: HashMap<u32, String>,
    pub(crate) background_job_order: Vec<u32>,
    pub(crate) coproc_names: HashMap<u32, String>,
    /// jobs.c job-notification dedup set.
    pub(crate) last_notified_job_ids: HashSet<usize>,
    /// pcomplete.c completion spec registry.
    pub(crate) completion_specs: CompletionRegistry,
    /// bashhist.c per-session history list. Cloned deeply (not the Rc) so a
    /// subshell's `history` mutations cannot reach the parent — GNU gets
    /// the same isolation from the fork copy.
    pub(crate) session_history: Option<Rc<RefCell<SessionHistory>>>,
    /// Arithmetic expansion error flags (subshell boundary).
    /// GNU expr.c raises FORCE_EOF on unbound variable under `set -u`;
    /// these flags isolate errors in command substitutions from the outer shell.
    /// TODO: Move to ShellState::clone() for automatic isolation (requires &mut self API change)
    pub(crate) arithmetic_expansion_error: Cell<bool>,
    pub(crate) arithmetic_nonfatal_error: Cell<bool>,
    pub(crate) arithmetic_fatal_error: Cell<bool>,
    pub(crate) arithmetic_nounset_error: Cell<bool>,
    pub(crate) arithmetic_last_error_category:
        Cell<Option<crate::executor::arithmetic::ArithmeticErrorCategory>>,
    /// Bad substitution flag (subshell boundary).
    /// GNU subst.c:10277 - a mid-expansion bad substitution kills the
    /// substitution's command list, never the enclosing word's command.
    /// TODO: Move to ShellState::clone() for automatic isolation (requires &mut self API change)
    pub(crate) parameter_bad_substitution: Cell<bool>,
}

impl Clone for ShellState {
    fn clone(&self) -> Self {
        Self {
            variables: self.variables.clone(),
            env_vars: self.env_vars.clone(),
            aliases: self.aliases.clone(),
            functions: self.functions.clone(),
            function_definition_redirects: self.function_definition_redirects.clone(),
            function_def_infos: self.function_def_infos.clone(),
            function_definition_locations: self.function_definition_locations.clone(),
            positional_params: self.positional_params.clone(),
            pipestatus: self.pipestatus.clone(),
            function_name_stack: self.function_name_stack.clone(),
            bash_argc_stack: self.bash_argc_stack.clone(),
            bash_argv_stack: self.bash_argv_stack.clone(),
            bash_lineno_stack: self.bash_lineno_stack.clone(),
            bash_source_stack: self.bash_source_stack.clone(),
            local_var_scopes: self.local_var_scopes.clone(),
            local_attr_scopes: self.local_attr_scopes.clone(),
            local_typed_scopes: self.local_typed_scopes.clone(),
            expanding_aliases: self.expanding_aliases.clone(),
            loop_depth: self.loop_depth,
            function_depth: self.function_depth,
            dollar_vars_changed_by_set: self.dollar_vars_changed_by_set,
            random_state: self.random_state.clone_state(),
            subshell_depth: Cell::new(self.subshell_depth.get()),
            last_background_pid: self.last_background_pid,
            background_jobs: self.background_jobs.clone(),
            background_job_order: self.background_job_order.clone(),
            coproc_names: self.coproc_names.clone(),
            last_notified_job_ids: self.last_notified_job_ids.clone(),
            completion_specs: self.completion_specs.clone(),
            session_history: self
                .session_history
                .as_ref()
                .map(|s| Rc::new(RefCell::new(s.borrow().clone()))),
            arithmetic_expansion_error: Cell::new(self.arithmetic_expansion_error.get()),
            arithmetic_nonfatal_error: Cell::new(self.arithmetic_nonfatal_error.get()),
            arithmetic_fatal_error: Cell::new(self.arithmetic_fatal_error.get()),
            arithmetic_nounset_error: Cell::new(self.arithmetic_nounset_error.get()),
            arithmetic_last_error_category: Cell::new(self.arithmetic_last_error_category.get()),
            parameter_bad_substitution: Cell::new(self.parameter_bad_substitution.get()),
        }
    }
}
