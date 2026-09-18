# array / quotearray compatibility audit — 2026-09-18

Read-only audit of the GNU Bash `array` and `quotearray` suites against
Rubash at code baseline `2494bc57` (code-identical to `c282a850`), per
`docs/audit-baseline-2026-09-18.md`. No `src/` files were modified.

## Method

- Oracle: owner-compiled GNU Bash 5.3.0 at WSL `/usr/local/bin/bash`,
  invoked from a **script file**
  (`MSYS_NO_PATHCONV=1 wsl /usr/local/bin/bash /mnt/d/repo/rubash/<file>.sh`),
  never `bash -c` (Windows/WSL arg passthrough corrupts quoting).
- Rubash: `target/debug/rubash.exe <file>.sh`, stdin `/dev/null`,
  `timeout -k 5 40`, `__RUBASH_NO_UPSTREAM_SCRIPTS=1`.
- Suite artifacts: `target/issue-suites/results/true-baseline/{array,quotearray}/`.
- Per-subtest artifacts: `.tmpwork/audit/array/<sub>.{gnu,rb}.{out,err,rc}`.
- Minimal reproducers: `.tmpwork/audit/array/repros/rNN-*.sh` with
  `.gnu.out/.gnu.err/.gnu.rc` and `.rb.out/.rb.err/.rb.rc` byte-captured.
- GNU C source in `third_party/bash/` is the specification; every class
  below cites the owning C function.

## Headline numbers

- `array`: 137 stdout diff lines; `quotearray`: 64 stdout diff lines
  (baseline ledger; stderr excluded).
- Per-subtest scan: diverging subs are `array{1,6,10,11,19,20,22,24,25,
  26,27,29,32,33}` and `quotearray{1,2,3,4,5}`. `array5` was an
  environment artifact (stale capture; see Environment artifacts below).

## Divergence classes

### C1 — Compound-assignment token in a non-declaration command leaks `__RUBASH_CA1__`

- Tests: `array1.sub:1` (`printf "%s\n" -a a=(a 'b  c')`).
- Repro: `repros/r01-marker-leak.sh`.
  - GNU stdout: *(empty)*; stderr `syntax error near unexpected token '('`; rc=2.
  - RB stdout: `-a\na=__RUBASH_CA1__(a 'b  c')`; rc=0.
- GNU cite: `parse.y:5653` recognizes `=(` only when
  `assignment_acceptable(last_read_token)` or `PST_ASSIGNOK`
  (`parser.h:43`) is set; `PST_ASSIGNOK` is raised only for
  `ASSIGNMENT_BUILTIN`s at `parse.y:5803-5804`. `printf` is not one, so
  `(` is a `shellbreak` metacharacter (`parse.y:5688`) → grammar error.
- RB owner: `src/parser/token_actions.rs:44-69` injects
  `COMPOUND_ASSIGNMENT_MARKER` (`src/executor/types.rs:40`) for *any*
  post-command `name=(...)` word without checking whether the command is
  an assignment builtin; the marker then survives to argv and is printed.
- Verdict: Rubash bug. Two semantic layers are wrong: (a) the `=(` form
  is admitted outside assignment-builtin context instead of being a parse
  error, and (b) the internal marker byte string reaches user-visible
  output.
- Severity: **High** — user-visible internal-marker leak plus a missing
  syntax error.

### C2 — `eval`/compound-assignment element quote grouping lost (`-iname 'abc` split)

- Tests: `array6.sub:58,61,72,75,78` — `eval a2=("${a[@]/#/\"-iname \'\"}")`,
  `eval a2=("${a[@]/#/"-iname '"}")` (and the `${@/...}` positional form).
- Repro: `repros/r02-compound-element-split.sh`.
  - GNU: `<-iname 'abc>` `<-iname 'def>` for all three forms.
  - RB: `<-iname>` `<'abc>` `<-iname>` `<'def>` for the eval forms;
    direct `a2=(...)` is correct.
- GNU cite: the eval'd string is reparsed; `parse.y:5653-5683`
  (`parse_compound_assignment`, `parse.y:7104`) keeps each list word's
  quote grouping, so `-iname 'abc` (unterminated quote inside the
  replacement) remains one element. Element expansion via
  `arrayfunc.c:557 expand_compound_array_assignment` →
  `parse.y:7015 parse_string_to_word_list` (generated `y.tab.c:9374`).
