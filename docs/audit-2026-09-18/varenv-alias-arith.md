# varenv / alias / arith compatibility audit — 2026-09-18

Read-only audit of the GNU Bash `varenv`, `alias`, and `arith` suites against
Rubash, per `docs/audit-baseline-2026-09-18.md`. **No `src/` files were
modified.** Baselines: varenv 85, alias 68, arith 53 stdout diff lines
(stderr excluded from the ledger).

## Method

- Oracle: owner-compiled GNU Bash 5.3.0 at WSL `/usr/local/bin/bash`,
  invoked from a **script file**
  (`MSYS_NO_PATHCONV=1 wsl /usr/local/bin/bash /mnt/d/repo/rubash/<file>.sh`),
  never `bash -c` for quoting-sensitive cases.
- Rubash: `target/debug/rubash.exe <file>.sh` with
  `__RUBASH_NO_UPSTREAM_SCRIPTS=1`.
- Suite artifacts: `target/issue-suites/results/true-baseline/{varenv,alias,arith}/{gnu,rb}.{out,err}`.
- Test sources: `target/issue-suites/results/bash-tests-rw/{varenv,alias,arith}*.{tests,sub}`.
- Minimal reproducers + per-shell captures: `.tmpwork/audit/venv/*.sh`,
  `v2.gnu.out`, `v2.rb.out`, etc. Divergent repros were run at least twice;
  all reproduced identically (deterministic) unless marked otherwise below.
- GNU C source in `third_party/bash/` is the specification; every class
  cites the owning C function and the Rubash owner.

## Headline numbers

| suite  | stdout diff lines | diverging subtests | real root-cause classes |
|--------|-------------------|--------------------|-------------------------|
| varenv | 85                | varenv2, 7, 9–14, 16, 20, 21, 23–25 (varenv15 is a CRLF fixture artifact) | 7 |
| alias  | 68                | alias.tests:56–58, alias4.sub:17–18 cascade | 2 |
| arith  | 53                | arith.tests:178; arith1, arith3, arith6, arith9, arith10 | 6 |

`set -k` (varenv.tests:38,70) and the `alias -p` listing format are **not**
divergent — both matched byte-for-byte in the suite output.

---

## varenv — 85 stdout diff lines, 7 classes

### V1 — `local`/`typeset` without a value does not inherit the temporary environment

- Tests: `varenv2.sub:29-57` (`fff3`/`fff4`/`fff5`), `varenv20.sub:4-8`.
- Repro: `.tmpwork/audit/venv/v2sub.sh` (varenv2.sub verbatim), `v20.sh`.
- GNU:

  ```
  |0|10|      ← x=11 fff3, typeset i=0 x="${x-10}"
  10          ← export x; printenv x  (exported LOCAL value)
  |0|12|      ← x=12 fff4, typeset i=0 x  (local inherits temp 12)
  |y|         ← fff5, z=y typeset z   (local inherits temp y)
  |y|         ← z=42 fff5
  ```
- rb:

  ```
  |0|10|
  1           ← printenv saw the OUTER temp x=1, not the exported local
  |0||        ← typeset x created an EMPTY local instead of inheriting 12
  ||          ← local z empty instead of temp y
  |42|        ← inner temp z=y ignored; local z took outer temp 42
  ```
- Also `v=t f` with `f() { local v; declare -p v; }`: GNU `declare -x v="t"`
  (temp vars arrive exported AND `local v` inherits value+attributes);
  rb `declare -- v` (neither).
- GNU cite: `declare.def:360` `inherit_flag = MKLOC_INHERIT`;
  `variables.c:2579 make_local_variable` (temp-var attribute/value
  propagation); `variables.c:4546 push_temp_var`,
  `variables.c:5215 push_var_context` (temp vars are `att_exported`).
- rb owner: `src/executor/declare_local.rs:365 execute_local` /
  `initialize_non_inherited_locals` (declare_local.rs:522) and
  `src/executor/function_locals.rs:4 save_local_names` — locals are created
  empty; nothing consults the function's temporary environment.
  `src/executor/public_accessors.rs:435 call_function_inner` applies the
  temp env via plain `set_env`, so attribute/export flags are lost.

