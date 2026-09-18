# Master Audit Summary — 2026-09-18

Baseline: `docs/audit-baseline-2026-09-18.md` @ `2494bc57`, 83 suites, 1833 stdout diff lines, 42 zero-diff.
Eight subsystem audits, one report per family (linked below). Every class carries GNU C `file:line` + Rubash owner `file:line`; all repros byte-verified against WSL GNU Bash 5.3.0.

| Report | Suites | Diff lines | Classes | Worst severity |
|---|---|---|---|---|
| `nameref.md` | nameref | 281 | 16 | Critical |
| `assoc.md` | assoc | 171 | 14 | High |
| `array-quotearray.md` | array, quotearray | 201 | 19 | **Critical (injection)** |
| `errors-type.md` | errors, type, posix2, set-e, invocation | 174 | ~10 | High |
| `history-histexp.md` | history, histexp | 200 | 4 | Critical (but mostly baseline artifact) |
| `expansion-cluster.md` | comsub×3, new-exp, iquote, extglob, cond, nquote, procsub, exp×2, posixexp | ~220 | 9 clusters | High |
| `io-redir-jobs.md` | jobs, redir, vredir, read, coproc, glob×2, shopt, test, braces, ifs-posix, intl, set-x, posixpipe | ~290 | ~15 | High |
| `varenv-alias-arith.md` | varenv, alias, arith | 206 | ~15 | High |

## Baseline corrections (measurement artifacts, NOT rubash bugs)

1. **`history` gnu.rc=137** — interactive `bash --norc -i` hangs on non-tty piped stdin under WSL; gnu.out truncated at history4. Everything "rb printed extra" past that point is a dead baseline. The `history -a` 8-vs-2-line diff is a stale `tmp/newhistory` left by SIGKILL skipping `trap rm`. True residual: H1+H2 only.
2. **`jobs` gnu.rc=124 AND rb.rc=124** — both sides hit the 40s harness timeout; the 64-line diff is a lower bound, not a full account.
3. **`arith` 53↔57** across identical runs — `$RANDOM` histogram indices (arith3); mark RANDOM-flow diffs nondeterministic.
4. **`errors` ~125 of 129 lines** — GNU imports harness `OLDPWD=/mnt/d/repo/rubash` (`variables.c:952` + `OLDPWD_CHECK_DIRECTORY` config-top.h:183); `cd -` leaves the test dir so GNU's own `./errorsN.sub` lookups 404. Rubash deletes OLDPWD at `init.rs:135`. The giant diff is an *import-semantics* bug, not an execution-fatality bug.
5. **`ifs-posix`** — rubash killed at 45s with zero output: a perf problem (6856 iterations), not correctness.
6. **`posix2`** — WSL interop forwards NO env to Windows rubash.exe; `init.rs:113` fills TMPDIR with `C:\Users\ADMINI~1\...` whose backslashes get eaten on child re-parse → 127 cascade. Platform/env boundary, not suite semantics.

## Cross-suite root causes (fix once, retire multiple ledgers)

### RC-1 — In-process `${THIS_SH}` child isolation (CRITICAL, hits 4+ suites)
`execute_direct_shell_script` (`src/executor/external_finish.rs:45-258`) runs child scripts on the same `Executor`:
- `ExecuteError::ExpansionFailure` passes through `:251-257` and kills the parent loop (nameref C1 — truncates the suite at nameref11, ~120 lines).
- `child_shell_environment` (`:260`) keeps only `EXPORTED_VARS`; `ASSOC_VARS`/`ARRAY_VARS`/`READONLY_VARS` marks and `BASH_VERSION`/`Executor::new` init (`init.rs:194`) never reach the child (assoc C1, history H1 — `${!BASH_CMDS[@]}` → `0 1`).
- `readonly`/`export` write `std::env` (`builtins/setattr/apply.rs:66,166`) and lookups fall back to `env::var` — non-exported readonly vars leak INTO the child (nameref C1 leak channel).
- History children bypass the line driver (`main.rs:962`/`1032`) — no recording, no `!` expansion, shared parent `session_history` (history H1).

### RC-2 — `[@]`/`$@` modeled as join-then-resplit string (High)
GNU carries a word list with `W_HASQUOTEDNULL` provenance (`subst.c:2957`); rubash joins to a string then re-splits. Produces C3/C4/C9 in array-quotearray (`${foo}"${a[@]}"` collapse, `b=${*/a/x}` IFS[0] join, `"${a[@]:-y}"` empty-field collapse).

### RC-3 — `array_expand_once` shopt registered but never consumed (CRITICAL — code injection)
`$(…)` inside array subscripts executes (`array_assignment_exec.rs:309`, `arithmetic/mod.rs:236`; shopt stub at `shopt.rs:252`). Same family: `${#a[$(…)]}` double-expands (`expand_braced_indices.rs:135`). GNU: `arrayfunc.c:1355` AV_NOEXPAND.

### RC-4 — Three parallel hand-rolled subscript lexers (rubash#117 exemplar)
`parser/array_element_assignment.rs:154`, `arithmetic/mod.rs:785`, `declare/storage/assoc.rs:197` each diverge from GNU `skip_matched_pair`/`skipsubscript` (`subst.c:2086-2189`). Plus three parallel comsub `)`-close scanners (`continuation.rs:659`, `skip.rs:326`, `heredoc_scan.rs:1`) each missing different `parse.y` states (`#` comments, `)` heredoc delimiters, case parens).

