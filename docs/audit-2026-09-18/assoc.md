# assoc compatibility audit — 2026-09-18

Read-only audit of the GNU Bash `assoc` suite against Rubash, per
`docs/audit-baseline-2026-09-18.md`. **No `src/` files were modified.**
The baseline ledger records **171 stdout diff lines** for `assoc`; stderr
is tracked separately (`gnu.err`/`rb.err`).

## Method

- Oracle: owner-compiled GNU Bash 5.3.0 at WSL `/usr/local/bin/bash`,
  invoked from a **script file**
  (`MSYS_NO_PATHCONV=1 wsl /usr/local/bin/bash /mnt/d/repo/rubash/<file>.sh`),
  never `bash -c` (the wsl.exe passthrough collapses `\\` and corrupts
  quoting baselines).
- Rubash: `target/debug/rubash.exe <file>.sh`, stdin `/dev/null`.
- Suite artifacts: `target/issue-suites/results/true-baseline/assoc/`
  (`gnu.out`, `rb.out`, `gnu.err`, `rb.err`).
- Test sources: `target/issue-suites/results/bash-tests-rw/assoc.tests`
  and `assoc1.sub`–`assoc19.sub`.
- Minimal reproducers: `.tmpwork/audit/assoc/p*.sh`, run through both
  shells and compared line-for-line.
- GNU C source in `third_party/bash/` is the specification; every class
  below cites the owning C function.

## Headline numbers

- 171 stdout diff lines in the ledger; ~40 distinct hunks cluster into
  **14 divergence classes** below.
- stderr divergences (separate ledger): Rubash-only `syntax error:
  unexpected EOF` (assoc9), `arithmetic syntax error` (assoc6),
  `wait: %N: no such job` ×2 (assoc18), `myarray[]]]: bad array
  subscript` (assoc5); GNU-only `not a valid identifier` diagnostics
  (assoc5:26, assoc9:84/96/154) and `"": bad array subscript`
  (assoc11:34). Rubash also emits 13 `stderr` lines from `$(... >&2)`
  subscripts where GNU emits 9 (subscript expansion count differs).

## GNU model reference (used by several classes below)

- Hash function: `hashlib.c:208 hash_string` — FNV-1, `i = i*16777619`
  then `i ^= *s` per byte (`char` is signed on x86).
- Bucket: `hashlib.c:51 HASH_BUCKET` — `hash & (nbuckets-1)`.
- Insert: `hashlib.c:337-339 hash_insert` — head-insert into the chain.
- Iterate: `assoc.c:481-499 assoc_to_word_list_internal` /
  `hashlib.h:66 hash_items` — bucket 0..nbuckets-1, chain head→tail
  (most-recent-first within a bucket).
- Grow: `hashlib.c:41-42 HASH_SHOULDGROW` (nentries ≥ nbuckets*2) →
  `hashlib.c:154-160 hash_grow` ×4; `hashlib.c:139-148 hash_rehash`
  re-inserts at chain heads (reverses intra-bucket order).
- Table sizes: `declare -A` → 1024 buckets (`assoc.h:28
  ASSOC_HASH_BUCKETS`, `variables.c:2857 make_new_assoc_variable`);
  scalar→assoc conversion → 128 (`arrayfunc.c:117 convert_var_to_assoc`
  → `assoc_create(0)` → `hashlib.h:72 DEFAULT_HASH_BUCKETS`);
  `hashed_filenames` → 256 (`hashcmd.h:24`, `hashcmd.c:46`);
  `aliases` → 64 (`alias.c:49,75`).
- Dynamic assoc vars: `variables.c:1895-1897` creates `BASH_CMDS`/
  `BASH_ALIASES` via `init_dynamic_assoc_var` at every shell init;
  `build_hashcmd` (`variables.c:1675-1705`) and `build_aliasvar`
  (`variables.c:1745-1775`) rebuild the variable's table on each access
  by iterating the source table's buckets and `assoc_insert`-ing into a
  fresh table of the *source* nbuckets.
- `${!a[@]}`: `subst.c:10017-10046` → `arrayfunc.c:1669 array_keys` →
  `assoc.c:508 assoc_keys_to_word_list` for assoc vars.
