# expansion-cluster compatibility audit — 2026-09-18

Read-only audit of the twelve GNU Bash expansion-related suites against
Rubash at code baseline `2494bc57` (code-identical to `c282a850`), per
`docs/audit-baseline-2026-09-18.md`. **No `src/` files were modified.**

Suites: `comsub` (17), `comsub2` (48), `comsub-posix` (11), `new-exp`
(51), `exp` (6), `more-exp` (6), `posixexp` (7), `procsub` (11),
`extglob` (32), `cond` (19), `nquote` (12), `iquote` (76) — counts are
stdout diff lines from the baseline ledger; stderr is tracked separately
and called out where it is the only divergence channel.

## Method

- Oracle: owner-compiled GNU Bash 5.3.0 at WSL `/usr/local/bin/bash`,
  invoked from a **script file**
  (`MSYS_NO_PATHCONV=1 wsl /usr/local/bin/bash /mnt/d/repo/rubash/<file>.sh`),
  never `bash -c` (wsl.exe arg passthrough collapses `\\`→`\` and corrupts
  quoting baselines).
- Rubash: `target/debug/rubash.exe <file>.sh`.
- Suite artifacts: `target/issue-suites/results/true-baseline/<suite>/`
  and `target/issue-suites/results/bash-tests-rw/`.
- Minimal reproducers: `.tmpwork/audit/exp/v*.sh`.
- GNU C source in `third_party/bash/` is the specification; every class
  cites the owning C function.
- rubash#117 lens: every command-substitution divergence is classified as
  word-level text shortcut vs parser-backed execution, with the shortcut
  site named.

## Shortcut site map (rubash#117)

Word-level text shortcuts that bypass the real parser:

| Site | File:line | What it does |
| --- | --- | --- |
| `split_shell_words_with_quote_info` + `expand_aliases(&words)` | `src/executor/command_substitution.rs:243-246` | Splits comsub body text into words, splices alias text back in, rejoins — replaces GNU's parse-time alias expansion |
| `comsub_body_alias_splice` | `src/executor/arithmetic_aliases.rs:457-521` | Rewrites comsub body *text* for `let`/`declare`/math aliases instead of parsing |
| `command_substitution_has_unclosed_compound` gate | `src/executor/command_substitution.rs:260-263` | Decides whether body is handed to the parser vs treated as fast-path text |
| `RE_BLOCKED_SINGLE_WORD_BODIES` blacklist | `src/executor/command_text.rs:41` | Blacklist admission for the single-word substitution fast path |
| `RE_SPLIT_WORD_SAFE_BODIES` | `src/executor/command_text.rs:47` | Blacklist admission for `split_shell_words` bodies |
| `try_parse_shell_words` / `try_extract_command_substitutions` | `src/executor/command_text.rs:422-427` | Text-level word splitting for fast paths |
| `has_unclosed_command_substitution` | `src/lexer/continuation.rs:788` | Line-completeness pre-scan that decides where `$(` closes — *before* the parser sees the body |
| `skip_parenthesized_unit` | `src/lexer/continuation.rs:659-743` | The actual `)`-matching inside the pre-scan (no `#`-comment handling at top level) |
| `skip_cmd_subst` / `update_command_substitution_case_depth` | `src/lexer/skip.rs:326-368,630` | A second, parallel `)`-matcher used by other lexer paths |
| `skip_heredoc_in_chars_with_closure` | `src/lexer/heredoc_scan.rs:1` | Heredoc body skip with `)`-closure coupling |
| `remove_residual_shell_quotes` | `src/builtins/echo.rs:106-108` | Post-expansion quote stripping inside `echo` (workaround for incomplete re-reading upstream) |
| hardcoded comsub-body canned strings | `src/executor/command_substitution.rs:200-206` | Returns literal `"4"`, `"bar() { echo $(< x1); }"`, `"./e"` for three specific body texts (`set`-option probe, `declare -f foo \| sed`, `type -p e`) |

GNU has **no** word-level substitution shortcuts; the only sanctioned
short-circuit is `parse_string_to_command`'s empty-command check
(`parse.y`). Every comsub body goes through `command_substitute`
(`subst.c:7143`) → `parse_and_execute`, with the body text produced by
`parse_comsub` (`parse.y:4451`) / `parse_matched_pair` (`parse.y:3877`).

## Divergence classes

## comsub (17) — CS classes

### CS-1 — `#` comment inside `$(...)` not recognized by the line-completeness pre-scan → premature close

- Tests: `comsub1.sub:36-37` (`echo $(#comment )`, `echo a)`),
  `comsub.tests:18` region, `comsub2.sub:46`, `comsub3.sub:10`,
  `comsub4.sub:6`, `comsub5.sub:27`, `comsub6.sub:40-42`,
  `comsub7.sub:24`, `comsub9.sub:7`, `comsub11.sub:24-27`,
  `comsub-eof[23].sub` header.
- Repro `v27.sh`:
  ```sh
  echo $(#comment )
  echo a)
  echo done
  ```
  GNU: `` / `a)` / `done`. RB: `` / `a)` / `done` — **but** inside a
  larger script (`comsub-posix.tests:30-39` style) the missing `#`
  handling cascades into `unexpected EOF` aborts when the comment
  contains quotes/backslashes (see CSP-1).
- GNU cite: `parse.y:3877 parse_matched_pair()` honors `LEX_CKCOMMENT`
  so `#` starts a comment inside the matched pair; `parse.y:4451
  parse_comsub()` re-enters `parse_command` so the body is a real parse.
- RB owner: `src/lexer/continuation.rs:659 skip_parenthesized_unit` has
  **no `#` handling** — the comment-skip code at
  `continuation.rs:920-936` lives in the `depth>0` fallback of
  `has_unclosed_command_substitution` and is only reached when the fast
  `skip_parenthesized_unit` path already failed. `#` at top level of a
  comsub is therefore treated as ordinary text; a `)` inside the comment
  closes the substitution early.
- rubash#117: this is a *scanner* shortcut (deciding where `$(` ends
  before the parser sees it), not the parser itself. Whitelist-only
  admission does not help — the scanner must be correct for every input;
  the fix is `#`-comment handling in `skip_parenthesized_unit` (or
  deferring the close decision to the parser).
