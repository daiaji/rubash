# nameref compatibility audit — 2026-09-18

Read-only audit of the GNU Bash `nameref` suite against Rubash at code
baseline `2494bc57` (code-identical to `c282a850`), per
`docs/audit-baseline-2026-09-18.md`. **No `src/` files were modified.**

## Method

- Oracle: owner-compiled GNU Bash 5.3.0 at WSL `/usr/local/bin/bash`,
  invoked from a **script file**
  (`MSYS_NO_PATHCONV=1 wsl /usr/local/bin/bash /mnt/d/repo/rubash/<file>.sh`),
  never `bash -c` (Windows/WSL arg passthrough corrupts quoting).
- Rubash: `target/debug/rubash.exe <file>.sh`.
- Suite artifacts: `target/issue-suites/results/true-baseline/nameref/`
  (`gnu.out` 417 lines / `rb.out` 170 lines / `gnu.err` 171 lines /
  `rb.err` 56 lines; `gnu.rc`=0, `rb.rc`=1).
- Test sources: `target/issue-suites/results/bash-tests-rw/nameref.tests`
  + `nameref1.sub`..`nameref25.sub` (the driver runs
  `${THIS_SH} ./namerefNN.sub` per sub at `nameref.tests:130-132`).
- Standalone per-subtest runs: every `namerefNN.sub` was re-run under both
  shells with `PATH=<tests-dir>:/usr/bin:/bin` (so `recho`/`zecho`
  resolve) and `THIS_SH=/usr/local/bin/bash` for the GNU side.
- Minimal reproducers: `.tmpwork/audit/nameref/*.sh`, run through both
  shells with stdout/stderr/rc compared byte-wise.
- GNU C source in `third_party/bash/` (bash.git @b4608166, 5.3 patch 15)
  is the specification; each class cites the owning C function.

## Headline finding

The nameref suite's 281-line stdout diff is dominated by **one
architectural divergence** (class C1): Rubash executes
`${THIS_SH} ./namerefNN.sub` **in the same process**, so

1. an `ExpansionFailure` inside a child sub propagates into the parent's
   `for` loop and **aborts the entire suite** — Rubash output stops at
   `nameref11.sub:76` (`${!foo[2]}` → `bar: invalid indirect expansion`),
   so all `nameref12`–`nameref25` output and diagnostics are missing; and
2. child scripts see parent state through the shared process environment
   (`std::env`), because `readonly`/`export` builtins call
   `env::set_var` and value lookups fall back to `env::var` — non-exported
   readonly variables leak into the "fresh" child shell.

Standalone per-subtest reruns confirm that nameref12–25 contain many
*real* residual divergences independent of the truncation; they are
itemized below.

## Divergence class table