- RB owner: `src/parser/token_actions.rs:104-133,227-269`
  (`collect_compound_assignment`/marker path) and
  `src/executor/assignment_expansion.rs` (marker consumer
  ~418-460,539-555,713); on re-parse through `eval`, element boundaries
  that depend on preserved quoting are re-split on whitespace.
- Verdict: Rubash bug; this is a **rubash#117 word-level fast-path** case
  — the compound RHS is preserved as *text* behind a marker and then
  re-split by a text splitter that does not carry GNU's per-word
  `W_QUOTED` provenance. Do not add another symptom predicate; the
  element list needs real word-list semantics.
- Severity: **High** — silent argv corruption of array contents.

### C3 — `${foo}"$@"` / `x${a[@]}` concatenation collapses the element list to one word

- Tests: `array6.sub:111-119` (`recho ${foo}"$@"`, `${foo}"${array[@]}"`),
  `array26.sub` (`x${a[@]}` / `x$@` variants).
- Repros: `repros/r03-concat-dollarat.sh`, `repros/r18-star-vs-at-nosplit.sh`.
  - GNU `printf '<%s>\n' ${foo}"${array[@]}"` (IFS=):
    `<var with spacesab>` `<cd>` `<ef>`.
  - RB: `<var with spacesab cd ef>` — one word.
  - GNU `x${a[@]}` (IFS=+): `<xaa>` `<bb>`; RB: `<xaa bb>`.
- GNU cite: `subst.c:2957 string_list_dollar_at` produces a `WORD_LIST`;
  `expand_word_internal` (`subst.c:11229+`) splices that list into the
  surrounding word so literal text glues to the first (and last)
  element. Element boundaries are structural, not IFS-dependent.
- RB owner: `src/executor/arrays/executor.rs:201 array_at_word_values`
  admits only whole-word `"${a[@]}"` shapes; a concatenated word falls
  through to the scalar join `join_expanded_array_values`
  (`src/executor/command_substitution_values.rs:637-648`, `[@]` →
  `join(" ")`), after which field splitting cannot recover boundaries
  when space is not in IFS.
- Verdict: Rubash bug; **#117-class wrong invariant** — "`[@]` joined by
  space + field splitting is equivalent to a word list" is false
  whenever IFS lacks space or elements contain IFS characters.
- Severity: **High**.

### C4 — `${arr[*]}`/`${*}` scalar-context join and `${var-$*}` word-list distinction

- Tests: `array20.sub` (`b=${*/a/x}` with IFS=+ → `x+b+c`; RB `x b c`),
  `array24.sub` (IFS-empty `${var-$*}` / `${var-${A[*]}}` word splitting),
  `array26.sub`.
- Repros: `repros/r04-star-ifs-join.sh`, `repros/r10-op-array-default.sh`.
  - `b=${*/a/x}` (IFS=+): GNU `x+b+c`; RB `x b c`.
  - `printf '%s\n' ${var-$*}` (IFS=''): GNU prints `abc`/`def ghi`/`jkl`
    (three words); RB prints `abcdef ghijkl` (one word).
- GNU cite: `subst.c:3030 string_list_pos_params` dispatches on
  `pchar`/`quoted`/`PF_ASSIGNRHS`; `subst.c:2902
  string_list_dollar_star` joins with `IFS[0]`; `subst.c:4487`
  `expand_no_split_dollar_star` is set for op `=` **and** whenever the
  expansion sits on an assignment RHS (`PF_ASSIGNRHS`, subst.c:11527);
  for op `-` the `$*` value stays a word list.
- RB owner: `src/executor/expand_braced_replacement.rs:39-63` joins
  positional `*` with a hard-coded `" "` unless a narrow
  `ASSIGNMENT_RHS`-and-null-IFS flag is set — the flag only covers the
  `${var=word}` operator interior, not ordinary assignment words, so the
  wrong join separator is used. For `${var-$*}`, `expand_braced_ops.rs`
  (`parameter_operator_value`/`expand_parameter_word`) similarly
  collapses the word list into a single joined string.