- Compound assign: `arrayfunc.c:557 expand_compound_array_assignment`
  (word list via `parse_string_to_word_list`, **no** field splitting for
  assoc vars — the expansion is deferred, lines 591-599) →
  `arrayfunc.c:700 assign_compound_array_list` → kv-pair dispatch
  `arrayfunc.c:671 kvpair_assignment_p` / `arrayfunc.c:630
  assign_assoc_from_kvlist` (per-pair `expand_subscript_string` /
  `expand_assignment_string_to_string`, lines 644/652) or `[k]=v`
  elements (lines 752-836, `skipsubscript` at 758, key expansion at 817,
  value at 865).
- Subscript boundary: `subst.c:2086 skip_matched_pair` /
  `subst.c:2186 skipsubscript` — honors `\`, `'`, `"`, nested `[`,
  backquotes, `$(...)` and `${...}` (lines 2146-2170).
- `@K`/`@k`/`@A` transforms: `subst.c:8839 array_transform` —
  `@K`→`assoc_to_kvpair` (`assoc.c:346-411`: keys `sh_double_quote`d iff
  `sh_contains_shell_metas`, lone `@`/`*`, or `ansic_shouldquote`;
  values always quoted), `@k`→`assoc_to_kvpair_list` (`assoc.c:514-533`
  bare words through `string_list_pos_params`, so `[*]` joins),
  `@A`→`assoc_to_assign` (`assoc.c:414-478`).

## Divergence classes

### C1 — In-process `${THIS_SH}` child loses `__RUBASH_ASSOC_VARS` → `${!a[@]}` returns `0 1`

- Tests: `assoc1.sub:20,28` (`echo ${!BASH_CMDS[@]}`), `assoc2.sub:19,27`
  (`echo ${!BASH_ALIASES[@]}`).
- Ledger: gnu.out:77 `foo qux` / rb.out:77 `0 1`; same at 87.
- Repro: `.tmpwork/audit/assoc/p1g-child.sh` run via `p1g-parent.sh`
  (`${THIS_SH} ./p1g-child.sh`). Parented run prints `M0:` (empty),
  `B:0 1`; direct run prints `M0:BASH_CMDS$BASH_ALIASES`, `B:qux foo`.
- GNU cite: `variables.c:1895-1897` — a fresh shell unconditionally
  creates `BASH_CMDS`/`BASH_ALIASES` as `att_assoc` dynamic vars; the
  attribute lives on the `SHELL_VAR`, not in the environment.
- RB owner: `src/executor/external_finish.rs:45-119
  execute_same_shell_script` → `execute_direct_shell_script:158-167`
  replaces `self.env_vars` with `child_shell_environment()`
  (`external_finish.rs:260-294`), which carries **only**
  `EXPORTED_VARS`-marked names plus `PWD`/`OLDPWD`/`SHELL` and
  re-inserts `EXPORTED_VARS` — `__RUBASH_ASSOC_VARS`,
  `__RUBASH_ARRAY_VARS`, `__RUBASH_READONLY_VARS`, etc. are all dropped,
  and `Executor::new`'s init marks (`src/executor/init.rs:194-197`
  `mark_env_name(ASSOC_VARS, "BASH_CMDS")`) are never re-run for the
  in-process child. `parameter_array_storage` still materializes
  assoc-shaped storage (`dynamic_arrays.rs:169-170` →
  `bash_cmds_storage`), but `is_marked_var(ASSOC_VARS)` fails, so
  `${!a[@]}` takes the indexed `array_indices` branch at
  `src/executor/arrays/executor.rs:305-315` and prints element indices.
- Verdict: **Rubash semantic bug.** The env-string variable model
  cannot carry attributes into a "fresh shell" child; the child env
  must re-derive marks (as `Executor::new` does) or store attributes
  per-variable.
- Severity: **High** — every `${THIS_SH}` child in every suite loses
  assoc/array/readonly attributes; here it turns key enumeration into
  `0 1`.

### C2 — Dynamic assoc enumeration order: wrong bucket count / wrong seed order

- Tests: `assoc1.sub:21,26,29` (`echo ${BASH_CMDS[@]}`, `hash`),
  `assoc2.sub:20,28` (`recho ${BASH_ALIASES[@]}`).
- Ledger: gnu.out:84-85 `foo sh blat qux` / rb.out:84-85
  `sh qux foo blat`; gnu.out:95-99 `blat foo sh qux` … /
  rb.out:95-99 `sh qux foo blat` ….
- Repro: `.tmpwork/audit/assoc/p1-bashcmds.sh`, `p2-bashaliases.sh`
  (direct run reproduces the ordering diff even with marks intact).