| # | Class | Repro (`.tmpwork/audit/nameref/`) | GNU cite | Rubash owner | Verdict | Severity |
|---|-------|------------------|----------|--------------|---------|----------|
| C1 | In-process `${THIS_SH}` child: error propagation + process-env leakage | `trunc_repro.sh` + `fail_ind.sh`; `leak_parent.sh` + `nameref2_clone.sub` | `execute_cmd.c:624 execute_command_internal` (child shell is a separate process); `subst.c` `expand_param_error` aborts only that command | `src/executor/external_finish.rs:45-118` same-shell dispatch; `121-258` `execute_direct_shell_script` (partial state swap, `Err` passthrough at 251-256); `src/builtins/setattr/apply.rs:66,166` `env::set_var`; `src/executor/mod.rs:344` `ExecuteError::ExpansionFailure`; `src/executor/ast_exec.rs:769-830` | root-caused | **Critical** |
| C2 | `${!name-word}` / `${!name+..}` on unset name applies the operator instead of `invalid indirect expansion` | `ind2.sh`, `ind3.sh`; suite `nameref3.sub:29` | `subst.c:7883-7935 parameter_brace_expand_indir` (`valid_identifier(name) && v==0` → `invalid indirect expansion`, `expand_param_error` at 7913-7917) | `src/executor/expand_braced_special.rs:52-54` bails to operator family; `src/executor/parameter_errors.rs:261-291 indirect_parameter_operator_value` — `env_vars.get(indirect_name)?` (281) treats unset as "use default" | root-caused | High |
| C3 | Empty-cell nameref treated as set-but-null; `-`/`+` operators see wrong set-state | `empty_cell.sh`; suite `nameref4.sub:143` | `variables.c:2011-2027 find_variable_nameref` (empty cell → NULL); `subst.c:10131` `var_is_set = temp != NULL` on resolved value | `src/executor/variable_state.rs:59-64` `nameref_resolution` returns `NotNameref` for empty cell → `ref` read as literal empty scalar; `parameter_errors.rs` operator set-tests | root-caused | High |
| C4 | `unset -n name` on a non-nameref removes the variable | `unset_n_scalar.sh` | `builtins/set.def:1023` `nameref ? unbind_nameref(name)`; `variables.c:3807-3816 unbind_nameref` no-ops unless `nameref_p(v)` | `src/builtins/set/unset.rs:180-195` `nameref_cell` computed only when `!options.nameref`; `-n` falls through to unconditional `env_vars.remove` (~215) | root-caused | Medium |
| C5 | `${!name[sub]}` indirect-through-element semantics | `ind_elem_err.sh`; suite `nameref1.sub` (`${!foo[0]}` → rb `one`, GNU ``), `nameref11.sub:75-76` | `subst.c:7883-7935` (nameref-cell shortcut at 7895-7903 only for the unsubscripted name; array-ref error path 7924-7935) | `src/executor/expand_braced_special.rs:73-208` indirection resolves base nameref then applies `[sub]` to the target value; misses GNU's "unset target → invalid indirect expansion" | root-caused | Medium |
| C6 | `${f/x/X}` pattern-subst on a nameref whose cell is `arr[i]` yields empty | `patsub_elem.sh`; suite `nameref9.sub` (missing `idX2`) | `subst.c` `get_var_and_type`/`param_expand` resolve the nameref to the *element value* before patsub | `src/executor/parameter_patterns.rs:203-204` `parameter_pattern_scalar_value` uses `resolved_variable_name` result (`arr[1]`) as a raw `env_vars` key → `None` → `""` | root-caused | Medium |
| C7 | Command substitution inside an array subscript evaluated extra times | `comsub_count.sh`; suite `nameref10.sub` (GNU 4 × `comsub`, rb 6) | `subst.c`/`arrayfunc.c` evaluate the subscript expression once per expansion | `src/executor/arrays/executor.rs:100+` `array_element_parameter_value` / subscript re-expansion through the nameref path | root-caused | Medium |
| C8 | `coproc NAME` clobbers NAME's attributes/value instead of honoring readonly/nameref | `cop.sh`; suite `nameref11.sub:45-54` | `execute_cmd.c:~2408-2430` — `readonly_p(v)` → `err_readonly` early return; else `convert_var_to_array` + `bind_array_variable` fds | `src/executor/compound_exec.rs:813-948` (esp. ~905-933): stores `"(rfd wfd)"` text + marks `ARRAY_VARS` on NAME unconditionally → `declare -ar ROVAR=(63 60)`, `declare -an ref=(...)`, `declare -ar RO=(...)` | root-caused | High |
| C9 | `${@:0}` / `${*:0}` omit `$0` | `at0.sh`; suite `nameref11.sub:47` (GNU `./nameref11.sub` ×2, rb blank) | `subst.c:3745-3779 pos_params` — offset 0 includes `$0` | `src/executor/parameter_ops.rs:461-484 positional_parameter_substring` `(offset as usize).saturating_sub(1)` never includes `$0` | root-caused | Medium |
| C10 | `mapfile`/`declare` target validation for nameref-to-element | `mapfile_ref.sh`; suite `nameref11.sub:32`, `nameref18.sub` | `mapfile.def` binds via resolved name + `arrayfunc.c` element; `declare.def:553-557` rejects array-ref nameref LHS | `src/executor/mapfile_builtin.rs:425-513` + `mapfile_helpers.rs:88-104` — keeps `-n` (`declare -an r=()` vs GNU `declare -a r=()` + `warning: removing nameref attribute`); accepts literal `XXX[0]` as a *name* (`declare -a XXX[0]=(...)`) | root-caused | High |
| C11 | Circular vs maximum-depth diagnostics conflated; depth bound 16 vs 8 | `maxdepth.sh`; suite `nameref8.sub`, `nameref15.sub` | `variables.h:181 NAMEREF_MAX=8`; `variables.c:2191,2220,3797` `maximum nameref depth (8) exceeded`; circular at `variables.c:2034-2038` | `src/executor/variable_state.rs:52` `for _ in 0..16` then `NamerefResolution::Circular` — wrong bound, and depth overflow is reported as `circular name reference` | root-caused | Medium |
| C12 | `declare -n`/assignment attribute matrix on arrays and through namerefs | `declare_n_array.sh`; suite `nameref15.sub` (`-an a=...`), `nameref20.sub` (`declare ref=(X)`), `nameref22.sub` (`declare -n array=(...)`) | `declare.def:553-557` `reference variable cannot be an array`; `declare.def:574` `valid_nameref_value`; `arrayfunc.c:471` nameref-cell validation; GNU emits `warning: a: removing nameref attribute` (`nameref15:91,96`) | `src/executor/temporary_assignments.rs:212-345`; `src/builtins/setattr/apply.rs` — rb accepts `-n` onto existing arrays (`-an`), keeps `-n` when GNU removes it, collapses `declare ref=(X)` differently | needs-deeper-work | High |
| C13 | `readonly`/`typeset -r` through a nameref applies to the wrong subject | `ro_through_ref.sh`; suite `nameref12.sub:87`, `nameref17.sub` (`-nr`→`-r` after `typeset +n`) | `setattr.def`/nameref transform: `readonly ref` (ref→`var[0]`) → `readonly: 'var[0]': not a valid identifier`; `declare.def:678` readonly+nameref `+n` restriction | `src/builtins/setattr/apply.rs:114-167` `nameref_target_name` → applies readonly to `ref` itself (`declare -nr ref="var[0]"`); rb also allows `+n` removal GNU refuses | root-caused | Medium |
| C14 | `set -u` unbound checks missing/wrong for nameref-to-array forms | `setu_ref.sh`; suite `nameref25.sub` | `subst.c:10165` `unbound_vars_is_error` on resolved nameref/array-element expansion | unbound check doesn't fire through `${r}` where `r`→`a[@]`/`a[k]`; rb prints `ok 5`-`ok 8` where GNU aborts | needs-deeper-work | Medium |
| C15 | Diagnostic wording / missing diagnostics for invalid nameref values through builtin channels | suite `nameref11.sub:14-40`, `nameref13.sub:101`, `nameref18.sub`, `nameref24.sub:24` | `declare.def:574-578` `invalid variable name for name reference`; per-builtin `not a valid identifier` forms (`printf -v`, `exec {r}>`, `((r=0))`, `getopts`, `select`) | wording `not a valid identifier` vs `invalid variable name for name reference`; missing `declare: '': not a valid identifier`; rb `(63 60): invalid variable name` wrong subject | root-caused | Low |
| C16 | `declare -n name` re-declaration / local-scope reset | suite `nameref12.sub:63-79`, `nameref14.sub` (`var` vs `foo`), `nameref12` extra `declare -n x` | `declare.def:645-704` — bare `declare -n name` on an existing nameref resets/keeps per ksh93 rules without following the chain | `src/executor/temporary_assignments.rs` + local-scope handling — rb prints stale `-nr ref="var[0]"` inside the function where GNU prints `declare -n ref` | needs-deeper-work | Medium |