- Verdict: Rubash bug; same #117 invariant family — "join early, split
  later" does not model `PF_ASSIGNRHS`/`expand_no_split_dollar_star`.
- Severity: **Medium-High** (corrupts values whenever IFS≠space runs
  through scalar/operator contexts).

### C5 — Arithmetic subscript side effects are deferred past sibling expansions

- Tests: `array10.sub` —
  `echo "${days[${count}],,}, ${days[$((count++))],,}, ${days[$((count++))],,}"`.
- Repro: `repros/r05-arith-sidefx.sh`.
  - GNU: `monday, monday, tuesday`; RB: `monday, monday, monday`.
- GNU cite: `arrayfunc.c:1355 array_expand_index` →
  `expand_arith_string`/`evalexp` executes `count++` immediately during
  each `${...}` evaluation; expansions in one word run left-to-right
  (`subst.c:11229 expand_word_internal`).
- RB owner: `src/executor/arrays/executor.rs:127-163` queues writes into
  `PENDING_SUBSCRIPT_WRITES`
  (`src/executor/expand_braced_indices.rs:7-32`), which is applied only
  once per word at `src/executor/command_prepare.rs:894`. Every `$((count++))`
  in the same word therefore evaluates against the same pre-word `count`.
- Verdict: Rubash bug. The deferred-write mechanism is a wrong invariant
  for any word containing more than one side-effecting subscript.
- Severity: **Medium** (wrong values + lost mutations; ordering only
  matters with side effects).

### C6 — `declare`/element-assignment path cannot handle `]`/`[` inside subscripts

- Tests: `array11.sub` — `declare foo["foo[bar]"]=bowl`,
  `array2["foo[bar]"]=bleh`, `foo["version[agent]"]=version.agent`.
- Repro: `repros/r06-assoc-bracket-keys.sh`.
  - GNU: `declare -A foo=(["foo[bar]"]="bowl" ["version[agent]"]="version.agent")`.
  - RB: `foo[bar]` key is silently dropped (no diagnostic).
- GNU cite: `arrayfunc.c:1288 tokenize_array_reference` →
  `subst.c:2186 skipsubscript`/`skip_to_delim` walks quotes, backslashes
  and nested `[...]`/`$(...)`; `declare.def:597` validates the reference.
- RB owner: `src/builtins/declare/assign.rs:364-370
  declare_indexed_element` splits on the first `[` and requires the
  *last* char `]`; a subscript containing `[` returns `None` and the
  operand falls through to the scalar-name path
  (`assign.rs:195-318`), creating a phantom variable named
  `foo["foo[bar]"]`. Direct element assignment
  (`src/executor/array_assignment_exec.rs:17-44`) handles these keys, so
  the gap is specific to the `declare` operand path.
- Verdict: Rubash bug (silent data loss).
- Severity: **Medium**.

### C7 — `declare` array/scalar conversion and compound-value expansion

- Tests: `array19.sub` (large cluster).
- Repro: `repros/r07-declare-semantics.sh`.
  - `declare a='(1 2 3)'` (no prior array): GNU `declare -- a="(1 2 3)"`
    scalar; RB `declare -a a=([0]="1" [1]="2" [2]="3")`.
  - `declare -a var="([$(echo total 0)]=1 [2]=2])"`: GNU runs the comsub,
    gets `total 0` arith error, ends `var=()`; RB stores 4 literal
    fragments `[$(echo`, `total`, `0)]=1`, `[2]=2]` and never expands.
  - `declare -l foo="AbCdE"` on existing `foo=(one two three)`: GNU
    assigns element 0 (`[0]="abcde"`, rest kept); RB replaces the whole
    array with `[0]="abcde"`.
  - `declare -a e='($(echo Darwin))'`/`$y`: GNU `[0]="Darwin"`; RB literal
    `\$(echo`/`Darwin)` fragments.