### RC-5 — Word-level comsub execution replaces `parse_and_execute` (#117 canonical)
`split_shell_words_with_quote_info` + `expand_aliases` + `comsub_body_alias_splice` (`command_substitution.rs:243-246`, `arithmetic_aliases.rs:457`) → alias loss, `IFS: command not found` on stderr. GNU has NO word-level substitution shortcut; `command_substitute` (`subst.c:7143`) goes through `parse_and_execute`.

### RC-6 — `echo` post-expansion quote removal (`echo.rs:106-108`)
`echo '"abc"'` → `abc`; consumer-side patch for a producer-side hole (admitted in its own comment), plus an `args == ["hi)"]` symptom hack. Producer hole belongs in expansion, not echo.

## Other notable single-owner findings

- `type -f` flag inverted (`type_builtin.rs:162` vs `type.def:153` CDESC_NOFUNCS).
- coproc-in-function `}` abort (`coproc_command.rs:8-68` vs `parse.y:1125-1174`).
- `exec 0<` doesn't replace script input (`main.rs:837`); `&` unconditional `Stdio::null()` (`compound_exec.rs:89` vs `execute_cmd.c:2835-2841`); `&>` drops stderr only on function calls (`function_env.rs:212-226`).
- posixpipe suite-name literal `4` fast path (`external_finish.rs:296-327`) — #117-class, delete.
- Function temporary-environment model: `local` drops export/readonly attrs through `call_function_inner`/`execute_local` (varenv V1/V2).
- `set -e`: `!` never propagates `CMD_IGNORE_RETURN` into pipeline elements (`pipeline_exec.rs:411` vs `execute_cmd.c:650-656`).
- Bash 5.3 `${ command; }` valsub/`${|command; }` funsub: wrong invariant codified at `embedded_mutations.rs:867-868` (comsub2's 48 lines).
- `BASH_XTRACEFD` unimplemented (set-x); `-ef` NTFS hardlink / `-N` timestamp semantics (test); glob sort uses byte-order not `strcoll`/`strvec_posixcmp`.
- `NAMEREF_MAX=8` unimplemented (`variables.h:181`); `unset -n` deletes scalars (`variables.c:3807`); `${!foo-unset}` swallows unset-ness (`subst.c:7910`).

## Environment-owned (do not fix in rubash)

- `/bin/sh` absent on Windows (jobs3, histexp.tests:62, coproc).
- `/usr/bin/` argv0 prefixes, `od`/`cp`/`expr` formatting (WinuxCmd).
- Windows-invalid filenames (`a:b`, `x*` redirection targets).
- `2>&1 |` stderr interleave ordering (known buffering limitation).
- iquote `'177` divergence is state-dependent eval re-lex, not a blanket eval bug.

## Suggested fix order

1. **RC-1** child-shell isolation — largest single multiplier (nameref ~120, assoc C1, history H1, varenv propagation).
2. **RC-3** `array_expand_once` consumption — security-grade (executes `$(…)`).
3. **RC-2** word-list provenance for `[@]`/`$@`.
4. **RC-4/RC-5** converge scanners/shortcuts to `skip_matched_pair` + `parse_and_execute` (#117 direction).
5. Per-suite singles: `type -f`, coproc-in-function, `exec 0<`, `&>`, `OLDPWD` import, valsub/funsub model.

---

## Design plans (post-audit, 2026-09-18)

Four read-only design agents produced implementation plans for the top root
causes; full texts preserved in the Devin session transcript (session
f0f292b805a340e8). Key design decisions:

- **RC-1 (child isolation)**: split `execute_direct_shell_script` —
  `${THIS_SH} file` gets a fresh `Executor::new()` built under a scrubbed
  process-env scope (refactor `apply_child_environment` → shared
  `child_process_environment()`), `./x.sh` ENOEXEC mode keeps shared-executor
  subshell semantics. New `child_exit_status` converts ALL ExecuteError
  variants to a status at the boundary (fixes abort-parent P0 alone in
  Stage 1). Stage 3 moves the script driver (`run_script_with_history` et al)
  from main.rs into the lib so in-process children get history recording.
- **RC-3 (array_expand_once)**: no new state — GNU consults a global per
  operation (retroactive). Converge ~35 subscript consumers onto
  `subscript_expansion.rs` as the canonical resolver; indexed no-expand still
  arithmetic-evals (`$(`→operand error, not execution), assoc takes literal
  key. Also fixes the reverse gap: option-OFF must do the second expansion
  RB currently never performs.
- **RC-2 (word-list model)**: Stage-0 adds `Vec<Fragment>` (Literal|Splice)
  parallel output in the embedded walker, composing prefix/suffix onto
  first/last element per subst.c splice rule; Stage-2 promotes
  `ExpandedFragment`/`ExpandedWord` to the canonical result with
  `into_scalar()` adapters over ~300 call sites.
- **RC-4/5 (scanner convergence)**: census found 7 `)`-scanners + 5
  `]`-finders + 5 case-depth dialects. Canonical = `skip_cmd_subst`
  (skip.rs:15) → new `src/lexer/comsub_scan.rs`; subscript canonical =
  `assoc_subscript_end` generalized. 6-stage migration with per-stage suite
  slices; `continuation.rs` delegation diff prepared for captain review.
  Comsub shortcut census: canned strings + posixpipe `4` paths delete first;
  `$(<file)` kept (only GNU-sanctioned shortcut); ~15 single-builtin handlers
  collapse to whitelist or die.