## Class details

### C1 — In-process `${THIS_SH}` execution (dominant; Critical)

`nameref.tests:130-132` loops `${THIS_SH} "$testfile"` over all subs.
GNU forks a real child shell: fresh variable state, and an expansion
error inside the child aborts only that child's command and terminates
the *child* with nonzero status — the parent's loop continues.

Rubash's `execute_same_shell_script`
(`src/executor/external_finish.rs:45-118`) deliberately runs same-shell
scripts in-process (`execute_direct_shell_script`, lines 121-258). The
function does swap `env_vars`/`shell_state` for a `THIS_SH` invocation
(158-187), but two leaks remain:

- **Process environment.** `readonly`/`export` write through to the real
  process environment (`src/builtins/setattr/apply.rs:66` and `:166`
  `env::set_var`), and lookup paths fall back to `env::var`
  (`apply.rs:55,144,152`; the expansion fallback chain under
  `shell_variable_value`, `variable_state.rs:149+`). Proof:
  `leak_parent.sh` + `nameref2_clone.sub` —
  `readonly foo=one; ${THIS_SH} ./nameref2_clone.sub` where the child
  does `typeset -n ref=foo; readonly ref; foo=4; echo "ref=<$ref>"`:
  - GNU: `ref=<>` + `foo: readonly variable` (child never saw `foo`).
  - RB: `ref=<one>` (child saw the parent's *non-exported* readonly
    `foo=one`; a plain non-exported `bar=two` does **not** leak —
    `probe_parent2.sh` — so the vector is specifically the
    `env::set_var`/`env::var` process-env side channel).
  - Same mechanism produces the stray `one` in the suite at
    `nameref2.sub` (`echo $ref` → GNU empty, rb `one`; diff hunk at
    `gnu.out:36-38`).