### V2 — temp assignments to function calls leak into/lose the child environment

- Tests: `varenv2.sub` (`x=1 fff` → `printenv x` inside must see the
  exported local `10`, rb gives the outer temp `1`); `varenv16.sub:24-43`
  (`foo=showfoo show2` inside `showfoo` where `foo` is local).
- GNU: `foo=showfoo environment foo=showfoo`; rb: `foo=showfoo environment foo=foo`
  (the child got the *outer* temp value `foo=foo`, not the inner `showfoo`).
- GNU cite: `execute_cmd.c` temp-env merge for function calls +
  `variables.c:4662 merge_temporary_env`/`push_temp_var` — the temp binding
  is exported for the command and shadows the previous env entry.
- rb owner: `src/executor/public_accessors.rs:435 call_function_inner`
  (temp env → `env_vars` without ordering vs. outer temp) +
  `src/executor/readonly_functions.rs:163 apply_child_environment`
  (child env built from `EXPORTED_VARS`/`local_export_env_values`; the
  newest temp binding does not reliably win).

### V3 — `unset` does not unbind locals at the right dynamic scope; `localvar_unset` and POSIX temp-unset unimplemented

- Tests: `varenv10.sub:19-39`, `varenv20.sub:10-13`, `varenv24.sub:24-98`.
- Repros: `.tmpwork/audit/venv/ve10.sh`, `v20.sh` (`local v=x; unset v;
  declare -p v` → GNU `declare -- v` / `declare -x v`, rb `declare: v: not
  found`).
- GNU marks an unset local **invisible but keeps the binding** so the
  local attribute survives a later re-assignment, and removes the
  *visible* binding at whatever scope owns it:
  `variables.c:3788 unbind_variable`, specifically the
  `att_invisible`/`hash_remove` walk at `variables.c:3960-4000`
  (`old_var->context == variable_context || (localvar_unset &&
  old_var->context < variable_context)`); `variables.c:3752
  posix_unbind_tempvar` for `x=temp unset x` under `set -o posix`
  (GNU: `x = unset`; rb: `x = global`).
- `shopt -s localvar_unset` (`variables.c:145`) changes whether an inner
  `unset x` unbinds an *outer* local; rb ignores the option entirely
  (only listed in `src/builtins/complete.rs:475` /
  `builtins/shopt/support.rs:48`).
- rb owner: `src/executor/unset_arrays.rs:4 execute_unset`,
  `:193 unset_outer_local_variable` (only handles *outer* scopes;
  current-scope locals are removed wholesale from `env_vars`), and
  `src/executor/function_locals.rs` scope save/restore — rb has no
  "invisible local" state, so unset locals vanish instead of staying
  bound-but-invisible.

### V4 — readonly locals: shadowing and previous-scope leakage

- Tests: `varenv7.sub:57` (`readonly var=outside; func { local
  var=inside }` → GNU `local: var: readonly variable` + `inside: outside`;
  rb silently `inside: inside`), `varenv9.sub:16-61` (`readonly`/`declare`
  inside a function must not leak `i1/j1/…` to the caller scope — rb
  prints `declare -- i1`/`declare -- j1`/… after return),
  `varenv25.sub` (`declare -r string`/`declare -ir int`/`declare -ar
  array` printed at previous scope; `int="100+42"` stored literally
  instead of evaluating to `142`).
- GNU cite: `variables.c:2579 make_local_variable` (readonly check on
  existing local); `variables.c:5409 push_scope`/`5449 pop_scope`
  (function-scope variable lifetime); `declare.def:443-449`.
- rb owner: `src/executor/declare_local.rs:365 execute_local` /
  `write_local_compound_readonly_assignment_errors` (:494) — readonly
  check misses "existing global readonly" when declaring a same-named
  local; `src/executor/function_locals.rs:152 restore_function_locals`
  fails to discard names first declared inside the function.