- GNU cite: `variables.c:1692` `build_hashcmd` —
  `assoc_create(hashed_filenames->nbuckets)` (256) then bucket-order
  copy; `variables.c:1762` `build_aliasvar` — `assoc_create
  (aliases->nbuckets)` (64). Enumeration order therefore comes from the
  **source table's** bucket count, not 1024. `hash` listing:
  `builtins/hash.def:261-268 print_hashed_commands` → `hash_walk`
  (bucket order) printing `item->times_found` (set to the `found`
  argument, 0 for `hash -p`, `hashcmd.c:91-117`).
- RB owner: `src/executor/dynamic_arrays.rs:222-224
  bash_cmds_storage` formats `builtins::hash::hashed_entries`
  (name-sorted, `src/builtins/hash.rs:274-278`) into storage;
  enumeration then rehashes at the hard-coded 1024 in
  `bash_assoc_order` (`src/executor/assignment_helpers.rs:398-435`),
  which is only correct for `declare -A` tables. `bash_aliases_storage`
  (`dynamic_arrays.rs:212-219`) sorts alphabetically first — also wrong
  seed order. `hash` listing: `src/builtins/hash.rs:196-206` sorts by
  **path** and prints a hardcoded hit count (`1`, or `3` for `bash`)
  instead of `times_found`.
- Verdict: **Rubash semantic bug** (two layers: storage construction
  order and the fixed-1024 enumeration model; plus fabricated hit
  counts). The `bash_assoc_order` model itself is a correct port for
  1024-bucket tables — verified numerically: FNV-1("0".."3") land in
  buckets 300-303 → GNU `[3] [2] [1] [0]` order — but dynamic vars need
  source-table bucket counts (256/64).
- Severity: **Medium** — deterministic-order visible in every
  `${!BASH_CMDS[@]}`/`${BASH_CMDS[@]}`/`hash`/`alias -p` output.

### C3 — `${assoc[@]:off}` / `${assoc[*]:off}` slicing ignores assoc hash order

- Tests: `assoc4.sub:22-23` (`recho "${i[*]:0}"`, `recho "${i[@]:0}"`).
- Ledger: gnu.out:108-112 `</barq//fooq>`, `<>` `<barq>` `<>` `<fooq>`
  vs rb.out `</fooq//barq/>`, `<fooq>` `<>` `<barq>` `<>`.
- Repro: `.tmpwork/audit/assoc/p13-order.sh` — reproduces **directly**
  (not dependent on THIS_SH); `declare -p i` and `"${i[@]}"` are
  correct, so `i` is assoc-marked and `bash_assoc_order` is right.
- GNU cite: `subst.c` array-slice path → `assoc_to_word_list`
  (`assoc.c:501`) order, then positional slice.
- RB owner: `src/executor/arrays.rs:798-830 array_parameter_slice`
  unconditionally calls `indexed_array_entries` (insertion order) and
  filters by numeric index ≥ offset; invoked from
  `src/executor/arrays/executor.rs:216-233` (unquoted) and `253-267`
  (quoted) without an `ASSOC_VARS` check. For an assoc var, "index" is
  meaningless — GNU slices the hash-ordered word list by position.
- Verdict: **Rubash semantic bug.**
- Severity: **Medium** — all `[@]`/`[*]` slices of assoc arrays return
  insertion order.

### C4 — Subscript boundary/expansion doesn't match `skipsubscript` + `expand_subscript_string`

- Tests: `assoc5.sub:21-40` — `myarray[$(echo ])]=def`,
  `${myarray[']']}`, `${myarray[\]]}`, `myarray[$bar]=123`,
  `${myarray['a]=test1;#a']}`, `myarray['a]=test2;#a']="def"`;
  `assoc11.sub:36-38,44-47,59-63` — `foo=('a]a' abc ']' def
  $(echo 'foo[bar') bleh \; semicolon a=b assignment)`,
  `foo=('`' backquote '"' dquote "'" squote \\ bslash)`,
  `dict=( '"' dquote '`' bquote "'" squote '\' bslash)`;
  `assoc17.sub` matched (plain `A[']']`/`A[\]]` work — the gap is the
  *expansion-containing* and quote-corner cases).
- Ledger: gnu.out:134-140 vs rb.out:134-140 (missing `]` entry,
  `bleh abc` vs `def bleh abc`, mangled `set` line, empty lookups);
  gnu.out:234-237,242-243 vs rb.out:174-177,182-183 (keys merged into
  one giant `"foo[bar bleh ; semicolon a=b assignment"` /
  `"\" dquote ' squote \\ bslash"` key).