- **ExpansionFailure propagation.** The child executes with
  `self.execute_ast(&ast)` on the *same* executor; `Err` results pass
  through `match result` at `external_finish.rs:251-257`, which only
  converts `ExecuteError::ExitCode` — `ExecuteError::ExpansionFailure`
  (`src/executor/mod.rs:344`) propagates into the parent's AST walk and
  aborts the enclosing `for` loop (`ast_exec.rs:769-830` skip semantics
  apply at the wrong boundary). Proof: `trunc_repro.sh` + `fail_ind.sh`
  (`echo "${!foo}"` with `foo` unset):
  - GNU: error line, child continues (`child-after`), `childrc=0`, loop
    iterates, `loop-done`.
  - RB: error line, then the parent's loop is dead — no `child-after`,
    no `childrc=`, only `loop-done` (the `Err` unwound the loop; the
    trailing `echo` still ran because it was outside the loop body).
  - In-suite: `rb.err` ends at `nameref11.sub: line 76: bar: invalid
    indirect expansion` and `rb.rc=1`; everything from `nameref11`'s
    tail through `nameref25` (~120 stdout lines, ~115 stderr lines) is
    missing purely from this abort.

This class alone accounts for the mega-hunk `gnu.out:148-417` vs
`rb.out:149-170` and is the single highest-leverage divergence.

### C2 — `${!name-op}` on an unset name (High)

`nameref3.sub:29` `recho "${!foo-unset}"` (after `unset -n foo`):

- GNU: `nameref3.sub: line 29: foo: invalid indirect expansion` —
  `parameter_brace_expand_indir` (`subst.c:7883`) checks
  `valid_identifier(name) && v == 0` at `subst.c:7910-7917` →
  `expand_param_error`; the `-` rhs is never evaluated and `recho`
  never runs (no argv line).
- RB: prints `argv[1] = <unset>` — `expand_braced_special.rs:52-54`
  hands `!foo-unset` to the operator family via
  `has_indirect_parameter_word_operator`
  (`parameter_ops.rs:364-375`), and
  `indirect_parameter_operator_value`
  (`parameter_errors.rs:261-291`) hits `self.env_vars.get(indirect_name)?`
  at line 281 → `None` → treated as "unset → apply default" instead of
  raising the indirection error.

