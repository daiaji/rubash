# errors / type / posix2 / set-e / invocation compatibility audit — 2026-09-18

Read-only audit of the GNU Bash `errors`, `type`, `posix2`, `set-e`, and
`invocation` suites against Rubash at code baseline `2494bc57`
(code-identical to `c282a850`), per `docs/audit-baseline-2026-09-18.md`.
No `src/` files were modified.

## Method

- Oracle: owner-compiled GNU Bash 5.3.0 at WSL `/usr/local/bin/bash`,
  invoked from a **script file**
  (`MSYS_NO_PATHCONV=1 wsl /usr/local/bin/bash /mnt/d/repo/rubash/<file>.sh`),
  never `bash -c`. During this audit, `wsl … bash -c 'echo $?'`-style
  probes were observed to return pre-expanded values (`$?` arrives as
  `0`), so every reproducer below is a file on disk.
- Rubash: `target/debug/rubash.exe <file>.sh`, `__RUBASH_NO_UPSTREAM_SCRIPTS=1`.
- Suite artifacts: `target/issue-suites/results/true-baseline/<suite>/`.
- Minimal reproducers: `.tmpwork/audit/{posix2,set-e,invocation,errors,type}/`.
- GNU C source in `third_party/bash/` is the specification; every class
  below cites the owning C function.

## Headline numbers

| Suite | stdout diff lines | Root causes |
| --- | --- | --- |
| errors | 129 | 3: `OLDPWD`/`cd -` working-directory divergence (bulk), `$[…]` silent expansion, `${$name}` bad substitution not fatal |
| type | 39 | 4: coproc brace-body parse aborts `type4.sub`, `type -f` inverted, hash hit count hardcoded, `/tmp/bash`↔`/tmp/rubash.exe` name (env) |
| posix2 | 3 | 1: `TMPDIR` not forwarded by WSL interop + backslash temp path re-parse mangling (environment-triggered, rubash-aggravated) |
| set-e | 1 | 1: `!` + pipeline does not suppress `errexit` inside a compound stage |
| invocation | 2 | 1 stdout cause (`bash ls` binary-file refusal vs PATH miss); plus stderr-only: env-imported `BASHOPTS`/`SHELLOPTS` lose readonly, message-format diffs |

The headline `errors` hypothesis — "GNU stops at a fatal error and Rubash
continues" — is **disproven for the bulk of the suite**. GNU does not
terminate early; it `cd`s out of the test directory so the twelve
`errors*.sub` files cannot be found, while Rubash stays in place and runs
them all. The POSIX special-builtin fatal paths (`return`, `unset`
readonly, `. /nosuchfile`, `trap` bad signal) match GNU — see E4.

---

## 1. `errors` suite (129 stdout diff lines)

Suite artifacts: `true-baseline/errors/`. GNU stdout is 9 lines;
Rubash's is ~137. The full GNU stdout:

```
declare -fr func
after f
/mnt/d/repo/rubash
1
1
1
after return
after trap
end
```

### E1 — `cd -` succeeds under GNU, fails under Rubash → all twelve `errors*.sub` execute only under Rubash (≈125 of 129 lines)

- Test lines: `errors.tests:224` (`cd -`), `errors.tests:351-375`
  (`${THIS_SH} ./errors1.sub` … `./errors12.sub`).
- Chain:
  1. The harness runs each suite in a subshell `cd "$BASE"` from
     `$REPO`, so the exported environment carries
     `OLDPWD=/mnt/d/repo/rubash`.
  2. GNU startup (`variables.c:952-963`, gated by
     `config-top.h:183 OLDPWD_CHECK_DIRECTORY`) imports `OLDPWD` when it
     names a directory — `/mnt/d/repo/rubash` exists on the WSL side —
     and keeps it exported.
  3. `errors.tests:224` `cd -` → GNU chdirs to `/mnt/d/repo/rubash` and
     prints it (`gnu.out` line 3).
  4. Lines 351-375 then look up `./errorsN.sub` relative to the new cwd;
     they do not exist there → each child prints
     `…/bash: ./errorsN.sub: No such file or directory` **to stderr**
     (`gnu.err` tail) and contributes nothing to stdout.
  5. Rubash removes `OLDPWD` at init (`src/executor/init.rs:135`
     `env_vars.remove("OLDPWD")`), so `cd -` fails with
     `cd: OLDPWD not set` (`src/builtins/cd.rs:299-307`), cwd stays
     `bash-tests-rw`, and every `./errorsN.sub` is found and executed —
     producing the ~125 extra stdout lines (`rb.out` lines 6-133).