- Repro: `.tmpwork/audit/assoc/p15-assoc5.sh` —
  `myarray[$(echo ])]=def` → RB `myarray[]]]: bad array subscript`
  (stderr), lookup of `]` key empty; GNU stores/reads `]`→`def`.
- GNU cite: `subst.c:2086 skip_matched_pair` — the `]` inside
  `$(...)`/`${...}`/backquote is skipped as a unit (lines 2152-2170) and
  quoted spans are skipped via `skip_single_quoted`/`skip_double_quoted`
  (2146-2151); `arrayfunc.c:817` expands the raw subscript once with
  `expand_subscript_string`.
- RB owner: `src/parser/array_element_assignment.rs:154-184
  matching_subscript_end` handles `\`, `'`, `"`, nested `[`/`]` but
  **not** `$(...)`, `${...}` or backquotes — a `]` inside `$()` closes
  the subscript early. Companion paths with the same class of gap:
  `src/executor/arithmetic/mod.rs:785-822 assoc_subscript_end` (handles
  substitutions but is only used in arithmetic), the compound-element
  splitter `src/executor/assignment_expansion.rs` `split_compound_element_words`
  (which merges `['a]a' abc` + `']' def` … into single keys instead of
  keeping GNU's word list), and `assoc_token_scan_state` /
  `merge_assoc_subscript_tokens`
  (`src/builtins/declare/storage/assoc.rs:197-252`) which re-glues
  whitespace-split tokens under an unclosed-`[` heuristic.
- Verdict: **Rubash semantic bug.** This is the whack-a-mole shape
  flagged by rubash#117: three parallel hand-rolled subscript lexers,
  each with its own quote/substitution coverage. The fix invariant is
  to converge them on `skip_matched_pair` semantics, not to add more
  quote cases per symptom.
- Severity: **High** — silent data corruption (wrong keys stored) plus
  missing diagnostics and lost array elements.

### C5 — `set` prints assoc vars as single-quoted scalars

- Tests: `assoc5.sub:34,40` (`set | grep ^myarray=`).
- Ledger: gnu.out:137 `myarray=(["]"]="def" [foo]="bleh" ...)` vs
  rb.out:137 `myarray='(["a]a"]=abc [foo]=bleh ["a]=test1;#a"]=123)'`.
- GNU cite: `builtins/set.def:506 print_all_shell_variables` →
  `variables.c:1096 print_assignment` → `assoc_p(var)` →
  `print_assoc_assignment` (`arrayfunc.c`, `name=(["k"]="v" ...)` form).
- RB owner: `src/builtins/set.rs:198-225 print_shell_variables` — every
  variable goes through scalar `shell_quote`, so the assoc storage
  string is wrapped in `'…'`.
- Verdict: **Rubash semantic bug** (display-only; the missing `]`
  element in the same line is C4).
- Severity: **Low** — cosmetic but byte-visible.

### C6 — Element-assign subscript `\'` → fatal `arithmetic syntax error`

- Tests: `assoc6.sub` — `foo[bar\'bie]="doll"` (file line ~64; Rubash
  reports `line 54`, an offset artifact) aborts the whole subfile: all
  `bar'bie`/`bar$bie`/`bar[bie`/`bar\`bie`/`bar\]bie`/`bar${foo}bie`
  blocks (gnu.out:156-185) produce no Rubash output.
- Repro: `.tmpwork/audit/assoc/p16.sh` — `foo["bar'bie"]="doll"` works,
  `foo[bar\'bie]="doll"` → `arithmetic syntax error: operand expected`
  and the script dies. `foo=([bar\'bie]=doll)` (compound form) works —
  `p14-assoc6.sh`.
- GNU cite: `arrayfunc.c:1169/1601` — assoc element subscripts go
  through `expand_subscript_string` (word expansion), never the
  arithmetic evaluator; `\'` is a quoted literal `'`.
- RB owner: the element-assignment subscript path —
  `src/parser/array_element_assignment.rs:85-135
  array_element_assignment_from_word` plus the arithmetic subscript
  machinery `src/executor/arithmetic/mod.rs:330-392
  expand_arithmetic_assoc_subscripts`/`expand_assoc_subscript_once` and
  the bare-quote check `has_bare_single_quote` (~`:198`): the escaped
  `'` survives into a path that rejects it as an arithmetic operand.
  Second bug layered on top: the error is **fatal** (aborts the rest of
  the file); GNU reports subscript errors per command and continues.
- Verdict: **Rubash semantic bug** (wrong diagnostic + wrong fatality).
- Severity: **High** — one bad subscript kills an entire script.

### C7 — `b+=([\`]= [\]]=)` inside `typeset -A` file → `syntax error: unexpected EOF`, whole file dead

- Tests: `assoc9.sub:14-16` — `typeset -A a=( [\\]= [\"]= [\)]= ) b`
  parses, then `b+=([\`]= [\]]=)` fails.
- Ledger: gnu.out:195-221 (all assoc9 output) absent from rb.out.
- Repro: `.tmpwork/audit/assoc/p6.sh` (byte-faithful, `\\` preserved) —
  GNU prints `declare -A a=([")"]="" ["\""]="" ["\\"]="" )`,
  `declare -A b=(["]"]="" ["\`"]="" )`, `SURVIVED`; Rubash prints the
  `a` line then `syntax error: unexpected EOF while looking for
  matching `)'`.