Also reproduces as `printf 'A<%s>\n' "${!foo-unset}"` (`ind2.sh`):
GNU `foo: invalid indirect expansion`; RB `A<unset>`.

### C3 — Empty-cell nameref set-state (High)

`typeset -n ref` (never assigned) stores `ref=""` with the `NAMEREF_VARS`
mark. `nameref_resolution` (`variable_state.rs:59-64`) returns
`NotNameref` when the cell is empty/not-a-name, so `ref` is then read as
a plain variable with value `""` — **set but null**.

GNU treats an empty-cell nameref as *unset*: `find_variable_nameref`
(`variables.c:2023-2026`) returns NULL when `nameref_cell` is empty, and
`param_expand` computes `var_is_set = temp != NULL` (`subst.c:10131`) on
the resolved value.

`empty_cell.sh`: `typeset -n ref; echo "A<${ref-unset}>"; echo "B<${ref+set}>"`
- GNU: `A<unset>` / `B<>` — both operators see an unset parameter.
- RB: `A<>` / `B<set>` — **inverted** on both operators.

Suite hit: `nameref4.sub:143` `echo ${ref-unset}` → GNU `unset`, rb ``
(diff hunk `gnu.out:64`).

### C4 — `unset -n` on a non-nameref (Medium)

`y=1; unset -n y; declare -p y`:
- GNU: `declare -- y="1"` — `set.def:1023` routes `-n` to
  `unbind_nameref` (`variables.c:3807-3816`), which returns 0 unless
  `nameref_p(v)` — a no-op on scalars.
- RB: `declare: y: not found` — `unset_name`
  (`src/builtins/set/unset.rs:120-225`) only computes `nameref_cell`
  when `!options.nameref` (line 181), so `-n` falls through to the
  unconditional `env_vars.remove(&unset_name)` (~line 215).

### C5 — `${!name[sub]}` indirect-through-element (Medium)

`ind_elem_err.sh`:
```sh
foo=bar
echo "A<${!foo[2]}>"        # GNU: A<> silent;   RB: A<> (match)
declare -n n=bar            # bar unset
echo "B<${!n[2]}>"          # GNU: n[2]: invalid indirect expansion; RB: B<> silent
```
GNU `parameter_brace_expand_indir` (`subst.c:7883-7935`): the
ksh93-compat "return the nameref cell" shortcut at `7895-7903` applies
only to the bare name; for `name[sub]` the element value is resolved and
used as the indirect name, and an unset/unresolvable target is
`invalid indirect expansion` (7924-7935). `nameref1.sub`'s
`echo ${!foo[0]}` (foo nameref→bar, bar scalar `one`) prints empty in
GNU; rb prints `one` — rb resolves the nameref *then* applies `[0]` to
the target value, collapsing GNU's element-vs-indirection distinction.

### C6 — `${f/x/X}` with `f`→`arr[i]` (Medium)

`patsub_elem.sh` (`nameref9.sub` verbatim):
```sh
arr=( idx1 idx2 ); i='arr[1]'
echo ${!i}; echo ${!i/x/X}
typeset -n f='arr[1]'
echo ${f}; echo ${f/x/X}
```
GNU: `idx2 idX2 idx2 idX2`. RB: `idx2 idX2 idx2` + empty.
`parameter_pattern_scalar_value` (`parameter_patterns.rs:192-225`)
resolves `f` → `arr[1]` then does `env_vars.get("arr[1]")` (line 204) —
the resolved nameref target is used as a *literal* variable name, so an
element target never reaches `array_element_parameter_value`. The same
literal-name lookup gap likely affects every `${var<op>...}` form whose
nameref target is an array element.

### C7 — Subscript command substitution re-evaluated (Medium)