- Repro (file-based, run from a different cwd with `OLDPWD` exported
  pointing at an existing dir): GNU `cd -` prints the dir and rc=0;
  Rubash reports `cd: OLDPWD not set`, rc=1.
- Verdict: environment-triggered, Rubash-owned semantic gap. GNU's rule
  is "import OLDPWD iff it names a directory"; Rubash's init deletes it
  unconditionally. A complicating sub-issue: the inherited value is a
  WSL path (`/mnt/d/…`); `src/executor/path.rs` already has `/mnt/X`
  translation support, but it never sees the value because init removes
  it first. Not a missing-fatal-error path.

### E2 — `$[…]` legacy arithmetic swallows the error; extra blank line

- Test line: `errors.tests:286` `eval echo \$[/bin/sh + 0]`
  (paired with `:287` `eval echo '$((/bin/sh + 0))'`).
- GNU stdout: nothing for either line (both are arithmetic syntax errors
  on stderr: `/bin/sh + 0: arithmetic syntax error: operand expected`).
  Rubash stdout: one **blank line** (`rb.out` line 5, diff hunk `5a5`).
- GNU cite: `subst.c:10276 bad_substitution:` — `set_exit_status
  (EXECUTION_FAILURE)` + `report_error`, returning
  `&expand_wdesc_fatal` when `posixly_correct && !interactive`
  (`subst.c:10288`). `$[…]` goes through the same arithmetic-evaluation
  error path as `$((…))`.
- RB owners: `src/parser/arithmetic_expansion.rs:100-137` parses the
  `$[…]` form; `src/executor/embedded_mutations.rs:548-560` collects and
  evaluates it; the whole-word `$((…))` path in
  `src/executor/expand_word.rs:275-324` does report the error correctly
  (only the `$[…]` line is silent). `src/executor/parameter_core.rs:257`
  contains a fast-path check for `$( (` / `$[` forms.
- Verdict: Rubash expansion/error-reporting gap — the `$[…]` evaluator
  fails silently and yields an empty expansion, so `echo` prints a blank
  line and the command exits 0. Per rubash#117 do **not** fix this with
  another `contains`/blacklist admission on the fast path; the `$[…]`
  evaluator must report the same structured expansion error the
  `$((…))` path reports.

### E3 — `${$name}` bad substitution silently continues (stderr-visible real bug inside the "extra" output)

- Test: `errors2.sub:1-3`
  (`set -e; trap 'echo $?' EXIT; echo ${$NO_SUCH_VAR}`).
- Repro `.tmpwork/audit/errors/badsub.sh`:
  - GNU: stderr `badsub.sh: line 3: ${$NO_SUCH_VAR}: bad substitution`,
    then `TRAP:0`, `gnu_rc=1`. The expansion error fails the command and
    `set -e` ends the script — `echo after` never runs.
  - RB: prints a blank line, then `after`, `TRAP:0`, `rb_rc=0` — the
    malformed expansion is silently replaced by the empty string.
- GNU cite: `subst.c:10276 bad_substitution:` as in E2; the enclosing
  command fails with `EXECUTION_FAILURE` and `execute_cmd.c`
  command-list/`set -e` propagation does the rest.
- RB owners: `src/executor/parameter_errors.rs:600-739` returns
  structured "bad substitution" errors for other malformed forms, but
  `${$name}` never reaches them — the inner `$` is treated as a nested
  expansion producing empty. The consumed-error path is
  `src/executor/command_prepare.rs` +
  `src/executor/ast_exec.rs:777-780`.
- Verdict: Rubash bug — expansion-fatal propagation gap. (Invisible in
  the ledger because GNU never reaches `errors2.sub`, but real.)