- GNU cite: `declare.def:925-950` `compound_array_assign` vs
  `simple_array_assign` (a `(…)` value is compound only when the array
  exists or `-a`/`-A` is creating one); `declare.def:991-1021` dispatch —
  `assign_array_var_from_string` (`arrayfunc.c:910` →
  `arrayfunc.c:557 expand_compound_array_assignment` →
  `parse.y:7015 parse_string_to_word_list` + real expansion per word) vs
  `bind_array_variable (name, 0, value, …)` for simple assign.
- RB owner: `src/builtins/declare/assign.rs:308-316` treats any
  `(…)`-bracketed value as compound regardless of `array_exists`;
  `expand_compound_array_value` (`assign.rs:377+`) only expands `${...}`
  shapes — `$a`, `$(…)`, and word splitting inside the value are not
  performed; the non-compound path at `assign.rs:314-317` stores a scalar
  over an existing array instead of binding element 0.
- Verdict: Rubash bug (three sub-issues: classification, missing
  expansion of declare-arg compound values, and element-0 binding).
- Severity: **High** (visible `declare -p` shape and dropped comsub
  side effects).

### C8 — Indexed subscript keeps quotes where GNU evaluates them arithmetically; assoc `let` keeps quotes literal where RB strips them

- Tests: `array25.sub` — `${a[' ']}` on an indexed array;
  `let "a[\" \"]=11"` / `(( a[" "]=11 ))` with `assoc_expand_once`.
- Repro: `repros/r08-blank-subscripts.sh`.
  - `${a[' ']}` (indexed): GNU stderr `' ': arithmetic syntax error:
    operand expected (error token is "' '")`, no output line; RB prints
    `2. 0`.
  - `let "a[\" \"]=11"` (assoc, expand-once): GNU stores a *literal*
    3-char key `" "` → `["\" \""]="11"`; RB dequotes and overwrites key
    ` ` → `[" "]="11"`.
- GNU cite: `arrayfunc.c:1355 array_expand_index` —
  `expand_arith_string` on the subscript text; quotes are arith syntax,
  so `' '` is a syntax error for indexed arrays, while for associative
  arrays under `assoc_expand_once`/`AV_NOEXPAND` the subscript text is
  the key verbatim (`arrayfunc.c:1288-1348`,
  `subst.c:2186 skipsubscript`; whitespace-only subscripts are explicitly
  allowed by the `#if 0` at `arrayfunc.c:1317-1323`).
- RB owner: `src/executor/arrays/executor.rs:151-153`
  (`strip_matching_quotes` on indexed subscripts) and
  `src/executor/subscript_expansion.rs:29-49
  expand_subscript_string`/`mask_subscript_escapes` plus the `let`/`(( ))`
  assoc path — quote characters are stripped instead of being passed to
  arith (indexed) or kept as key data (assoc under expand-once).
- Verdict: Rubash bug, both directions (accepts what GNU rejects; loses
  key data GNU preserves).
- Severity: **Medium**.

### C9 — `"${a[@]:-word}"` / `"${@:-word}"` collapse empty elements into one word

- Tests: `array22.sub` (`a[0]= a[1]=; recho "${a[@]:-y}"`), plus the
  positional `${@:-y}` cases.
- Repro: `repros/r09-default-empty-elements.sh`.
  - GNU: `<>` `<>` (two empty words); RB: `< >` (one word).
- GNU cite: `subst.c:2957 string_list_dollar_at` /
  `parameter_brace_expand_word` (`subst.c:7663`, array branch
  ~`7713-7755`): a quoted `[@]` under `:-` still expands as a word list —
  the operator only substitutes when the parameter is unset/null per
  array semantics.
- RB owner: `src/executor/parameter_errors.rs:237-247
  parameter_operator_value` reduces `name[@]`/`name[*]` to
  `values.join(" ")` (a scalar), and `expand_braced_ops.rs:28-39` returns
  it as one word.
- Verdict: Rubash bug; the operator path has no way to return a word
  list — same structural gap as C3.
- Severity: **Medium**.

### C10 — Malformed subscripts / `]`-only keys accepted by builtins

- Tests: `array27.sub`, `quotearray2.sub` — `declare A[$k]=X` /
  `declare "A[$k]=X"` / `read "A[$k]"` / `printf -v "A[$k]"` for
  `k=']'`; `declare "A[]]=X"`.