- GNU cite: `parse.y` word assembly (`read_token_word`, parse.y:5305+)
  treats `` \` `` and `\]` inside a compound-assignment word as escaped
  literals; `arrayfunc.c:758 skipsubscript` then finds each `[`/`]`
  boundary correctly.
- RB owner: parser compound-assignment/append handling — the `\`` and
  `\]` escapes inside `b+=(...)` leave the `)`-match unbalanced
  (`src/parser/` compound-assign token path feeding
  `array_element_assignment_from_word`). Because the file-level parse
  fails, **all** of assoc9's later tests (dict load/unset loops,
  `read`/`printf -v` subscripts, `assoc_expand_once`, `a[$x]` with
  `x='$(date >&2)'`, `assoc['$var']`, `foo["foo]bar"]`) produce no
  output — this single parse bug accounts for ~27 deleted stdout lines.
- Verdict: **Rubash semantic bug** (parser).
- Severity: **High** — largest single-hunk loss in the suite.

### C8 — `${a[@]@K}` loses quoting on boundary elements (unquoted context)

- Tests: `assoc11.sub:28,32,47,76` — `echo foo=\( echo ${foo[@]@K} \)`,
  `echo ${a[@]@K}`.
- Ledger: gnu.out:233 `foo=( echo "\\" "5" ... "2" )` vs rb.out:173
  `foo=( echo \\" "5" ... "2 )`; gnu.out:248 `")" "rparen" ... "\\" "bs"`
  vs rb.out:188 `)" "rparen" ... "\\" "bs`.
- Byte-level: the **first** element's leading `"` and the **last**
  element's trailing `"` are dropped by Rubash; interior elements are
  correctly quoted. Looks like a word-level quote-strip applied to the
  assembled `@K` string instead of emitting `assoc_to_kvpair`'s literal
  output.
- GNU cite: `subst.c:8854` `array_transform('K')` →
  `assoc_to_kvpair` (`assoc.c:346-411`) → then `quote_escapes` on the
  whole result for unquoted context (`subst.c:8705`); every element's
  quoting is data, not re-interpreted.