### E4 — Non-divergences worth recording (the feared fatal-error cluster is already correct)

GNU and Rubash agree on every POSIX special-builtin fatality probed by
the tail of `errors.tests` (366-383): `return` at top level is non-fatal
without `-o posix` and fatal with it (`after return` printed once by
both); `unset` of a readonly var and of a non-identifier are fatal under
`-o posix`; `. /nosuchfile` is fatal under `-o posix`; `trap … SIGNOSIG`
is non-fatal (`after trap` in both); `function !!` is accepted in posix
mode (`end` in both). Both stderr tails are byte-identical for these
lines (`rb.err`/`gnu.err` tail). No missing fatal-error path here.

---

## 2. `type` suite (39 stdout diff lines)

Suite artifacts: `true-baseline/type/`. Diff hunks:

```
37,38:  /tmp/bash            vs /tmp/rubash.exe        (env — exe name)
42:        3\t/tmp/bash      vs    1\t/tmp/rubash.exe  (hash hits + name)
103-134: three mkcoprocs function descriptions MISSING under Rubash
136:    cat is aliased to `echo cat'  MISSING under Rubash
```

### T1 — `coproc name { … }` brace body inside a function aborts `type4.sub` parse (32 of 39 lines)

- Test: `type4.sub:28-42` (`mkcoprocs()` containing
  `coproc a { cat <<EOF1 … }` and `coproc b { cat << EOF2 … }`),
  `type4.sub:46-56` (`coproc ( b cat <<EOF … )`),
  `type4.sub:61-66` (`coproc cat -u - & read -u ${COPROC[0]} msg`).
- Repro (run `type4.sub` directly): Rubash prints the preceding
  `type bb` output, then `./type4.sub: line 40: syntax error near
  unexpected token '}'`, rc=2 — the whole file aborts at the first
  function definition, so all three `type mkcoprocs` blocks
  (`gnu.out:103-134`) are missing.
- GNU cite: coprocess grammar `parse.y:1125-1174`
  (`coproc NAME compound_command`, including `{' list '}` bodies) and
  token handling `parse.y:5931-5940`.
- RB owners: `src/parser/coproc_command.rs:8-68` (named coproc parsing,
  brace/subshell bodies) and `src/parser/support.rs:275-322`
  (brace-boundary matching requires a completed command before `}`).
  The heredoc-terminated brace body leaves the parser in a state where
  the closing `}` is not accepted inside a function body.
- Verdict: Rubash parser bug. Secondary cosmetic divergence in the same
  family: an unnamed subshell coproc reprints without the implicit name
  — Rubash `coproc ( cat <<EOF … )` vs GNU `coproc COPROC ( cat <<EOF
  … )` (GNU stores the name `COPROC` internally, `parse.y:1125-1149`;
  RB reprint owner `src/parser/ast_print.rs:725-749`). Verified via
  `.tmpwork/audit/type/coproc2.sh`. The third form
  (`coproc cat -u - &`) parses and prints identically
  (`.tmpwork/audit/type/coproc3.sh`).

### T2 — `type -f` semantics are inverted (suppresses aliases instead of functions)

- Test: `type5.sub:30-33`
  (`shopt -s expand_aliases; type cat; alias cat='echo cat'; type -f cat`).
- Repro `.tmpwork/audit/type/typef.sh`:
  - GNU: `type -f cat` → `cat is aliased to 'echo cat'` (alias still
    reported; only function lookup is suppressed).
  - RB: `type -f cat` → `type: cat: not found` (rc=1).
- GNU cite: `builtins/type.def:153-154` — `-f` sets `CDESC_NOFUNCS`;
  `type.def:281` skips only `find_function` when `CDESC_NOFUNCS` is set.
- RB owner: `src/executor/type_builtin.rs:162` maps `'f' =>
  functions_only = true` — inverted vs `CDESC_NOFUNCS`; the flag then
  filters `describe_name*` (`type_builtin.rs:143-215`,
  `src/executor/type_describe.rs`) so alias/keyword/builtin/file hits
  are suppressed instead of functions.
- Verdict: Rubash bug (option semantics inverted).

### T3 — hash-table hit counts are hardcoded

- Test: `type.tests` hashed-commands block (`gnu.out:40-42`).
- GNU prints the real hash table: `1 /bin/sh`, `3 /tmp/bash` —
  generated from actual entries/hits in
  `builtins/hash.def:267` (`hash -l`/listing loop).
- RB prints `1 /bin/sh`, `1 /tmp/rubash.exe`;
  `src/builtins/hash.rs:196-205` literally hardcodes
  `let hits = if name == "bash" { 3 } else { 1 };`.
- The `/tmp/bash` ↔ `/tmp/rubash.exe` name difference itself is
  environment-owned (the suite hashes `$THIS_SH`/`$PWD/bash`-equivalent
  — different executable). The hit count is Rubash-owned.

### T4 — environment-only differences

- `/tmp/bash` vs `/tmp/rubash.exe` (lines 37-38): the test creates
  `/tmp/bash` (or equivalent) keyed to the shell name. Expected for a
  differently-named binary; not a bug.

---

## 3. `posix2` suite (3 stdout diff lines)

Diff: Rubash adds `running $@ test failed` and reports `2 of 27` vs
GNU's `1 of 27`. Both emit the same stderr syntax error near
`posix2.tests:199` — not the divergence.

### P1 — `TMPDIR` never reaches the Windows rubash.exe; the fallback's backslashes mangle on re-parse

- Test: `posix2.tests:94-110` — writes `$TMPDIR/conftest1` containing
  `$TMPDIR/conftest2 "\$@"` and `$TMPDIR/conftest2` (`echo $#`), then
  `numargs=$($TESTSHELL $TMPDIR/conftest1)`; non-zero rc →
  `testfail 'running $@'`.