- Repro: `repros/r11-special-keys.sh`.
  - GNU: `declare: 'A[]]=X': not a valid identifier`; array unchanged.
  - RB: stores `[0]="[]]=X"` inside the assoc array; `A[]]=X` direct
    assignment reports `A[]]]: bad array subscript` while GNU treats the
    whole word as a command (`A[]]=X: command not found`).
- GNU cite: `declare.def:597-601` → `arrayfunc.c:1288
  tokenize_array_reference` → `subst.c:2186 skipsubscript`: `A[]]` has
  `len==1` → invalid reference → `sh_invalidid`. The same
  `valid_array_reference` check gates `read`/`printf -v` (`read.def`,
  `printf.def` `-v` operand validation).
- RB owner: `declare_indexed_element`
  (`src/builtins/declare/assign.rs:364-370`) accepts subscript `]`; the
  read validator `src/executor/read_builtin.rs:8-24 is_valid_read_name`
  and `valid_printf_array_target` (`src/builtins/printf.rs:215`) accept
  any non-empty `[…]` tail, so `]`, `*`-as-key mishandling, etc. all pass.
- Verdict: Rubash bug — no equivalent of `valid_array_reference`'s
  empty-subscript/`]`-termination rules on builtin operand paths.
- Severity: **Medium**.

### C11 — `array_expand_once`/`assoc_expand_once` not honored: double expansion executes command substitutions in subscripts

- Tests: `array32.sub` (`shopt -s array_expand_once`;
  `a[$subscript]=hi`, `a=( [$subscript]=hi )`, `printf -v`, `read`,
  `declare -i`, `test -v`, `let`, `(( a[$subscript]++ ))`,
  `${#a[$subscript]}`); `quotearray1.sub`/`quotearray5.sub` (assoc side).
- Repro: `repros/r12-array-expand-once.sh`.
  - GNU: `declare -a a` (empty), stderr `$(echo INJECTION! >&2 ; echo 0):
    arithmetic syntax error` — **the comsub never runs**.
  - RB: stderr `INJECTION!`, `a=([0]="hi")` — the nested `$(…)` executed;
    the compound form additionally splits the literal text into six
    elements.
- GNU cite: `arrayfunc.c:1355 array_expand_index`: with `AV_NOEXPAND`
  (array_expand_once) the subscript skips `expand_arith_string` and goes
  straight to `evalexp`, which rejects `$(…)` as a syntax error without
  executing it. `arrayfunc.c:392-398` /
  `assign_array_element_internal` for the assignment path;
  `subst.c:1336 extract_array_assignment_list` for compound elements.
- RB owner: `src/executor/array_assignment_exec.rs:309
  eval_arithmetic_expansion_value` and `src/executor/arithmetic/mod.rs:236`
  — no `array_expand_once` gate exists (the shopt is registered in
  `src/builtins/shopt.rs:252-260` but nothing consumes it in the
  executor); the arithmetic evaluator executes `$(…)` it finds in
  subscript text. The compound-element path splits on whitespace inside
  `$(...)`.
- Verdict: Rubash bug.
- Severity: **Critical** — uncontrolled command substitution execution
  (`INJECTION!` runs) plus silently stored garbage elements.

### C12 — `[[ ]]` conditional: subscripted operands double-expanded; `(`/`<` inside `a[...]` aborts the whole command

- Tests: `quotearray1.sub` — `[[ assoc[$key] -eq assoc[$key] ]]`,
  `[[ index[7<(4+2)] -le assoc[0] ]]`, `[[ -v assoc[$key] ]]`.
- Repro: `repros/r14-cond-arith.sh`.
  - GNU: `0`, `0`, `survived`.
  - RB: `1`, then `syntax error near unexpected token in conditional
    expression`, rc=2 — the rest of the file never runs.
  - Also: RB expands `assoc[$key]` during word expansion to the literal
    text `assoc[x],b[$(echo uname >&2)]`, then re-parses it as
    arithmetic → `arith syntax error`; GNU evaluates the subscript once
    inside the arith evaluator and looks up key
    `x],b[$(echo uname >&2)` → 42.