- Verdict: **Rubash bug**. Severity: **High** (cascades to script
  aborts in comsub-posix).

### CS-2 — backquote-comsub body used as `${x//pat/}` pattern keeps backslashes

- Tests: `comsub.tests:35-40` — `v="$(v="$(pwd)"; echo ${v//tmp/foo/bar})"`
  family (`${v/\/tmp\/foo\/bar/...}`). GNU: `/foo/`; RB: `\/tmp\/foo\/bar`
  (literal escaped pattern emitted).
- GNU cite: `subst.c:11598-11605` — for backquote substitutions GNU runs
  `de_backslash`/`r` before `command_substitute`; combined with
  `string_extract_verbatim` (`subst.c:1148`) the `\` escapes are consumed
  at pattern-build time, so the replacement word is `tmp/foo/bar` — the
  literal-slashes form only appears when the escape survived to the
  glob engine.
- RB owner: the `${x//pat/repl}` pattern path treats the
  backquote-substituted replacement as literal text — the `\` are never
  de-escaped on the replacement-word path
  (`src/executor/parameter_replace.rs` / braced-replacement expansion).
- rubash#117: parser-adjacent (quote-removal stage), not a word-level
  fast path; whitelist/deferred-parse would not change it.
- Verdict: **Rubash bug**. Severity: **Medium**.

### CS-3 — alias expansion inside `$(...)` is a text splice, not parse-time expansion

- Tests: `comsub5.sub` (`switch=case`), `comsub6.sub`
  (`alias comsub0='echo $(cat '`, `math0`, `x` aliases),
  `comsub7.sub` (`number`, `my_alias`, `v`, `let --`, `eval` aliases).
- Diff signature: GNU prints `ok 1`..`ok 8`, `Mon Aug 29`/`after`, `7`,
  `hey after x`; RB prints only `ok 1` and loses the rest.
- GNU cite: `parse.y:4529` — inside `parse_comsub`, when
  `shell_is_posix`/`expaliases_flag` is set GNU runs
  `expand_aliases` **during the parse** so alias text becomes real
  tokens; `subst.c:7143 command_substitute` → `parse_and_execute`.
- RB owner: `src/executor/arithmetic_aliases.rs:457 comsub_body_alias_splice`
  rewrites the *body string* (e.g. `let --` splice at :493-516), and
  `command_substitution.rs:246` calls `expand_aliases(&words)` on the
  `split_shell_words_with_quote_info` output — both operate on text after
  the body has already been chopped into words, so aliases that produce
  unclosed `$(`, `case`, or multi-command text break.
- rubash#117: **canonical word-level shortcut**. Whitelist-only
  admission (body must contain no alias names before splicing) or
  deferred `parse_and_execute` would make these cases correct.
- Verdict: **Rubash bug**. Severity: **High**.

### CS-4 — `VAR=val cmd` inside `$(...)` → spurious `VAR: command not found` on stderr

- Tests: `new-exp8.sub:3-9` (`$(IFS=: ...)` family), `comsub.tests:33`.
- Repro `v48.sh`:
  ```sh
  FOO=$( IFS=: ; echo $XPATH )
  echo ok
  ```
  GNU stderr: *(empty)*. RB stderr: `IFS: command not found` (stdout
  still correct — `ok`).
- GNU cite: `subst.c:7143 command_substitute` → `parse_and_execute` —
  `IFS=:` is a temporary assignment prefix parsed by the grammar, never
  dispatched as a command.
- RB owner: `src/executor/command_substitution.rs:243-246` —
  `split_shell_words_with_quote_info` produces `IFS=:` as a word, and
  one of the fallback exec paths dispatches it as a command name while
  the parser-backed path produces the right output — dual-channel
  artifact of the word-level path racing the real executor.
- rubash#117: word-level shortcut; invisible on stdout but pollutes
  stderr. Whitelist admission (`IFS=:` is not a provably-trivial body →
  fall to parser) removes it.
- Verdict: **Rubash bug** (stderr-only). Severity: **Medium**.

### CS-5 — comsub parse error is fatal; GNU continues

- Repro `v45.sh`:
  ```sh
  foo=$( {xxx}</dev/stdin)
  echo "x$foo"
  echo after
  ```
  GNU: `x` + `after` (+ `command not found` on stderr). RB:
  `syntax error in command substitution` then script aborts — `after`
  never prints.
- GNU cite: `subst.c:7143` — a body that fails to parse produces an
  error status and empty output; the *outer* script continues.
- RB owner: `src/executor/command_substitution.rs:260-263` —
  `command_substitution_has_unclosed_compound` and the error propagation
  out of `expand_command_substitution_inner` turn an inner parse error
  into a top-level abort.
- rubash#117: admission-gate (`has_unclosed_compound`) misjudges
  `{xxx}` as unclosed compound; the error path then aborts instead of
  degrading to empty output. Deferred parse-and-execute removes it.
- Verdict: **Rubash bug**. Severity: **High** (abort).

## comsub2 (48) — Bash 5.3 nofork/funsub/valsub

All 48 diff lines trace to `${ ...; }` (funsub) and `${| ...; }`
(valsub) semantics introduced in Bash 5.3.

### CS2-1 — `${| ...; }` discards the body's own stdout

- Repro `v44.sh`:
  ```sh
  echo ${| echo 67890; REPLY=12345; }
  echo x=<$REPLY>
  ```
  GNU: `67890` / `x=<12345>`. RB: `x=<abc>`-style output — the `67890`
  line is missing entirely.
- GNU cite: `parse.y`/`subst.c` nofork substitution — the valsub body
  writes to the *real* stdout; only `REPLY` is captured into the word.
- RB owner: `src/executor/embedded_mutations.rs:867-868` —
  `stdout_capture = Some(String::new())` with the comment
  *"GNU valsub assigns captured stdout bytes back to REPLY — it does not
  print them."* That comment states the **wrong** invariant: GNU does
  print the body output; `REPLY` is set *in addition*.
- Verdict: **Rubash bug** — the implementation comment codifies a
  misreading of `subst.c`. Severity: **High**.

### CS2-2 — `local` inside `${| ...; }` is rejected; `b` leaks

- Repro `v44.sh` (funsub-with-`local` lines):
  GNU: `inside: 12 22 42` / `outside: 42 2`.
  RB: `local: can only be used in a function` + `outside: 42 22`.
- GNU cite: funsub/valsub bodies run in a **function-like scope**;
  `local` is legal (Bash 5.3 `subst.c` nofork path pushes a variable
  context).
- RB owner: `src/executor/embedded_mutations.rs:890-893` — the
  `pipe_output` valsub branch calls `self.execute_ast(&body)` directly;
  `execute_current_shell_body` at :940 pushes a scope but the valsub
  branch does not, so `local` sees `function_depth == 0`.
- Verdict: **Rubash bug**. Severity: **High**.

### CS2-3 — valsub `REPLY` restore leaks the inner value

- Repro `v44.sh` nested valsub: GNU prints `inside1-inside2-outside`;
  RB prints `inside1-inside2-inside2` — inner `REPLY` leaks outward.
- RB owner: `embedded_mutations.rs:872-905` — the REPLY save/restore
  round-trips through `env_vars` (`save_scope_var`/`restore_scope_var`)
  but expansion reads a divergent store, so the restored value is not
  what subsequent `${REPLY}` sees.
- Verdict: **Rubash bug**. Severity: **High**.

### CS2-4 — funsub body is not exempt from `set -e`

- Tests: `comsub22.sub` — GNU prints `inside: after false`; RB does not.
- GNU cite: the nofork body runs in an expansion context where `errexit`
  does not abort mid-substitution.
- RB owner: `embedded_mutations.rs:940 execute_current_shell_body` —
  errexit flag is not suppressed for the duration of the body.
- Verdict: **Rubash bug**. Severity: **Medium**.

### CS2-5 — nested `${ }` inside other expansions prints literally

- Tests: `comsub-eof2/3.sub`, `comsub14.sub` region —
  `x=${ echo ${ echo one;} two }`, heredoc `funsub`,
  `[[ ${ echo -n "[...]"; } == ... ]]`, `$(( ${| x+=4; } ))`.
- GNU: recursively expands. RB: prints the literal
  `${ echo ${ echo one;} two }` text / `bad 1` / heredoc body literal.
- RB owner: `src/executor/parameter_core.rs:750 funsub_span_is_top_level`
  + `word_contains_current_shell_command_substitution` routing — nested
  funsub spans inside `${param-op}`, heredocs, `[[ ]]`, and `$(( ))` are
  not recursively expanded.
- Verdict: **Rubash bug**. Severity: **High**.

### CS2-6 — alias / `expand_aliases` state inside `${ }` diverges

- Tests: `comsub2.sub` alias block — GNU `shopt expand_aliases` → `on`,
  `1`/`2`; RB `off` / missing.
- Same root as CS-3: aliases must expand at *parse* time; the word-level
  path sees post-expansion text and reports post-expansion shopt state.
- Verdict: **Rubash bug**. Severity: **Medium**.

### CS2-7 — `${ }` positional parameters / `shift` / `jobs` formatting

- `comsub2.sub:38-40` (`${ func; }` `$@`/`shift` → `2 2` missing),
  `comsub17.sub` (`${ jobs; }` `[1]-`/`[2]+` markers and spacing).
- Owner: `embedded_mutations.rs` funsub positional-param handling and
  `jobs` output formatting.
- Verdict: **Rubash bug** (semantics) + **Low** (jobs spacing).

## comsub-posix (11)

### CSP-1 — `#` comment containing quotes/backslashes inside `$(...)` → `unexpected EOF` abort cascade

- Tests: `comsub-posix.tests:30-39` — comments containing `" ' \` inside
  `$(...)`. GNU: blank lines + `yes`. RB: `unexpected EOF` at
  `comsub-posix.tests:249` — the whole tail of the suite dies.
- Same scanner root as CS-1 (`continuation.rs:659`); the quote chars in
  the comment additionally corrupt the quote tracking.
- Verdict: **Rubash bug**. Severity: **High** (suite abort).

### CSP-2 — heredoc `)` delimiter inside `$(...)` merges output

- Tests: `comsub-posix.tests:36-43` (`cat <<')'` inside comsub).
- GNU: `hello` / `after 5` as separate lines. RB:
  `hello echo after 5 echo '` + `eof: command not found` on stderr.
- RB owner: `src/lexer/heredoc_scan.rs:1
  skip_heredoc_in_chars_with_closure` + `continuation.rs:704-716` —
  the heredoc delimiter that is itself `)` collides with the comsub
  close scan.
- rubash#117: scanner shortcut; deferred parse removes it.
- Verdict: **Rubash bug**. Severity: **High**.

### CSP-3 — `case`-pattern `)` / `esac` inside `$(...)` mis-tracked

- Tests: `comsub-posix5.sub:45` (`unexpected EOF`),
  `comsub-posix6.sub` (`do`/`done` token errors,
  `case: -c: invalid option`).
- RB owner: `src/lexer/skip.rs:630
  update_command_substitution_case_depth` + `skip.rs:326-368
  skip_cmd_subst` — case-pattern `(` `)` tracking diverges from
  `parse.y`'s `PST_CASEPAT` state.
- Verdict: **Rubash bug**. Severity: **High**.

### CSP-4 — `{fd}<$(...)` fd-variable redirection rejected

- Tests: `comsub-posix.tests` redirlist — `{fd2}<$(...)`.
- RB: `}` unexpected / `-` output. GNU: opens fd var, `close`/`0` ok.
- RB owner: parser redirection lexing of `{var}<` — independent of
  comsub scanning.
- Verdict: **Rubash bug**. Severity: **Medium**.

### CSP-5 — environment noise

- `comsub-posix3.sub:16: /bin/cat: command not found` — GNU env
  artifact, not a Rubash divergence.
- `${foo-"a}"`/`unexpected EOF` at tests:97 — **both** shells error;
  parity, not a divergence.

## new-exp (51)

### NE-1 — `echo` strips `"` that are part of the *value* (post-expansion quote removal in the builtin)

- Repro `v31.sh`:
  ```sh
  echo '"abc"'
  echo 'x"abc"y'
  echo "\")x\""
  x='")x"'; echo "$x"
  ```
  GNU: `"abc"` / `x"abc"y` / `")x"` / `")x"`.
  RB: `abc` / `x"abc"y` / `)x` / `)x`.
- GNU cite: `builtins/echo.def` — echo writes already-expanded argv
  verbatim; quote removal lives in `subst.c` (`dequote_string:4807`),
  not in the builtin.
- RB owner: `src/builtins/echo.rs:106-108 remove_residual_shell_quotes`
  — called unconditionally at :64/:73; the comment at :98-105 admits
  this is a workaround for expansion-time re-reading gaps.
- rubash#117: this is the *consumer-side* patch for a producer-side
  hole — classic whack-a-mole. The fix is at the expansion layer (the
  `"` should never survive to argv), not inside `echo`.
- Verdict: **Rubash bug**. Severity: **High** — any `"..."`-valued arg
  loses its quotes.
- Related narrow hack in the same file: `echo.rs` special-cases
  `args == ["hi)"]` → `hi` — a symptom-level patch for one
  `comsub-eof5` heredoc case; same verdict.

### NE-2 — `"${foo:-$@}"` / `"${foo:-$xxx$@}"` empty-field semantics

- Repro `v23.sh`/`v25.sh`: `recho "${foo:-$@}"` with zero params.
  GNU: `argv[1] = <>` (one empty field). RB: zero fields.
  Conversely `recho "${@%%[!/]*}"` (all-empty result): GNU drops
  fields, RB emits `argv[N] = <>`.
- GNU cite: `subst.c:9777 parameter_brace_expand` + `subst.c:13219
  expand_word_list_internal` — quoted `$@` inside `:-` yields exactly
  one empty field; an unquoted/`@`-list whose elements expand empty is
  deleted.
- RB owner: `src/executor/parameter_words.rs` / braced-op list
  handling — the `:-`-with-`$@` and empty-element-deletion rules are
  inverted.
- Verdict: **Rubash bug**. Severity: **Medium**.

### NE-3 — `${#:-}` valid vs `${#:}` invalid not distinguished

- Repro `v24.sh`: GNU `0` for `${#:-}`, bad-substitution for `${#:}`.
  RB: bad-substitution for **both**.
- GNU cite: `subst.c:9777` — `${#:-}` is `${#` + `:-` + empty word;
  `:-` permits a null word.
- RB owner: `src/executor/parameter_errors.rs:515-534
  is_length_operator_expression` — `rest.len() > 1` rejects the empty
  `:-` word. (`${#:}` correctly stays an error at :622-629.)
- Verdict: **Rubash bug**. Severity: **Medium**.

### NE-4 — array slice `"${a[@]:2}"` collapses to one word

- Repro `v26.sh`:
  ```sh
  a=(A B C D)
  b=("${a[@]:2}")
  echo "${#b[@]}", "${b[@]}"
  ```
  GNU: `2, C D`. RB: `1, C D` — the slice is joined into a single
  element.
- GNU cite: `subst.c` array slice (`parameter_brace_expand` `@`/`[*]`
  with `:off:len`) produces a word *list*.
- RB owner: braced array-slice expansion joins with space before the
  array-assignment sees the fields.
- Verdict: **Rubash bug**. Severity: **Medium**.

### NE-5 — `${@@Q}` / `${!var@Q}` edge cases

- Repro `v47.sh`: `printf "<%s> " "${@@Q}"` on unset/empty → GNU emits
  nothing for an unset `z` (`recho ${z@Q}` → no args); RB emits
  `argv[1] = <>` and an extra leading `<>` on the `"${@@Q}"` line.
  `${!VAR5[@]@Q}`/`${!VAR5@Q}` (varname=`VAR5[@]`) → GNU `'aaa' 'bbb'`;
  RB `''`/`'aaa'`.
- GNU cite: `subst.c` `@Q` on an unset/empty list produces no fields;
  `${!name[@]@Q}` applies the transform to the *expanded* list.
- RB owner: `src/executor/parameter_transforms.rs` — `@Q` on empty
  emits a spurious empty field; indirect+subscript+transform loses the
  `@` subscript.
- Verdict: **Rubash bug**. Severity: **Medium**.

### NE-6 — `${var@A}` drops `i`/`rl` attributes

- Repro `v46.sh`: `declare -ir x=4; echo ${x@A}` → GNU `declare -ir`,
  RB `declare -i`. `declare -ai`/`declare -arl` arrays → RB prints
  `declare -a` (integer/local-readonly attrs dropped).
- GNU cite: `subst.c`/`variables.c` — `@A` reprints the full attribute
  set.
- RB owner: `parameter_transforms.rs` `@A` path only emits the array
  flag.
- Verdict: **Rubash bug**. Severity: **Low-Medium**.

### NE-7 — `declare -f`/`type`/`set`-option inside `$(...)` return canned strings

- `src/executor/command_substitution.rs:200-206` returns hard-coded
  strings keyed on the body text: `"4"` for a `set`-option parse probe,
  `"bar() { echo $(< x1); }"` for `declare -f foo | sed`, `"./e"` for
  `type -p e` — literal answers to three test lines that still lose to
  GNU on the `$(< x1)` reprint (`< <(cat x1)` formatting, `bar ()`
  spacing) and return wrong data for every other input.
- rubash#117: the most literal possible whack-a-mole — a fixed string
  masquerading as semantics.
- Verdict: **Rubash bug**. Severity: **High** (returns wrong data for
  every other function).

### NE-8 — prompt `\[` → `\002` and `\$` → `$` vs `#`

- `new-exp10.sub` `${x@P}`/`prompt` decode: RB emits `\002` where GNU
  emits `\001` for `\[`, and `$` where GNU emits `#` for `\$`.
- `\$`: GNU prints `#` because euid==0 — environment-owned, not a
  Rubash defect.
- `\[`: `\001`/`\002` are the readline non-printing delimiters
  (`RL_PROMPT_START_IGNORE`/`END_IGNORE`); RB swapped them.
- RB owner: prompt-decode (`@P`) path.
- Verdict: `\[ `→ **Rubash bug** (Low); `\$` → **environment**.

### NE-9 — `$(IFS=: ...)` spurious `IFS: command not found`

- Same as CS-4 (stderr-only artifact of the word-level path).

## exp (6)

### EXP-1 — `${x#$HOME}` with a backslash-bearing `HOME` — environment

- `exp1.sub`: `HOME` is a Windows path with `\` under this harness;
  the pattern-mismatch is a consequence of the env var's bytes, not a
  Rubash pattern bug. Re-run under a POSIX `HOME` to confirm.
- Verdict: **environment** (pending POSIX-HOME re-run).

### EXP-2 — `declare -A`/`@A` reprint of `$'a\242b\002c'` → U+FFFD

- `exp5.sub`: byte `0xA2` in an ANSI-C-quoted assoc key is re-encoded
  as `` (U+FFFD) — the internal `String` cannot hold the
  non-UTF-8 byte, so the `@A`/key reprint corrupts it.
- GNU cite: `subst.c`/`stransform.c` — keys are byte strings;
  `ansic_quote` reprint preserves `0xA2`.
- Verdict: **Rubash bug** — byte-transparency limitation. Severity:
  **Medium**.

## more-exp (6)

### ME-1 — `expr "$X" : 'BRE'` returns `0` for all match lengths

- `more-exp1.sub`: `RELEASE`/`REL_LEVEL`/`REL_SUBLEVEL` all `0` →
  `0 0 0` vs GNU `4 2`.
- `expr`'s `:` regex-match operator is unimplemented/always-zero in
  this environment — either the RB `expr` builtin or the comsub path
  swallows the match count.
- Verdict: **needs one more repro** to separate builtin-vs-env; likely
  **Rubash bug** (expr `:` unimplemented). Severity: **Medium**.

### ME-2 — `${#:-}` — same as NE-3.

## posixexp (7)

### PE-1 — `a=$@` scalar join uses space instead of null-IFS

- Repro `v37.sh`: `IFS=''; a=$@` with `set -- 1 2`.
  GNU: `a=12`; RB: `a=1 2`. `$@`/`$*` in scalar context join with the
  **first char of IFS**; a null/empty IFS yields no separator. RB
  joined with a space regardless.
- GNU cite: `subst.c` `$*`/`$@` scalar join — `IFS` first char, empty
  when IFS unset/null.
- RB owner: `parameter_words.rs` scalar-join path.
- Verdict: **Rubash bug**. Severity: **Low-Medium**.

### PE-2 — nested `${x1%'t'}`/`${$'x1'%$'t'}` inside `${x#pat}` not expanded

- `posixexp2.sub`: GNU `OK`, RB `notOK` — a braced-parameter expansion
  *inside* a pattern position is left unexpanded.
- GNU cite: `subst.c:7663 parameter_brace_expand_word` — the pattern
  word undergoes full expansion including nested `${...}`.
- RB owner: `src/executor/expand_braced_patterns.rs` — the pattern
  side of `${x#...}` does not recursively expand `${...}`/`$'...'`.
- Verdict: **Rubash bug**. Severity: **Medium**.

### PE-3 — `cp ${THIS_SH}` prints the exe path — environment/WinCmd `cp`

- `posixexp1.sub`: `cp` under this harness echoes the source path —
  Windows tooling noise.
- Verdict: **environment**.

## procsub (11)

### PS-1 — `<(cmd)` materializes a re-readable temp file; GNU's pipe EOFs after one read

- Repro `v39.sh`:
  ```sh
  wc -l < <(echo x; echo y)
  wc -l < <(echo x; echo y)
  wc -l < <(echo x; echo y)
  ```
  GNU: `2 2 2` for fresh substitutions, but a *re-used* FD gives `0`
  on the second read (`2 2 2 0` for the shared-FD form). RB: `2 2 2
  2` — the temp file is still readable.
- GNU cite: `subst.c:process_substitute` → `/dev/fd` pipe — a pipe
  delivers bytes once; a second `wc` on the same fd sees EOF.
- RB owner: `src/executor/external_setup.rs:480
  materialize_process_substitution_word` +
  `write_process_substitution_temp_bytes` — the substitution is
  captured to a named temp file, so it is seekable/re-readable and
  never EOFs.
- Verdict: **Rubash bug** — structural (file vs pipe semantics).
  Severity: **Medium** (matters only for repeated reads of one
  substitution).

### PS-2 — `source $FN <(date)` under `-c` child — Windows TMPDIR backslash path

- `procsub.tests` `extern` block silent under RB: `source` receives
  `C:Users\...` with backslashes, the `\` are dequoted and the path
  fails (`No such file or directory`).
- Verdict: **environment/platform** (Windows temp-path in a POSIX
  code path) — though a POSIX-conformant materialization would also
  sidestep it.

## extglob (32)

### EG-1 — `a:b` cannot be created on Windows → whole `:`-file group absent

- `extglob.tests` `touch a.b a,b a:b a-b ...` — the `:` file silently
  doesn't exist under NTFS, so every glob line that should list `a:b`
  diffs.
- Verdict: **environment**.

### EG-2 — glob sort order — RB byte-order vs locale collation

- `.b a` vs `a .b`, `.foo bar a` ordering diffs — GNU (under
  `LC_ALL=en_US.UTF-8` inherited by the harness) uses `strcoll`;
  RB sorts bytewise.
- GNU cite: `pathexp.c` glob sort → `strcoll`.
- RB owner: `src/executor/glob.rs` sort.
- Verdict: **Rubash gap** (no locale collation) — **Low**; under
  `LC_ALL=C` the orders coincide, so it is environment-sensitive.

## cond (19)

### CD-1 — `=~` RHS `\(` is treated as a real group; `BASH_REMATCH` diverges

- `cond.tests`: `[[ jbig2dec =~ \(escaped\) ]]`-style RHS — GNU
  treats `\(` as a literal paren (no capture); RB compiles a real
  group → phantom captures / `jbig2dec` instead of empty.
- GNU cite: `execute_cmd.c`/`test.c` conditional matching — the RHS
  preserves regex-significant `\` (quoted-vs-unquoted tracked).
- RB owner: `src/executor/conditional.rs:458-463
  conditional_regex_match_status` —
  `expand_word`/`restore_numeric_decimal_regex_escapes` only restores
  `\N` decimal escapes; `\(`/`\)` are dequoted to bare parens.
- Verdict: **Rubash bug**. Severity: **Medium**.

### CD-2 — unquoted `(one two)` group RHS → no match

- `cond.tests`: `[[ $x =~ (one two) ]]` — RB does not match.
- Owner: `conditional.rs` — unquoted-space-in-group handling in the
  RHS word.
- Verdict: **Rubash bug**. Severity: **Medium**.

### CD-3 — bracket expressions `[\\]` `[']']` `[\[=G=]` rejected

- `cond.tests`: `bad 5`, missing `ok 11`/`ok 12`/`ok 4a`, plus
  `[[ ']' =~ [']'] ]]` → `syntax error` abort cascade.
- RB owner: `src/executor/conditional_command.rs` (parser acceptance
  of `]`/`'` inside brackets) + `conditional.rs:497-505
  compile_conditional_regex` / `translate_posix_bracket_classes`.
- Verdict: **Rubash bug** — parser + translator. Severity: **High**
  (abort).

### CD-4 — `[[ n -eq arith-expr ]]` does not evaluate arithmetic

- `cond.tests`: `[[ n -eq ... ]]` → `2` vs GNU `0`/`1`.
- RB owner: `conditional.rs` numeric-compare path — RHS is not run
  through arithmetic evaluation.
- Verdict: **Rubash bug**. Severity: **Medium**.

### CD-5 — `BASH_COMMAND` inside ERR trap re-quotes argv

- `cond.tests`: trap prints `'[[' '-n' '$unset' ']]'` (RB) vs `func`
  (GNU) — RB reports the requoted argv, GNU the resolved command.
- RB owner: `src/executor/command_text.rs` `BASH_COMMAND` tracking.
- Verdict: **Rubash bug**. Severity: **Low**.

## nquote (12)

### NQ-1 — `$'...'` in a `case` pattern is not ANSI-C-decoded

- `nquote.tests`: `case "$z" in $'\v\f\a\b')` → GNU `ok`, RB `bad`.
- GNU cite: `parse.y` case-pattern expansion runs the pattern through
  word expansion, so `$'...'` decodes before matching.
- RB owner: `src/executor/compound_exec.rs:1104 expand_case_pattern`
  / `quote_aware_case_pattern` — the `$'...'` text reaches the matcher
  undecoded.
- Verdict: **Rubash bug**. Severity: **Medium**.

### NQ-2 — `od` column spacing — environment/tooling noise.

## iquote (76)

### IQ-1 — `\r` stripped from words — deliberate CRLF normalization

- `iquote.tests`: `x^M`/`b=^M`/`recho ...^M` all lose `^M` (`x^My^M`
  → `xy`, `<^M>`→`<>`).
- RB owner: `src/lexer/mod.rs:196-199` —
  `raw_line.strip_suffix('\r')` — an acknowledged Windows-CRLF
  normalization (comment cites niubash#106 tradeoff).
- GNU cite: `parse.y:read_token_word` keeps `\r` as an ordinary byte.
- Verdict: **real divergence, platform-motivated** — flagged for a
  decision, not a silent fix. Severity: **Medium**.

### IQ-2 — `eval` of `$'\ooo'` produces `'177` instead of DEL

- `iquote.tests:60-67`: `eval tmp=`printf "$'\\\\\x%x'\n" $a`` and
  `eval c=\$\'\\$(printf '%o' $a)\'` — GNU `0x7f`, RB `'177`.
- Standalone repro `v50/v51` shows *both* shells error on the literal
  `\$(` form — the suite-level divergence is in how the eval'd text
  is re-lexed after the comsub expands inside the eval argument:
  RB appears to drop the `$` and decode `\'`+`177` literally.
- RB owner: eval arg re-lex + `decode_ansi_c_quoted` interaction.
- Verdict: **Rubash bug** (eval re-parse). Severity: **Medium** —
  partially reproduced; the standalone `\$(` form is parity (both
  error), so the residual delta is state-dependent.

### IQ-3 — `od` spacing / `$'\x7f'` direct assignment

- `od` field-width diffs — tooling noise.
- `y=$'\177'; recho "xxx${y}yyy"` → `xxx^?yyy` — **works** in RB;
  DEL is only lost through the `eval`/pattern paths above.

## Root-cause clusters (for fix planning)

1. **Comsub close-scan / pre-parser scanners** (CS-1, CSP-1..3,
   CS-5): three parallel `)`-matchers (`continuation.rs:659`,
   `skip.rs:326`, `heredoc_scan.rs:1`) each missing a different
   `parse.y` state (`#` comments, `)` heredoc delimiters, `case`
   parens). Fix the *scanner invariants* or defer the close decision
   to the parser — do not add per-symptom guards.
2. **Word-level comsub execution** (CS-3, CS-4): `split_shell_words`
   + text alias splice replaces `parse_and_execute`. Whitelist-only
   admission (pure-literal bodies) or parser fallback removes the
   class.
3. **Bash 5.3 nofork/funsub/valsub** (CS2-1..7): stdout capture
   invariant is wrong, function scope is not pushed, `REPLY` restore
   leaks, `set -e` not suppressed, nested spans not expanded. One
   subsystem — `embedded_mutations.rs` — one fix pass.
4. **Value-layer quote removal leaking into `echo`** (NE-1):
   `remove_residual_shell_quotes` is a consumer-side patch; the
   producer (expansion) is where `"` must die.
5. **`${param-op}` list/scalar edge cases** (NE-2, NE-3, NE-4, NE-5,
   PE-1, PE-2): empty-field rules, `:-` null-word, array-slice
   joining, `@Q` empties, `$@` scalar join, nested pattern
   expansion — all in `parameter_*`/`expand_braced_*`.
6. **Conditional `=~` RHS quoting + bracket translation** (CD-1..3):
   `\(`/`[`/`]` preservation and `BASH_REMATCH`.
7. **Process substitution = temp file** (PS-1): re-readable,
   non-EOF — structural gap vs `/dev/fd` pipes.
8. **Byte transparency** (EXP-2, IQ-2, NQ-1): non-UTF-8 bytes
   (`0xA2`, `0x7f`, `^M`) lost or corrupted through `String`-based
   storage / eval re-lex / case-pattern decode.
9. **Environment noise** (EG-1, parts of EG-2, PE-3, NQ-2, CSP-5,
   NE-8 `\$`): Windows filename limits, locale collation, `cp`/`od`
   tooling, euid-based prompt — exclude from Rubash defect counts.

## Verification status

- `git status --porcelain -- src/` — clean; no `src/` edits.
- `tasklist` — no stuck `rubash.exe`; `bash.exe` entries are the
  agent's own Git-Bash shells, not suite runners.
- All repro scripts under `.tmpwork/audit/exp/v*.sh`.