- Environment layer: the harness sets `TMPDIR=$w/tmp` (a `/mnt/d/…`
  Linux path) for both sides. GNU is a Linux process and sees it;
  **WSL interop does not forward the Linux environment block to Windows
  child processes** — verified empirically: `export MYUNIQUE_X=barbaz
  TMPDIR=…; rubash.exe script.sh` shows neither var in `set` output, and
  `cmd.exe /c "echo %TMPDIR%"` under WSL prints `%TMPDIR%`. (Earlier
  `-c`-based observations that "TMPDIR changes inside `source`" were
  artifacts of the wsl.exe/Git-Bash arg passthrough pre-expanding `$`
  in the `-c` string — file-based probes show `TMPDIR` is constant
  across `source` boundaries.)
- Rubash layer: `src/executor/init.rs:113-115` fills the missing TMPDIR
  via `safe_temp_dir_string()` (`src/executor/local_helpers.rs:185-197`),
  which returns the raw Windows `TEMP`/`TMP` —
  `C:\Users\ADMINI~1\AppData\Local\Temp`, **with backslashes**.
- Failure mechanics (repro `.tmpwork/audit/posix2/posixrepro2.sh`):
  1. The unquoted heredoc writes `C:\Users\ADMINI~1\AppData\Local\Temp/
     conftest2 "$@"` into `conftest1`.
  2. The child rubash parses that line: backslashes are quote
     characters, so the command word becomes the mangled
     `C:UsersADMINI~1AppDataLocalTemp/conftest2` → command not found →
     conftest1 exits 127 → `testfail 'running $@'`.
  3. With `TMPDIR=/tmp` (forward slashes) the same repro passes
     (`posixrepro.sh`, rc=0, `numargs=0`).
- GNU cite: the test relies only on ordinary env inheritance +
  `evalfile.c`/child-script semantics; GNU does nothing special —
  its TMPDIR is a Linux path without backslashes, so re-parse is safe.
- Verdict: environment-triggered (WSL env non-forwarding is the harness
  limitation noted in `scripts/true-baseline.sh:31-35`), with a
  Rubash-owned aggravation: the TMPDIR fallback is a native-format path.
  If `safe_temp_dir_string` (or the init insert at `init.rs:113-115`)
  returned a shell-display form (`C:/Users/…` forward slashes, as
  `shell_pwd_display` does for PWD at `init.rs:104-111`), the value
  would survive the heredoc→re-parse round trip and the test would
  pass. Worth fixing at the TMPDIR normalization layer, not in the
  test or the coproc/exec path.