### V5 — `local -` (save/restore shell options) unimplemented

- Tests: `varenv21.sub:16-48`.
- GNU: `local -` inside a function snapshots `$-`/`SHELLOPTS`/option
  variables and restores them at return — output `ignoreeof on / off /
  on`, `IGNOREEOF` → `10`, `local -p` prints `local -`, and `match 1`.
- rb: `ignoreeof off` ×3, `0`, no `local -` line, `bad 1` — options not
  saved/restored and `local -` not displayed.
- GNU cite: `declare.def:115` ("If any NAME is `-`, local saves the set
  of shell options and restores"), `declare.def:443-449`
  (`STREQ(name,"-")` → `make_local_variable("-", 0)` → scope save).
- rb owner: `src/executor/declare_local.rs:365 execute_local` — no `-`
  option handling exists; option state lives outside the local-scope
  save/restore in `src/executor/function_locals.rs`/`shell_options.rs`.

### V6 — `${var+word}` (and `-`/`#`) alternates containing `:` misroute to substring arithmetic

- Tests: `varenv12.sub:33,37` (`echo ${var+"BUG: still set 1"}` → GNU
  blank, rb `arithmetic syntax error in expression`, and the echo line is
  lost entirely — no output at all).
- Repro: `.tmpwork/audit/venv/alt.sh`, `alt2.sh`, `alt3.sh`.

  | input (var=set)      | GNU      | rb                     |
  |----------------------|----------|------------------------|
  | `"${var+"a: b c"}"`  | `a: b c` | arithmetic error       |
  | `${var+a:b}`         | `a:b`    | empty                  |
  | `${var-a:b}`         | `set`    | empty                  |
  | `${var#a:b}`         | `set`    | empty                  |
  | `${var:+a:b}`        | `a:b`    | `a:b` ✓                |

- Root cause: `src/executor/parameter_core.rs:598-673
  split_top_level_colon` tracks `${}`/paren/`?`/escape depth but **does
  not track quotes**, and `parse_parameter_substring`
  (parameter_core.rs:324) runs before the bare `+`/`-`/`#` operator
  handlers in `parameter_words.rs` (`:103` handles `:+` first, which is
  why `:+` survives; bare `+` is at `:210`, after the substring check).
  A `:` anywhere in an operator word is taken as a substring separator.
- GNU cite: `subst.c:9777 parameter_brace_expand` — the operator is the
  first operator char after the name; a `:` inside the word is data.
  `subst.c:2198 skip_to_delim` (the model the rb comment cites) is only
  reached once substring form is already established.

### V7 — environment import/export edge cases and array print order

- `varenv13.sub:30`: `env 'v[0]=help' ${THIS_SH} -c 'printenv "v[0]"'` —
  GNU prints `help` (non-identifier env names are bound via
  `variables.c:523 bind_invalid_envvar` into `invalid_env`,
  `variables.c:3307`, and re-exported by `maybe_make_export_env`,
  `variables.c:5081-5103`). rb drops it on the export side —
  `src/executor/env_helpers.rs:23 is_initial_export_candidate` /
  `readonly_functions.rs:163 apply_child_environment` never mark it.
  (Verified: `env 'v[0]=help' rubash.exe -c 'printenv "v[0]"'` → rc=1;
  `declare -p "v[0]"` inside rb shows the var was imported, just not
  re-exported.)
- `varenv11.sub`: `declare -A foo` prints keys in a different order
  (`[zero]`/`[one]` vs `[one]`/`[zero]`) — assoc iteration order.
  Cosmetic; values agree. Owner: assoc storage/`declare -p` printer.
- `varenv14.sub`: `declare -a s` shows `declare -- s="X(Y)"` instead of
  `declare -a s=([0]="X" [1]="Y")` — array attribute lost when the array
  is built via certain assignment paths; assoc `v`/`assoc` lose entries
  (`[0]="7"`, `[two]`/`[three]`/`[one]`/`[list]` dropped). Owner:
  `src/executor/variable_state.rs` + array-assignment paths.
- `varenv22.sub`: only delta is `trap -- '' SIGRTMIN` missing from
  `${THIS_SH}` child output — platform artifact (rb seeds startup traps
  in `init.rs:33-36`; SIGRTMIN has no Windows disposition). Not a
  varenv semantic bug.
- `varenv15.sub`: the diff hunk is GNU's trailing `\r` from the
  CRLF-encoded `varenv15.in` fixture (`xxd` shows `7a 0d 0a` vs rb `7a
  0a`). Positional parameters across `source` are correct in both.
  Classified as **fixture artifact** (rb treats `\r` as whitespace where
  GNU treats it as a word character — a real lexer difference, but
  triggered only by the CRLF fixture).
- `varenv23.sub`: `a=3 readonly a` inside `f1` under `set -o posix` must
  persist `a=3` at global scope (GNU `global: 3`); rb leaves `bcde` —
  temp-env merge for special builtins inside function context
  (`variables.c:4662 merge_temporary_env`,
  `:3752 posix_unbind_tempvar`; rb owner:
  `src/executor/temporary_assignments.rs:26 apply_temporary_assignments` /
  `function_calls.rs`).

---

## alias — 68 stdout diff lines, 2 classes

The whole suite diff is 4 hunks. Two root causes; one produces ~64 of the
68 lines by corrupting everything after it.

### A1 — multiline/compound alias bodies are not reparsed as command text

- Tests: `alias.tests:56-58`:
  `alias foo='a=() b=""\nfor i in 1; do echo hi; done'`; `foo` must run
  the `for` loop.
- GNU: `hi`. rb:
  `$'a=() b=""\nfor i in 1; do echo hi; done': command not found` — the
  alias value becomes a single command word.
- Note the nearby `alias L='m=("x")'` (tests:61) works — single-line
  compound bodies reparse fine; multiline bodies do not.
- GNU cite: `parse.y:3249 alias_expand_token` → `push_string(expanded,
  AL_EXPANDNEXT, ap)` at `parse.y:3274` pushes the replacement text onto
  the lexer input stack (`parse.y:2025-2130`), so newlines/`for` parse
  exactly like typed input.
- rb owner: `src/executor/arithmetic_aliases.rs:564
  alias_parser_source` / `:590 alias_parser_source_inner` (joins the
  alias value with `rest` words and reparses via `crate::parser::parse`,
  but the admission predicate `needs_parser_level_alias_expansion`,
  `src/executor/parse_helpers.rs:145`, and the `alias_reparse.rs:5
  execute_alias_introduced_compound_source` path produce a single-word
  command here — reparse falls back to command-not-found).

### A2 — an alias body with an open quote corrupts the input stream (cascade)

- Tests: `alias4.sub:17-18`:
  `alias foo="echo 'Error:"` then `foo bar'` — GNU's lexer pushes
  `echo 'Error:` into the input and reads `bar'` from the real source to
  close the quote → `Error: bar`, then continues normally.
- rb prints `Error: bar` then emits the *remaining source text of
  alias4.sub verbatim* (rb.out lines 19-63 are raw test source:
  `v=1`, `alias a=unalias -a`, `unalias -a`, `alias echo=echo`, …) and
  cascades into `alias5`/`alias6` before resyncing at `<áa>`. GNU's
  expected `ok 1`/`ok 2`/`text`/`whoops:  nullalias`/`foo`/`a`/`a b`/
  `a b`/`a a b`/`ok 3`/`ok 4`/`line with escaped newline`/`bar`/`bad`/
  `<|cat>` are all lost inside the cascade.
- This is the same root cause as A1, one level deeper: GNU's
  `alias_expand_token` splices alias text into the *token* stream, so an
  unclosed quote continues consuming real input; rb expands aliases at
  the executor level after a whole line is already tokenized, and the
  `has_unclosed_quote` shortcut (`parse_helpers.rs:150-176`) joins the
  current command's remaining words — which cannot represent
  "quote continues into the next source line".
- Contributing gate: `src/main.rs:1659-1669` — `has_unclosed_input_syntax`
  emits a generic `unexpected end of file` instead of re-reading; the
  `run_source_with_line_offset` TODO at `main.rs:1643-1652` already names
  the missing "complete command stream" model.
- GNU cite: `parse.y:5759-5767` (alias expansion during
  `read_token_word`), `parse.y:3249-3280`, `parse.y:1795
  with_input_from_string`, `parse.y:7039-7041` (`parse_and_execute`
  resets `echo_input_at_read`/`expand_aliases` per input chunk).
- alias7.sub's `al for …` `-c` blocks match in the suite (a standalone
  diff seen earlier was a `${THIS_SH}=./bash` artifact — both shells
  produce `foo in v`/`foo=v bar=` when `${THIS_SH}` resolves).