- GNU cite: `parse.y:3780` — `(` is not an operator while
  `PST_CONDCMD` is set, so `index[7<(4+2)]` is one word; cond operands
  reach `expr.c`/`arrayfunc.c array_variable_part` →
  `expand_subscript_string` (`arrayfunc.c:396`) which expands the
  subscript *inside* the arithmetic evaluator exactly once.
- RB owner: `src/parser/conditional_command.rs`
  (`conditional_expression_is_invalid` ~:60-116 and arg collection
  ~:118-150) — lexer-produced `<`/`(` tokens inside `[...]` are not
  suppressed; and `src/executor/conditional.rs:150-160` numeric binary
  ops arith-evaluate the already-expanded operand text instead of the
  raw subscript.
- Verdict: Rubash bug — two issues: (a) `[[ ]]` parser abort (fatal,
  kills the rest of the script), (b) assoc-subscript double expansion.
- Severity: **High** (script abort + wrong results).

### C13 — `unset name[subscript]` never expands the subscript

- Tests: `quotearray3.sub`, `quotearray5.sub` — `unset 'a[$key]'`,
  `unset "a[\$key]"`, `unset a[$key]` for keys `$(echo foo)`, `@`, etc.
- Repro: `repros/r15-unset-subscript.sh`.
  - GNU: `unset 'a[$key]'` expands `$key` in the subscript → removes the
    `$(echo foo)` element → `a=()`; `unset 'a[$key]'` with `key=@`
    removes key `@` → `a=([.]="v1")`.
  - RB: element kept (`["$(echo foo)"]="1"`, `["@"]="v0"` remain).
- GNU cite: `builtins/set.def` `unset_builtin` (~990-1010) →
  `arrayfunc.c` `unbind_array_element` → `assoc.c`/`arrayfunc.c:396
  expand_subscript_string` — the subscript of a *syntactically valid*
  array reference is expanded once during unbinding.
- RB owner: `src/executor/unset_arrays.rs:235-241` —
  `subscript.trim_matches('\'').trim_matches('"')` is the entire
  subscript handling; no expansion pass exists. The builtin-only fallback
  `src/builtins/set/unset.rs:139-152` is even shallower.
- Verdict: Rubash bug.
- Severity: **Medium-High** (elements silently survive `unset`).

### C14 — `test -v`/`[[ -v` on `assoc[@]`/`assoc[*]`, assoc iteration order, and nameref-to-`a[@]`

- Tests: `quotearray3.sub`, `quotearray4.sub` —
  `test -v assoc[$key]` (`key=@`), `test -v 'assoc[@]'`,
  `test -v 'array[@]'`, `echo ${!aref}` (`aref=assoc[@]`),
  `declare -n nref=assoc[@]; echo $nref`.
- Repro: `repros/r16-assoc-order-testv.sh`.
  - `test -v 'assoc[@]'` (key `@` present): GNU `0`; RB `1`.
  - `echo ${!aref}` / `${nref}`: GNU `star bang at` (hash order) both;
    RB `at star bang` (insertion order) and `at` (single element).
- GNU cite: `test.c:650-674` `unary_operator` case `v` —
  `valid_array_reference` + `AV_ATSTARKEYS` (line 666) makes `@`/`*`
  literal keys for assoc arrays; `subst.c:7922 chk_atstar` +
  `parameter_brace_expand_indir` (`subst.c:7883`) govern `${!aref}`;
  `assoc.c` hash-bucket order is the iteration order.
- RB owner: `src/builtins/test/variable.rs:8-24` returns `false`
  unconditionally for assoc `[@]`/`[*]` instead of looking up the literal
  key; `src/executor/prompt_expansion.rs:65-73 indirect_target_values`
  uses insertion-order `array_values` instead of
  `assoc_hash_ordered_values`; the nameref-to-`assoc[@]` expansion
  (`arrays/executor.rs:492-537` + nameref resolution) collapses to one
  element.
- Verdict: Rubash bug. Note: the hash-order part is an ordering
  divergence — deterministic in GNU but implementation-defined; worth
  matching via the existing `assoc_hash_ordered_values` port.
- Severity: **Medium**.

### C15 — `\x01` (CTLESC-carrier) bytes corrupted in function-local compound assignment