---

## 4. `set-e` suite (1 stdout diff line)

Diff: `53d52` — GNU has `A 1` where Rubash has none.

### S1 — `!` + pipeline does not suppress `errexit` inside a compound stage

- Test: `set-e1.sub:40`
  (`${THIS_SH} -ce '! { false; echo A $?; } | cat; echo B $?'; echo C $?`).
- GNU child output: `A 1`, `B 1`; Rubash: `B 1` only — the brace group
  dies at `false` despite the `!`.
- Repro `.tmpwork/audit/set-e/se1.sh`
  (`! { false; echo A $?; } | cat; echo B $?` under `-e`):
  GNU file-mode → `A 1`/`B 1`; Rubash → `B 1`. (Control cases:
  `! { false; echo A $?; }` and `! (false; echo A $?)` **without** the
  pipe behave identically in both — `se2.sh`/`se3.sh` — so the gap is
  specific to `!` + pipeline + compound stage.)
- GNU mechanism (`execute_cmd.c`):
  - `650-656`: `CMD_INVERT_RETURN` + `exit_immediately_on_error` →
    `command->flags |= CMD_IGNORE_RETURN`.
  - `execute_pipeline:2702-2703,2722-2723`: when `invert ||
    ignore_return`, every pipeline element gets `CMD_IGNORE_RETURN`.
  - `1104-1108`: a group command with `ignore_return` sets
    `CMD_IGNORE_RETURN` on its inner list → `false` inside
    `{ false; echo A $?; }` cannot kill the group.
- RB owners: `src/executor/pipeline_exec.rs:359-383` —
  `preserve_compound_errexit` is decided only by stage shape
  (`command_is_compound_pipeline_stage`, `pipeline_exec.rs:1817-1831`)
  and `!` is applied only to the final status at
  `pipeline_exec.rs:411-415` (`first.inverted` /
  `time_prefix.inverted`); it is never consulted for the stage dispatch.
  `src/executor/pipeline_stages.rs:64-68` then resets
  `subshell.suppress_errexit = 0` when `force_compound_errexit` is set,
  re-arming `-e` inside the group.
- Verdict: Rubash bug — the invert flag must suppress errexit inside
  pipeline stages the way `CMD_IGNORE_RETURN` does in GNU.

---

## 5. `invocation` suite (2 stdout diff lines)

Diff (stdout, one hunk):

```
< cannot execute binary file
> D:\repo\rubash\target\debug\rubash.exe: ls: No such file or directory
```

### I1 — `bash ls` (script lookup of a binary on PATH) — environment + resolution gap

- Test: `invocation.tests:73-75`
  (`PATH=/bin:/usr/bin; ${THIS_SH} ls |& sed 's|^.*: ||'`).
- GNU: `open_shell_script("ls")` fails → PATH search
  (`findcmd.c:258 find_path_file`, `shell.c:1572-1601`) finds
  `/bin/ls` → `check_binary_file` (`general.c:718-741`) refuses the ELF
  with `cannot execute binary file`, EX_BINARY_FILE (126); sed strips
  the prefix.
- RB: `run_script_file_with_init` (`src/main.rs:706-729`) →
  `find_script_on_path` (`public_accessors.rs:474-476` →
  `path.rs find_user_command`) searches the literal `/bin`, which does
  not exist in the Windows namespace → "No such file or directory", rc
  1. Repro `.tmpwork/audit/invocation/binfile.sh` matches the artifact.
- Nuance: rubash *does* have a logical `/bin`/`/usr/bin` mapping
  (`path.rs:516-526 logical_bin_command_name`) used by command lookup —
  `type ls` reports `ls is /bin/ls` — but the script-file PATH search
  does not apply it. If it did, it would find `ls.exe` (a PE, NULs in
  the first line) and `check_binary_file` (`main.rs:733-741`) would emit
  `cannot execute binary file`, matching GNU. So the stdout line is
  environment-mediated but the resolution gap is Rubash-owned.

### I2 — stderr-only divergences (excluded from the ledger, still real)