- Stderr noise caused by the same cascade: rb emits `a: command not
  found`, `ever: command not found`, and one `invalid alias name` vs
  GNU's two (`alias.tests:66-67`) — ordering fallout, not a third class.

---

## arith — 53 stdout diff lines, 6 classes

All repros run twice; all rb/GNU outputs below are deterministic across
runs (RANDOM caveat in ARITH-F).

### ARITH-A — `$name` inside `$((…))` is spliced as text, not looked up as 0

- Test: `arith.tests:178` `echo $(( jv += \$iv ))` (iv unset).
- GNU: `expand_arith_string` (`subst.c:3981`) expands `\$iv` → `$iv` →
  empty text → expression becomes `jv += ` → `evalexp` →
  `expr.c:1120`/`1507` "operand expected", command produces no output.
- rb: evaluates `$iv` as a variable read → 0 → `jv += 0` → prints `45`
  (suite value) and emits no diagnostic.
- Refinement (`dol.sh`): GNU prints `1` for `$(( $iv + 1 ))` (empty +
  `+ 1` → unary plus) and `0` for `$(( $iv ))` — so the defect only
  surfaces where the empty splice creates a parse error; rb's "unset →
  0" shortcut coincides elsewhere.
- rb owner: `src/executor/arithmetic/factor.rs:160-192
  parse_dollar_variable` → `value.rs:41 variable_value`
  (`unwrap_or_default` → `evaluate_variable_text` → `Some(0)`). rb parses
  `$name` as an lvalue token; GNU splices the expansion before parsing.