`comsub_count.sh` (from `nameref10.sub`): the function expands
`x[i=0$(echo comsub >&2)]` three ways — literal string, `${x[...]}`,
`${!1}`, `$foo` (nameref). GNU emits `comsub` 4 times; RB emits it 6 —
the second echo (`${x[i=0$(...)]}`) alone emits it **3** times under
Rubash, i.e. the subscript expression is expanded once for parsing and
again (twice) during evaluation. Side-effect-count divergence, not a
text-formatting issue. Owner: `array_element_parameter_value`
(`src/executor/arrays/executor.rs:100+`) and the subscript expansion
path it delegates to.

### C8 — coproc variable binding ignores readonly/nameref (High)

`nameref11.sub:45-54` exercises `coproc` against readonly and nameref
variables:

```sh
declare -r ROVAR=42; coproc ROVAR { :; }; wait; declare -p ROVAR
declare -n ref=x;  coproc ref  { :; }; wait; declare -p ref
declare -r RO RO_PID; coproc RO { :; }; declare -p RO_PID; wait; declare -p RO RO_PID
```

- GNU: refuses to bind — `ROVAR: readonly variable`,
  `ROVAR: cannot unset: readonly variable`, `RO: readonly variable`,
  `RO_PID: not found` (gnu.err:64-70); `declare -p` still shows
  `declare -r ROVAR="42"`, `declare -n ref="x"`, `declare -r RO="x"`.
  `execute_cmd.c:~2408-2430` checks `readonly_p(v)` → `err_readonly`
  early return before `convert_var_to_array`/`bind_array_variable`.
- RB: `declare -ar ROVAR=([0]="63" [1]="60")`, `declare -an ref=(...)`,
  `declare -ar RO=([0]="63" [1]="60")`, `declare -r RO_PID="<pid>"` —
  `compound_exec.rs:905-933` unconditionally formats `"(rfd wfd)"` and
  marks `ARRAY_VARS` on the coproc NAME, destroying the nameref/readonly
  state with no diagnostic.

(Fd numbers themselves are platform-dependent; the attribute/value
corruption is not.)

### C9 — `${@:0}` drops `$0` (Medium)

`nameref11.sub:47`: `echo ${@:0}` inside the sub prints `./nameref11.sub`
under GNU (twice — before and after the `coproc @` line); rb prints
nothing. `at0.sh` confirms `${@:0}`/`${*:0}` never include `$0`:
`positional_parameter_substring` (`parameter_ops.rs:461-484`) computes
`start = (offset as usize).saturating_sub(1)` — offset 0 → skip 0 →
positional params only. GNU `pos_params` (`subst.c:3745-3779`) prepends
`$0` when the offset reaches 0.

### C10 — mapfile/declare target validation (High)

`mapfile_ref.sh`:
```sh
declare -n r
mapfile r < /dev/null
declare -p r
```
- GNU: `declare -a r=()` + stderr `warning: r: removing nameref
  attribute` — `mapfile.def` resolves the nameref, removes `-n`, binds
  an array.
- RB: `declare -an r=()`, no warning — `mapfile_builtin.rs:425-513`
  keeps the `-n` mark while marking `-a`.

`nameref18.sub` goes further: `declare -n ref=XXX[0]` then `mapfile ref`
— GNU: `mapfile: 'XXX[0]': not a valid identifier` (element references
rejected as builtin targets). RB silently creates a variable literally
named `XXX[0]` (`declare -a XXX[0]=([0]="bar")`), and
`typeset ref=4` through `ref`→`XXX[0]` likewise materializes the bogus
literal-named variable instead of element `XXX[0]`. GNU's guard is
`declare.def:553-557` `valid_array_reference(name)` → `reference
variable cannot be an array` on the LHS plus `valid_nameref_value`
(`declare.def:574`) on the RHS.

### C11 — Circular vs max-depth diagnostics (Medium)

`maxdepth.sh` builds a 10-deep cycle `a→b→…→j→a`:
- GNU: `warning: a: maximum nameref depth (8) exceeded` (NAMEREF_MAX=8,
  `variables.h:181`; warnings at `variables.c:2191/2220/3797` etc.).