- RB owner: `src/executor/arrays/executor.rs:376-378` →
  `parameter_key_value_transform` (`src/executor/parameter_transforms.rs:250-301`)
  → `format_key_value_transform_part` /
  `quote_key_value_transform_key`
  (`src/executor/parameter_replace.rs:466-528`) — the pieces exist and
  quote correctly per element; the loss happens when the joined string
  is re-lexed/split on return (unquoted-expansion path). Do **not** add
  a boundary-quote special case — find where the assembled result loses
  its outer quoting (rubash#117).
- Verdict: **Rubash semantic bug.**
- Severity: **Medium** — `@K` is meant to be `eval`-round-trippable;
  losing boundary quoting breaks `eval "a=( ${a[@]@K} )"` whenever the
  first/last element needs quotes.

### C9 — `readonly -A name=( k v ... )` routed to indexed-array storage

- Tests: `assoc11.sub:86-89` — `readonly -A foo=( one 1 two 2 three 3 )`
  then `export foo`.
- Ledger: gnu.out:252 `declare -Arx foo=([two]="2" [three]="3" [one]="1" )`
  vs rb.out:192 `declare -arx foo=([0]="one" [1]="1" [2]="two" [3]="2"
  [4]="three" [5]="3")` — lowercase `-a` (indexed) flag and alternating
  words stored as index/value.
- GNU cite: `builtins/setattr.def` → `declare.def` shared
  `declare_internal` → `arrayfunc.c:700 assign_compound_array_list` →
  kv-pair path (`assoc_p` is set by `-A` before the compound assign).
- RB owner: `src/builtins/setattr.rs` `readonly_with_io` /
  `apply_readonly_arg` — `-A` marks the attr but the compound value is
  stored through the **indexed** path; it never reaches
  `builtins/declare/storage/assoc.rs append_assoc_value` /
  `parse_assoc_words`.
- Verdict: **Rubash semantic bug.**
- Severity: **Medium** — `readonly -A` silently produces the wrong
  variable type.

### C10 — kv-pair compound assignment field-splits expanded words

- Tests: `assoc12.sub:5-61` — `declare -A v1=( $foo 3 )`,
  `v2=( [$foo]=3 )`, `v3=( $foo 3 )`, `v1=( $foo $bar )`,
  `v1+=( $xtra xtra )`, `v3+=( '$xtra' xtra )`, `v1+=( [$xtra]='new xtra' )`
  with `foo='1 2'`, `bar='3 4 5'`, `xtra='20 40 80'`; also
  `assoc11.sub:34` (`foo=( "" null )` → GNU `"": bad array subscript`,
  Rubash stores `[]="null"`).
- Ledger: gnu.out:253-270 `declare -A v1=(["1 2"]="3" )`,
  `v1=(["1 2"]="3 4 5" )`, `v1=(["20 40 80"]="xtra" ...)` etc. vs
  rb.out:193-210 `v1=([3]="" [1]="2" )`, `v1=([5]="" [3]="4" [1]="2" )`,
  `v1=([80]="xtra" ... [20]="40" )` etc.
- Repro: `.tmpwork/audit/assoc/p3-kvpair-nosplit.sh`.
- GNU cite: `arrayfunc.c:581` `parse_string_to_word_list` produces the
  raw word list (`$foo`, `3`); `arrayfunc.c:594-599` skips
  `expand_words_no_vars` for assoc vars; `arrayfunc.c:671
  kvpair_assignment_p` selects the kv path (first word not
  `W_ASSIGNMENT`/`[…]`); `arrayfunc.c:630-666
  assign_assoc_from_kvlist` expands each pair word **whole** via
  `expand_subscript_string`/`expand_assignment_string_to_string` — `$foo`
  expands to the single key `1 2`, never field-split. Empty keys are
  rejected: `arrayfunc.c:645-649 err_badarraysub`.
- RB owner: `src/builtins/declare/storage/assoc.rs:15-31
  parse_assoc_words` and `:79-104 append_assoc_value` pair up
  `tokens.chunks(2)` — but the tokens arrive already expanded **and
  field-split** (`$foo`→`1`,`2`), so pairs shift: `(1,2),(3,"")`. The
  same splitter drops the empty-key error (`""` pairs to `[]="null"`).
  The expansion+split lives upstream in the compound-word builder
  (`src/executor/assignment_expansion.rs` `split_compound_element_words`
  / `expand_tilde_in_compound_assignment` call sites, `:367-380` and
  `:503-514`).
- Verdict: **Rubash semantic bug.** Invariant to fix: assoc kv-pair
  words must be carried unexpanded/unsplit to the pairing layer and
  expanded once per word — not patched by another splitting guard.
- Severity: **High** — wrong keys/values stored for the common
  `a=( $x y )` idiom.

### C11 — `"${assoc[*]@k}"` doesn't join into one word

- Tests: `assoc14.sub:9` (`recho "${assoc[*]@k}"`).
- Ledger: gnu.out:293 `argv[1] = <hello world key with spaces value
  with spaces foo bar one 1>` vs rb.out:233-240 (8 separate argv).
- Repro: `.tmpwork/audit/assoc/p12-atk.sh`.
- GNU cite: `subst.c:8871-8884` — `@k` produces `assoc_to_kvpair_list`
  then `string_list_pos_params(itype='*')` → joined single word when
  quoted.
- RB owner: `src/executor/arrays/executor.rs:379-380` —
  `ParameterTransform::KeyValueSplit` → `array_key_value_split_transform_values`
  (`:473-489`) ignores the `starred` flag parsed at `:365-368`; the `*`
  join at `:398-402` is only applied to non-kv transforms.
- Verdict: **Rubash semantic bug.**
- Severity: **Low-Medium** — one-word join semantics wrong only for
  `[*]@k`/`[*]@K` forms.

### C12 — `$(( ${A['literal-key']} ))` resolves to 0 (quoted subscript lost inside arithmetic)

- Tests: `assoc16.sub:41` — `echo $(( ${A['$(echo Darwin ; echo stderr>&2)']} ))`
  where the key is literal.
- Ledger: gnu.out:362-363 `42`/`42` vs rb.out:309-310 `42`/`0`.
- Repro: `.tmpwork/audit/assoc/p11-arith-quoted-key.sh` — the unquoted
  `$(...)` key works (`42`); the single-quoted key gives `0`; plain
  `${A['key']}` outside arithmetic is fine (`darjeeling`).
- GNU cite: the `$(( ))` body is treated as double-quoted
  (`subst.c`/expr.c), and `${A['…']}` is an ordinary braced expansion
  inside it — `subst.c:9777 parameter_brace_expand` →
  `expand_subscript_string` handles the quotes.
- RB owner: `src/executor/arithmetic/mod.rs` —
  `expand_arithmetic_assoc_subscripts` (`:330-370`) skips `$(`/`${`
  regions wholesale (`:331-338`), so the `${A['…']}` braced expansion
  reaches `expand_arithmetic_expression_mut`; meanwhile
  `expand_arithmetic_special_parameters` (`:394-411`, specifically the
  `'`→`\x17` blanket replace at `:409`) corrupts the single-quoted
  subscript before the embedded-parameter walker sees it — the key
  lookup misses → empty → `0`.
- Verdict: **Rubash semantic bug.**
- Severity: **Medium** — arithmetic use of quoted assoc subscripts.

### C13 — `wait -p name[sub] -n %jobspec` unsupported: jobspecs rejected

- Tests: `assoc18.sub:53` — `wait -p A[$rkey] -n %2 %3` under
  `shopt -s assoc_expand_once`.
- Ledger: gnu.out:378 `5: ok 1` vs rb.out:325 `bad 1`; rb.err
  `wait: %2: no such job`, `wait: %3: no such job`.
- Repro: `.tmpwork/audit/assoc/p10-waitp.sh` (needs the `shopt`; GNU
  without it fails `wait: 'A[]]': not a valid identifier` too).
  `wait %1`/`wait -n %2 %3` alone work in Rubash — only the `-p`/`-n`
  combination fails.
- GNU cite: `builtins/wait.def:126` `internal_getopt(list, "fnp:")` —
  `-p` name validated by `valid_identifier || valid_array_reference`
  (line 157); `-n` with a jobspec list goes `set_waitlist` (line 232) +
  `wait_for_any_job` (238) + `builtin_bind_var_to_int(vname, pstat.pid,
  bindflags)` (240) — array-element lvalue binding included.
- RB owner: `src/executor/job_builtins.rs:1365-1409 wait_any_request` —
  `-p`'s name is gated on `is_shell_name` (`:1390`), which rejects
  `A[]]`; `wait_background_operands` (`:1411-1434`) returns `None` for
  any `-n`/`-p` option (`:1426`); `execute_wait` (`:122-189`) then falls
  through to `src/builtins/wait.rs` `execute_with_io`, which has no
  job-table access and emits `no such job` for `%N`.
- Verdict: **Rubash semantic bug** (missing feature: array-element
  `-p` lvalue + `-n` operand list).
- Severity: **Low** — narrow combination; one suite line.

### C14 — Tilde expansion in assoc keys and `a[k]=~/v` values

- Tests: `assoc19.sub:31-47` — `declare -A aa=([~/key]=~/Desktop)`,
  `aa[~/Documents]=~/Library`, `aa=([~/key]=… [~/Documents]=~/Library)`.
- Ledger: gnu.out:382-385 `[/homes/cj/key]`, `[/homes/cj/Documents]`
  (both key and value expanded) vs rb.out:329-332 `["~/key"]`,
  `[/homes/cj/Documents]="~/Library"`, `["~/Documents"]`.
- Repro: `.tmpwork/audit/assoc/p8-tilde.sh`.
- GNU cite: `subst.c:4357 expand_string_assignment` / `internal_tilde`
  — subscript words and element values in assignment context both get
  the tilde pass; `expand_subscript_string` on `[~/key]` expands `~`.
- RB owner: `src/executor/assignment_expansion.rs:382-402
  expand_compound_element_tilde` — splits at `find("]=")` (which also
  breaks on keys containing `]=`) and tilde-expands only the value part;
  the `[~/key]` prefix is never touched. Element assigns
  (`aa[~/Documents]=~/Library`) show the reverse: key expanded, value
  not — the single-element path skips
  `expand_assignment_tilde_if_needed` (`:847-856`).
- Verdict: **Rubash semantic bug.**
- Severity: **Medium** — real semantic difference in `~`-key assoc
  assignments.

### C15 — stderr-only: missing/incorrect identifier validation for assoc-element operands

- Tests: `assoc5.sub:26` (`declare myarray["foo[bar"]=bleh` → GNU
  `declare: 'myarray[foo[bar]=bleh': not a valid identifier`, Rubash
  silently accepts); `assoc9.sub:84,96,154` (`read a[$b]`,
  `printf -v a[$b]`, `typeset foo["foo]bar"]=bax` → GNU `not a valid
  identifier`, Rubash accepts — but unreachable anyway under C7);
  `assoc11.sub:34` (`foo=( "" null )` → GNU `"": bad array subscript`,
  Rubash stores `[]="null"` — see C10).
- GNU cite: `builtins/declare.def:593-602` — a `[` in the word requires
  `valid_array_reference(name,0)`; `builtins/common.c:956-966` —
  `builtin_bind_variable`/`assign_array_element` gate for
  read/printf -v; `arrayfunc.c:645` `err_badarraysub` for empty keys.
- RB owner: `src/builtins/declare/assign.rs:352-356` (only validates
  `]`-before-`=` shape) and the read/`printf -v` bind path
  (`src/executor/read_builtin.rs`, `src/builtins/printf.rs`) — no
  `valid_array_reference` equivalent.
- Verdict: **Rubash semantic bug** (validation gap; affects diagnostics
  and silently-accepted bad stores).
- Severity: **Low** — stderr-only in this suite, but same invariant.

## Environment / WinuxCmd artifacts

- None of the stdout hunks are attributable to WinuxCmd: the baseline
  ran Rubash natively and all classes above reproduce with
  `target/debug/rubash.exe` directly against WSL GNU 5.3.0.
- The 4 extra `stderr` lines in `rb.err` (13 vs 9) come from Rubash
  evaluating `$(… >&2)` subscripts more times than GNU (C4/C12 family —
  subscript expansion count), not from the environment.
- `hash` hit counts (`0` vs `1`) are a Rubash implementation choice in
  `builtins/hash.rs:203`, not environment-dependent.

## Overlap / conflict notes

- `src/executor/assignment_helpers.rs` already contains a correct
  FNV-1/bucket-order port (`bash_hash_string`, `bash_assoc_order`,
  `:375-435`) — it models 1024 buckets unconditionally; C2 needs the
  source-table bucket count for `BASH_CMDS`/`BASH_ALIASES`.
- Three separate subscript-boundary lexers exist
  (`parser/array_element_assignment.rs:154`,
  `executor/arithmetic/mod.rs:785`, and the storage merger in
  `builtins/declare/storage/assoc.rs:197`). C4/C6/C7 all flow through
  them; per rubash#117 these should converge on `skip_matched_pair`
  semantics rather than accumulate per-character guards.
- If another agent is actively changing the assoc family, the highest-
  risk files to coordinate on are `src/executor/assignment_expansion.rs`
  (compound element splitting/tilde), `src/builtins/declare/storage/
  assoc.rs` (kv pairing + token merging), `src/executor/arrays/
  executor.rs` (transforms/slices), and
  `src/executor/external_finish.rs` (THIS_SH child env).

## Reproducer inventory

`.tmpwork/audit/assoc/`:

| file | class |
| --- | --- |
| `p1-bashcmds.sh` | C1/C2 |
| `p2-bashaliases.sh` | C1/C2 |
| `p1a-child.sh`, `p1c.sh`, `p1d-*`, `p1e-*`, `p1f-parent.sh`, `p1g-*` | C1 bisection |
| `p3-kvpair-nosplit.sh`, `p9-kv.sh` | C10 |
| `p4-star-slice.sh`, `p13-order.sh`, `p13b.sh` | C3 |
| `p5-quote-keys.sh`, `p5a-d.sh`, `p15-assoc5.sh` | C4 |
| `p6*.sh` | C7 |
| `p7-readonly.sh` | C9 |
| `p8-tilde.sh` | C14 |
| `p10-waitp.sh`, `p10b/c/d/e.sh` | C13 |
| `p11-arith-quoted-key.sh` | C12 |
| `p12-atk.sh` | C8/C11 |
| `p14-assoc6.sh`, `p16.sh`, `p16p.sh` | C6 |
| `p17.sh` | C10/C15 (empty key + identifier validation) |