### ARITH-B — `name[sub] ++` with whitespace before `++` fails on subscripted lvalues

- Test: `arith1.sub:23` `(( array[0] ++ ))`.
- GNU: increments to `2` (`expr.c:123-124` POSTINC/POSTDEC, applied at
  `expr.c:1087-1095`; `readtok` skips whitespace between tokens).
- rb: `operand expected`, array stays `1`.
- Root cause: `src/executor/arithmetic/factor.rs:178-198 parse_variable`
  calls `self.consume("++")` without `skip_ws()` after the lvalue.
  Scalar `a ++` only works by accident — `parse_lvalue`
  (`lvalue.rs:86-96`) `skip_ws()`s *before* checking `[`, leaving `pos`
  past the space for scalars; for `array[0] ++` the `]` is consumed and
  `pos` lands on the space, so `consume("++")` misses.
- Also note `cursor.rs:12 consume` never skips whitespace — the same
  shape could recur for any post-lvalue operator.

### ARITH-C — `x op= v` on an int var holding an invalid expression

- Test: `arith9.sub:35-38` — `x=4+; declare -i x; x+=7 y=4`.
- GNU: `evalexp` evaluates `x`'s stored value `4+` first →
  `4+: operand expected` (`expr.c:538` assignment-error path) → the
  whole assignment command aborts → `x = 4+ y =` (y never assigned).
- rb: `x = 11 y = 4` — `x+=7` produced `11` (rb evaluates the spliced
  `4+ + 7` as `4 + (+7)`, so no error) and `y=4` still ran.