- RB: `warning: a: circular name reference` — `nameref_resolution`
  iterates `0..16` (`variable_state.rs:52`) and labels *depth overflow*
  as `Circular`, so GNU's `maximum nameref depth (8) exceeded` never
  appears (missing throughout `nameref8.sub`/`nameref15.sub` stderr,
  e.g. gnu.err:14,17,20).

### C12 — `-n` on arrays / attr-removal matrix (needs-deeper-work, High)

Confirmed instances:

- `nameref22.sub:74` `declare -n array=(one two three)` on existing
  `declare -a array` → GNU `declare: array: reference variable cannot be
  an array` (`declare.def:553-557`); RB accepts → `declare -an
  array=([0]="one" [1]="two" [2]="three")`.
- `nameref15.sub` `typeset -n a=b; declare a=foo` → GNU `warning: a:
  removing nameref attribute` + `declare -a a=([1]="foo")`; RB
  `declare -an a=([0]="b" [1]="foo")` (kept `-n`, kept stale `[0]="b"`).
- `nameref20.sub` `f(){ declare -n ref=var; declare ref=(X); ...}` →
  GNU `declare -- var="X"`; RB `declare -a var=([0]="X")`.
- `nameref12.sub:58-60` `declare -n foo; declare -i foo; foo=7*6;
  declare -p foo` → GNU `declare: foo: not found` (the invalid cell
  assignment destroys/`declare -p` rejects); RB `declare -in foo`.

The full declare/typeset/assignment × nameref × array matrix needs a
dedicated pass; owners: `temporary_assignments.rs:212-345`,
`builtins/setattr/apply.rs`, `declare` print path.

### C13 — `readonly`/`typeset -r` through a nameref (Medium)

`ro_through_ref.sh`:
```sh
var=foo; typeset -n ref='var[0]'; readonly ref; typeset -p var; declare -p ref
```
- GNU: `readonly: 'var[0]': not a valid identifier`; `var` unchanged;
  `ref` stays `declare -n ref="var[0]"` — readonly is applied *through*
  the nameref and fails on the element-shaped target.
- RB: silent success; `declare -nr ref="var[0]"` — readonly was applied
  to `ref` itself. `apply.rs:114-167` resolves `nameref_target_name` and
  marks the wrong subject (and additionally permits `typeset +n` on a
  readonly nameref where GNU `declare.def:678-688` refuses — this is the
  `nameref17.sub` `declare -r foo4` vs `declare -nr foo4` hunk).

### C14 — `set -u` through nameref/array forms (needs-deeper-work, Medium)

`nameref25.sub` runs `$THIS_SH -uc '...'` fragments:
- `declare -n r='a[@]'; : "$r"` (a unset) → GNU `r: unbound variable`
  rc≠0; RB silent success → spurious `ok 5`-`ok 8`.
- `a=() k=; declare -n r='a[k]'; : "$r"` → GNU `k: unbound variable`
  (subscript eval hits unset `k`); RB `r: unbound variable` (wrong
  variable named).
- `a=() k=; "${a[k]}"` → GNU `a[k]: unbound variable`; RB
  `: command not found` (compounds an unbound-check miss with a bogus
  command lookup).

GNU owner: `subst.c:10165` `unbound_vars_is_error` applied to the
resolved expansion; RB needs the `-u` check to run on the resolved
nameref/array-element target, not the literal name.

### C15 — Diagnostic wording / missing per-builtin diagnostics (Low)

Suite stderr diffs (in-suite, prefix `./namerefNN.sub` on both sides —
not a path artifact):

- `typeset: '12345': not a valid identifier` (rb) vs
  `invalid variable name for name reference` (GNU,
  `declare.def:574-578`) — `nameref13.sub:101`.
- `nameref24.sub:24` `declare: '': not a valid identifier` missing in rb.
- `nameref18.sub:51` rb emits `(63 60): invalid variable name` — wrong
  subject string.