- Tests: `array29.sub` — `local -a foo=( "${var[@]}" )`,
  `local -A foo=( v "${var[@]}" )`, `local -A foo=( [$'\x01']="${v2[@]}" )`.
- Repro: `repros/r17-ctlesc.sh`.
  - GNU: `declare -a foo=([0]=$'\001\001\001\001')`.
  - RB: `declare -a foo=([0]="001001001001")` — each raw `\x01` byte was
    mangled into the text `001`; the global-scope form is correct.
- GNU cite: `arrayfunc.c:557 expand_compound_array_assignment` +
  `strtrans.c` CTLESC handling; `\x01` is ordinary data in an element.
- RB owner: the `local`/`declare` compound path —
  `src/builtins/declare.rs` + `src/builtins/declare/storage.rs`
  (`decode_ansic_storage_value`/`quote_*` round-trip ~:142-240) collides
  with Rubash's in-band carrier bytes (`src/executor/types.rs`,
  `\x01`/`\x10` markers); literal `\x01` element data is interpreted as a
  carrier/escape and re-rendered as `001`.
- Verdict: Rubash bug — carrier-byte collision, the highest-risk class
  per AGENTS.md.
- Severity: **Medium-High**.

### C16 — Failed indexed↔assoc conversion clears the variable

- Tests: `array33.sub` — `declare -a A=([1]=1)` on an associative `A`.
- Repro: `repros/r13-type-conversion.sh`.
  - GNU: `cannot convert associative to indexed array`, `A` keeps
    `([1]="1")`.
  - RB: same diagnostic, but `declare -p A` afterwards shows
    `declare -A A` — the previous value was dropped.
- GNU cite: `declare.def:914` — `builtin_error ("%s: cannot convert
  associative to indexed array")` → `any_failed++; NEXT_VARIABLE()`;
  the variable cell is left untouched.
- RB owner: `src/builtins/declare/assign.rs` / `src/builtins/declare.rs`
  — the conversion error path discards or overwrites the existing cell
  instead of leaving it untouched.
- Verdict: Rubash bug (state corruption on the error path).
- Severity: **Medium**.

### C17 — `${#a[$subscript]}` and related length/reference paths double-expand the subscript

- Tests: `array32.sub` (`echo ${#a[$subscript]}` → GNU arith error, RB
  prints `0` after running the comsub); part of the same
  expand-once family as C11 but on the length path.
- GNU cite: `subst.c` `parameter_brace_expand_length` (subscript via
  `expand_subscript_string`/`array_expand_index`, arrayfunc.c:1355).
- RB owner: `src/executor/expand_braced_indices.rs:135-203`
  (`expand_braced_length_parameter` → `eval_conditional_arith_value_with_writes`
  on the raw subscript text — no expand-once gate, comsub executes).
- Verdict: Rubash bug; same fix family as C11.
- Severity: **Critical** when it shares the comsub-execution bug.

### C18 — Assoc-array `@`/`*` unset semantics and `declare -A` bare-element diagnostics

- Tests: `quotearray3.sub` (`unset assoc[@]` unsets *key* `@` for assoc;
  `unset array[@]` unsets/flushes an indexed array per compat level);
  `array33.sub` (`declare -A A=(x x)` → GNU `must use subscript…`).
- Repro: `repros/r15-unset-subscript.sh` (third block), `r13`.
- GNU cite: `arrayfunc.c` `unbind_array_element` (~1163-1231):
  `ALL_ELEMENT_SUB` handling differs for assoc (literal key `@`/`*`)
  vs indexed (flush/unset by compat level).
- RB owner: `src/executor/unset_arrays.rs:244-257` flushes indexed
  `[@]`/`[*]` to `=()`; the assoc key-`@` removal works only when the
  subscript is spelled literally (C13 covers the expanded case).
  `declare -A A=(x x)` bare-element rejection exists
  (`assign.rs:290-301 assoc_bare_element`) but see C16 for the state
  after error.
- Verdict: partially working; residual divergences folded into C13/C16.
- Severity: **Low-Medium**.

### C19 — Diagnostics wording / control-flow differences on error paths