- Secondary: `declare -i w=4+` — GNU errors at declare time; rb silently
  drops the value (`ar9.sh` run: GNU `declare: 4+: operand expected`,
  rb silent, `w` ends unset).
- rb owner: `src/executor/temporary_assignments.rs` int-append path +
  `src/executor/arithmetic/mod.rs` (no "evaluate stored value alone
  first" step; GNU `expr_streval`/`expr.c:380-400` lvalue read).

### ARITH-D — self-recursive variable strings: rb's name-stack blocks GNU's depth-limited recursion

- Tests: `arith6.sub:33-35`:
  `n=0 a="(a[n]=++n)<7&&a[0]"; ((a[0]))` — GNU recursively re-evaluates
  `a[0]`/`a` inside `a`'s own value (loop terminates when `n` hits 7)
  → `${a[@]:1}` = `1 2 3 4 5 6 7`; rb evaluates the string once and
  returns 0 on re-entry → `1`.
  `a="(a[n]=n++)<7&&a"; ((a))` → GNU `0 1 2 3 4 5 6 7`, rb `0`.
- GNU cite: `expr.c:103 MAX_EXPR_RECURSION_LEVEL` (1024) +
  `expr.c:268-281 pushexp` — recursion is **depth-bounded**, not
  name-banned; a self-referencing variable legitimately re-evaluates
  until depth 1024 (`expression recursion level exceeded`, the
  arith9.sub:14 diagnostic).
- rb owner: `src/executor/arithmetic/value.rs:43,100` —
  `resolving: Vec<String>` returns `None` when a name re-enters, so
  legitimate (terminating) self-recursion is silently truncated to 0.

### ARITH-E — empty/quoted subscripts in `((…))`, `$((…))`, `let`

- Tests: `arith10.sub:44-45,36,41` (and the `assoc_expand_once`
  second pass).
- Matrix (`a` indexed, GNU vs rb):

  | construct (subshell)                  | GNU           | rb            |
  |---------------------------------------|---------------|---------------|
  | `(( a[""]=24 ))`                       | `a[]': not a valid identifier`, `[0]="0"` | `[0]="24"` |
  | `: $(( a[""]=25 ))`                    | same, `[0]="0"` | `[0]="25"`  |
  | `let 'a[""]=26'`                       | `[0]="26"`    | `[0]="26"` ✓  |
  | `a[""]=23` (plain assignment)          | `[0]="23"`    | `[0]="23"` ✓  |
  | `a[" "]=15` (word, quoted space)       | operand expected, subshell aborts | same ✓ |
  | `let "a[\" \"]"=18` (assoc_expand_once set) | operand expected, no output | `[0]="18"` |
  | `let "a[\"\"]"=22` (assoc_expand_once set)  | operand expected, no output | `[0]="22"` |

- GNU cite: `expr.c:359-377 expr_skipsubscript` (`noexp =
  already_expanded && (compat>51 || array_expand_once)` →
  `VA_NOEXPAND`), `error.c:461` "`%s': not a valid identifier"; the
  `((`-command path is `execute_cmd.c:3893 execute_arith_command` →
  `expand_arith_string` at `execute_cmd.c:3935` which dequotes `""` to
  an empty subscript.
- rb owner: `src/executor/arithmetic/lvalue.rs:44-79
  collect_raw_subscript`/`parse_lvalue` — an empty post-dequote
  subscript is treated as index 0 instead of an identifier error; the
  `assoc_expand_once`-sensitive `let` subscript path ignores the shopt.

### ARITH-F — RANDOM flows: deterministic-but-divergent, plus a real double-draw bug

- Tests: `arith3.sub:15-59`.
- Two separate facts (each verified twice, outputs identical across
  runs):
  1. **Sequence mismatch (expected/environmental):** rb's `RANDOM`
     sequence is deterministic and reseeds correctly
     (`RANDOM=42` → `19081 17033 15269` twice), but it is not GNU's
     LCRNG (`17772 26794 1435`). All diffs that follow purely from
     "different index values" are **not actionable** unless bit-exact
     RANDOM parity is a project goal. The baseline's 53↔57 suite-count
     wobble comes from this class.
  2. **Real defect:** `(( dice[$RANDOM]++ ))` consumes **two** draws
     (rb index `17033` = 2nd draw) and `(( dice[$RANDOM] += 7 ))`
     consumes **three** (`25461`), while `$((RANDOM))` consumes one —
     GNU consumes exactly one in every form (`rnd2.sh`). rb expands the
     `((…))` word text more than once (extra `$RANDOM` expansions are
     drawn and discarded before evaluation). This also makes
     `RANDOM=42`-reseeded `dice1`/`dice2` loops diverge *within* rb →
     `random sequences differ` is printed (GNU prints nothing).
- GNU cite: `execute_cmd.c:3893 execute_arith_command` —
  `expand_arith_string` (`subst.c:3981`) is called **once** on the
  `((…))` text (execute_cmd.c:3935); `expr.c:359 expr_skipsubscript`
  walks the subscript without re-evaluating `$RANDOM`.
- rb owner: the `((…))` expansion path — `src/executor/arithmetic/mod.rs`
  entry + `arithmetic/factor.rs`/`lvalue.rs` IndexedRaw deferred
  re-evaluation interacting with `$RANDOM` at `value.rs:46-52`.
- Conclusion for the ledger: arith3 diffs are *deterministic* but the
  value-level diffs are dominated by (1); only the draw-count defect
  (2) is a semantic bug.

### Non-diffs verified (for the record)

- `((echo abc; echo def;); echo ghi)` — both print `abc def ghi`.
- `let 'jv += $iv'` — both error identically (`operand expected`).
- `arith2/4/5/7/8.sub` — byte-identical stdout standalone.
- `alias5.sub` — identical; earlier suspicion was the A2 cascade.
- Arithmetic stderr wording was already aligned by `8c7bbf92`; remaining
  stderr deltas are ordering/prefix noise, not new classes.

---

## Cross-cutting notes

- **rubash#117 flag (fast paths):** two of these defects live in
  text-level shortcuts rather than the parser — `split_top_level_colon`
  (parameter_core.rs:598) without quote tracking (V6), and
  `has_unclosed_quote`/`needs_parser_level_alias_expansion`
  (parse_helpers.rs:145-176) gating alias reparse (A1/A2). Both are the
  blacklist-predicate shape the project has banned: the fix direction is
  the real lexer/parameter machinery (GNU `push_string` input model;
  `parameter_brace_expand` operator dispatch), not another `contains()`
  clause.
- **Determinism ledger:** every rb/GNU output quoted above was captured
  from at least two identical runs. The only nondeterminism observed was
  the baseline's own 53↔57 arith count, traced to arith3's RANDOM
  histogram indices (ARITH-F.1).
- **Environment artifacts (not rb semantics):** varenv15 CRLF fixture
  (V7), varenv22 `SIGRTMIN` trap line (V7), varenv11 assoc key ordering
  (cosmetic), the `alias7` standalone `./bash` artifact.

## Suggested fix order (for later work — no edits made here)

1. Function temporary-environment model (V1+V2+varenv20/23/24): single
   owner change in `call_function_inner`/`execute_local` to carry
   value+export+readonly through `local` and into child env.
2. Unset/local-invisible model (V3): `att_invisible` equivalent in
   `local_var_scopes` + `localvar_unset`.
3. Alias input-stream model (A1/A2): push alias text into the lexer
   stream like GNU `push_string`, retiring the executor-level join.
4. `${var±word}` operator dispatch order + quote-aware colon split (V6).
5. Arithmetic: `skip_ws` before postfix ops (ARITH-B), `$name` splice
   semantics (ARITH-A), stored-value eval for `op=` (ARITH-C),
   depth-based recursion (ARITH-D), empty-subscript identifier check
   (ARITH-E), single-expansion `((…))` (ARITH-F.2).