- `nameref11.sub` rb is missing GNU's
  `exec: '10': not a valid identifier` + `r: cannot assign fd to
  variable` (line 26, fd-var through nameref),
  `((: '0': not a valid identifier` (line 20, arithmetic through
  nameref), `printf: '/': not a valid identifier` (line 34),
  `warning: r: removing nameref attribute` (lines 27, 32).
- `nameref11.sub:19` `select r in /` — rb prints `#? 1) /` where GNU
  prints `#?` plus the `'/' : not a valid identifier` diagnostic —
  select loop-var through nameref.
- `declare: -t: invalid option` (nameref13.sub:14 `declare -nt r=a`) —
  rb's `declare` does not accept `-t` at all; GNU prints
  `declare -nt r="a"`.

### C16 — `declare -n` re-declaration / local scope (needs-deeper-work, Medium)

`nameref12.sub:63-79`: inside `f(){ unset var; declare -n ref=var;
declare -n ref; declare -p ref; }` GNU prints `declare -n ref` (bare
re-declaration resets the cell); RB prints `declare -nr ref="var[0]"`
— a stale readonly global `ref` (from line 87's failed-in-GNU
`readonly ref`) survives into the function scope and the redeclaration
does not reset it. Related: `nameref14.sub` `var` vs `foo` output and
`nameref12` line-4 extra `declare -n x`. The local-shadowing +
redeclaration semantics of an existing nameref need a dedicated pass
(`temporary_assignments.rs` local-scope path + `declare.def:645-704`).

## Stdout hunk → class map

| Diff hunk (gnu.out) | Test source | Class |
|---|---|---|
| `@@ -35` rb `one` vs GNU blank | `nameref2.sub` `echo $ref` | C1 (env leak) |
| `@@ -43` rb extra `argv[1] = <unset>` | `nameref3.sub:29` `${!foo-unset}` | C2 |
| `@@ -61` `unset` vs rb blank | `nameref4.sub:143` `${ref-unset}` | C3 |
| `@@ -123` missing `idX2` | `nameref9.sub` `${f/x/X}` | C6 |
| `@@ -148,270` mega-hunk | `nameref11.sub` onward | C1 (abort at line 76) dominating; embedded: C8 (coproc ROVAR/ref/RO), C9 (`${@:0}` ×2), C10 (`declare -an r=()`), C15 (`select` prompt, missing diagnostics) |
| all of `nameref12`–`nameref25` missing | driver loop | C1 truncation; residual per-sub divergences C4/C10/C11/C12/C13/C14/C15/C16 verified standalone |

## Environment artifacts

- **None proven.** The coproc fd numbers (`63`,`60`) and `RO_PID` values
  are platform-dependent, but the divergent `declare -an`/`-ar`
  *attributes* are Rubash-owned (C8).
- Harness caveats observed while auditing (not divergences): `recho`/
  `zecho` must be on `PATH` for both shells or `argv[...]` lines vanish;
  GNU-side `THIS_SH` must be set explicitly when running subs standalone;
  stderr/stdout interleave ordering differs because Rubash buffers
  stderr (known limitation, see AGENTS.md).
- `rb.err` `./namerefNN.sub` vs absolute-path prefixes in some of my
  standalone captures are invocation artifacts; in-suite both sides use
  `./namerefNN.sub`.

## Coverage notes / open items

- `nameref12`–`nameref25` were compared **standalone** (results embedded
  in classes above); a full-suite rerun after a C1 fix will expose any
  remaining hunks more precisely.
- The exact branch in `execute_same_shell_script` that decides
  `this_shell_invocation` should be re-checked when fixing C1 — the
  observed leak (non-exported readonly visible, `declare -p` fresh)
  indicates the env swap and the typed-store rebuild at
  `external_finish.rs:158-187` are effective for `env_vars` but not for
  the `std::env` channel.
- No `src/` files were modified; all reproducers live under
  `.tmpwork/audit/nameref/`.