1. `invocation.tests:15` `${THIS_SH} .` — GNU: `.: .: Is a directory`;
   RB: `D:\…\rubash.exe: .: Permission denied`. Wrong errno mapping
   (EISDIR→EACCES) and wrong diagnostic prefix (exe path vs arg).
   Owner: `main.rs` script-open error path / `posix_errors`.
2. `invocation1.sub:40` `BASHOPTS=$BASHOPTS` and `invocation2.sub:50`
   `SHELLOPTS=$SHELLOPTS` — GNU stderr
   `./invocationN.sub: line NN: {BASHOPTS,SHELLOPTS}: readonly variable`;
   RB emits nothing **under the harness only**. Repro
   `.tmpwork/audit/invocation/{ro,parent}.sh`: standalone rubash does
   report readonly, but when the parent exports `BASHOPTS` into the
   child's environment, the child imports it **writable** — assignment
   silently succeeds (rc=0). GNU marks imported `SHELLOPTS`/`BASHOPTS`
   readonly at env import (`variables.c:507-510`:
   `ro = STREQ(name,"SHELLOPTS") || STREQ(name,"BASHOPTS")`).
   RB owner: `init.rs` env import / `mark_initial_exported_vars`
   (`init.rs:137`) never applies the readonly mark to imported
   BASHOPTS/SHELLOPTS. Rubash bug.
3. `invocation.tests:71` `./x23` bad-interpreter — GNU:
   `./invocation.tests: ./x23: nosuchfile: bad interpreter: No such file
   or directory`; RB: `bash: ./x23: nosuchfile: bad interpreter` —
   wrong prefix (literal `bash` instead of the script's `$0`) and
   missing the trailing errno text. Owner: exec/shebang fallback path.

---

## 6. Confirmed vs unresolved

**Confirmed Rubash bugs** (oracle-verified, file-based repro):
- E2 `$[…]` silent expansion failure (blank line, no diagnostic).
- E3 `${$name}` bad substitution swallowed (rc 0, no diagnostic,
  continues under `set -e`).
- T1 `coproc name { …heredoc… }` parse abort inside function bodies;
  plus unnamed-coproc reprint missing implicit `COPROC`.
- T2 `type -f` inverted (suppresses non-function hits).
- T3 hash hit count hardcoded (`hash.rs:196-205`).
- S1 `!`+pipeline does not suppress errexit inside compound stages.
- I2.2 env-imported `BASHOPTS`/`SHELLOPTS` lose readonly; I2.1/I2.3
  diagnostic prefix/errno-format mismatches (stderr only).

**Environment-mediated / mixed**:
- E1 `OLDPWD`/`cd -` — harness env provides a valid `OLDPWD`; GNU
  imports it, Rubash deletes it (`init.rs:135`). The behavioral choice
  is Rubash's; the trigger is the WSL-path env.
- P1 posix2 `running $@` — WSL env non-forwarding (harness limitation)
  plus Rubash's backslash TMPDIR fallback (`init.rs:113-115`,
  `local_helpers.rs:185-197`).
- I1 `bash ls` — `/bin` absent on Windows (env) plus script-PATH-search
  not using the logical-bin mapping (`path.rs:516-526`).
- T4 `/tmp/bash` vs `/tmp/rubash.exe` — expected exe-name difference.

**Disproven hypotheses**:
- "errors is dominated by GNU stopping at a fatal error" — GNU keeps
  running; it changes directory, so the `.sub` files 404 on stderr.
- "Rubash is missing POSIX special-builtin fatal paths" — the probed
  set (return/unset/source/trap/function) all match.
- "TMPDIR is rebound during `source` execution" — file-based probes show
  it is constant; earlier evidence was `-c`-arg passthrough corruption.

## Probing caveat discovered during this audit

`wsl … bash -c '…$VAR…'` / `…-c '…$?…'` probes are unreliable: the
wsl.exe/Git-Bash passthrough can deliver the string to bash with `$`
sequences already expanded (observed: `bash -c 'false; echo $?'`
printing `0`, and `BASH_EXECUTION_STRING` containing pre-substituted
values). All conclusions above rest on script files passed by path, per
AGENTS.md.