- Examples (`array.err`, `quotearray.err`):
  - `c[-2]` bad subscript: GNU reports `bad array subscript` once;
    RB reports `c: readonly variable` then `c: bad array subscript`.
  - `declare` on readonly `c`: GNU `declare: c: readonly variable`;
    RB `declare: c: cannot destroy array variables in this way`.
  - Malformed-array / not-found messages: `declare: array: not found`
    vs RB variants; RB prefixes some errors with `rubash:`.
- GNU cite: `arrayfunc.c err_badarraysub`,
  `builtins/declare.def` error prologues.
- RB owner: per-site diagnostics in
  `src/builtins/declare.rs`, `src/builtins/declare/assign.rs`,
  `src/executor/array_assignment_exec.rs`,
  `src/executor/unset_arrays.rs`.
- Verdict: mixed — mostly cosmetic wording deltas, but several reflect
  genuinely different control flow (RB continues past errors GNU treats
  as fatal to the command, and vice versa). Not separately actionable;
  fold into the owning class.
- Severity: **Low**.

## Environment / harness artifacts (NOT Rubash bugs)

- **`array5.sub`**: earlier capture showed a spurious diff caused by a
  missing `TMPDIR` directory (`$TMPDIR/bash-test-$$` not created — the
  glob `*` expanded in the wrong cwd). Re-run with a proper temp dir
  produces identical output. Classify as harness artifact.
- **`od`/`expr` formatting** noted in the baseline doc — WinuxCmd is not
  on the Rubash-side PATH; these are environment, not semantics.
- **Stale `.diff` files** in `.tmpwork/audit/array/` may postdate fixed
  artifacts; re-run `diff <sub>.gnu.out <sub>.rb.out` before trusting
  one (array5 is the example).

## Cross-cutting note for rubash#117

Every "join-with-space-then-field-split" path above (C3, C4, C9) shares
one wrong invariant: **a quoted/unquoted `[@]` element list is modeled
as a joined `String` and re-split later**. GNU never does this —
`string_list_dollar_at`/`string_list_pos_params` return `WORD_LIST`s
and `expand_word_internal` splices them. Fixes should converge on
carrying element lists (or `W_HASQUOTEDNULL`-equivalent provenance)
through `expand_word_mut_with_context`, not on adding admission
predicates to `array_at_word_values`.

Similarly, C2 and C11 show the `COMPOUND_ASSIGNMENT_MARKER` text-marker
design losing quote/element provenance across re-parse (eval, declare
args); the durable fix is preserving a real word list, not extending the
marker-escape tables.

## Repro index

All under `.tmpwork/audit/array/repros/`:

| Repro | Class | GNU rc | RB rc | stdout | stderr |
|---|---|---|---|---|---|
| r01-marker-leak | C1 | 2 | 0 | diff | diff |
| r02-compound-element-split | C2 | 0 | 0 | diff | — |
| r03-concat-dollarat | C3 | 0 | 0 | diff | — |
| r04-star-ifs-join | C4 | 0 | 0 | diff | — |
| r05-arith-sidefx | C5 | 0 | 0 | diff | — |
| r06-assoc-bracket-keys | C6 | 0 | 0 | diff | — |
| r07-declare-semantics | C7 | 0 | 0 | diff | diff |
| r08-blank-subscripts | C8 | — | — | diff | diff |
| r09-default-empty-elements | C9 | 0 | 0 | diff | — |
| r10-op-array-default | C4 | 0 | 0 | diff | — |
| r11-special-keys | C10 | 0 | 0 | diff | diff |
| r12-array-expand-once | C11 | 0 | 0 | diff | diff |
| r13-type-conversion | C16 | 0 | 0 | diff | diff |
| r14-cond-arith | C12 | 0 | 2 | diff | diff |
| r15-unset-subscript | C13 | 0 | 0 | diff | — |
| r16-assoc-order-testv | C14 | 0 | 0 | diff | — |
| r17-ctlesc | C15 | 0 | 0 | diff | — |
| r18-star-vs-at-nosplit | C3 | 0 | 0 | diff | — |
| r19-assoc-complex-key | — (control, matches) | 0 | 0 | same | same |
