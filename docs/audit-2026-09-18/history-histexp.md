# history + histexp compatibility audit — 2026-09-18

Read-only audit of the GNU Bash `history` (121 diff lines) and `histexp` (79)
suites against Rubash at code baseline `2494bc57`, per
`docs/audit-baseline-2026-09-18.md`. **No `src/` files were modified.**

## Method

- Oracle: owner-compiled GNU Bash 5.3.0 at WSL `/usr/local/bin/bash`,
  invoked from a **script file**
  (`MSYS_NO_PATHCONV=1 wsl /usr/local/bin/bash /mnt/d/repo/rubash/<file>.sh`),
  never `bash -c` (wsl.exe passthrough corrupts quoting — verified live: `$s`
  in a `-c` loop came through empty).
- Rubash: `target/debug/rubash.exe <file>.sh` (Windows binary — run from the
  Windows side, never under WSL: Linux paths like `TMPDIR=/tmp` do not map).
- Suite artifacts: `target/issue-suites/results/true-baseline/{history,histexp}/`
  — `gnu.out` 175 / `rb.out` 216 lines, `gnu.err` 27 / `rb.err` 29,
  `gnu.rc` **137** / `rb.rc` 0 (history); `gnu.out` 191 / `rb.out` 206,
  `gnu.err` 62 / `rb.err` 40, both rc 0 (histexp).
- Test sources: `target/issue-suites/results/bash-tests-rw/{history.tests,
  history1-9.sub, histexp.tests, histexp1-7.sub, history.list}`.
- Standalone per-sub reruns of every `historyN.sub`/`histexpN.sub` under both
  shells, plus minimal reproducers in `.tmpwork/audit/history/` — all
  byte-compared on stdout/stderr/rc, each repro run at least twice.
- Prior session's probes in `.tmpwork/audit/history/` (`hseq.sh`, `itest*.sh`,
  `run-subs.sh`, `h1probe.sub`, `app*.sh`) were re-verified, not trusted.

## Headline findings

1. **The GNU baseline itself is truncated.** `history/gnu.rc = 137`
   (SIGKILL): the GNU run hangs inside `history4.sub` at
   `printf ... | ${THIS_SH} --norc -i` — interactive bash 5.3.0 on non-tty
   piped stdin hangs under this WSL environment (verified: bare
   `printf 'echo hi\n' | bash --norc -i` is killed by timeout; under a pty via
   `script -qec` it works). `history7.sub` hangs the same way. Everything after
   `history4.sub` in `gnu.out` (all of history5–9) is absent *because GNU was
   killed*, not because rubash produced extra output. `histexp` has no `-i`
   subs and its GNU run completed (rc 0).

2. **One architectural divergence dominates both suites' real diffs**
   (class H1): Rubash executes `${THIS_SH} ./xN.sub` **in-process**
   (`src/executor/external_finish.rs:45 execute_same_shell_script` →
   `:121 execute_direct_shell_script` → `execute_ast`), so the child never
   goes through the line-oriented history driver
   (`src/main.rs:962 run_script_with_history` → `:1032 run_history_group`).
   Consequences observed in the artifacts: no recording (`history` listings
   empty, `history -d` reports bogus "out of range" on an empty list), no
   `!` expansion (`!!` printed literally), the child's `history` builtin
   **lists/mutates the parent's shared session** (`histexp6.sub` listed the
   parent's own commands), and `BASH_VERSION` is empty in the child
   (`echo ${BASH_VERSION%\.*}` → blank). This is the same in-process-child
   root cause already flagged Critical in `docs/audit-2026-09-18/nameref.md`
   (class C1).

   Proof: every sub run **directly** (`rubash.exe ./historyN.sub`,
   `bash ./historyN.sub`) is **byte-identical** between GNU and rubash for
   history1, 3, 5, 6, 8, 9 and histexp1–7 — including the `cat <<!` multiline
   cmdhist entry, `fc -s` re-execution echoes on stderr, `history -d`
   range deletion + error wording, HISTSIZE stifling, timestamped/multiline
   HISTFILE loading (history9 — the just-merged intl-history batch works),
   `!` inside `$( )`/backticks/`<( )`/double quotes/for-bodies, `!:*`,
   `!': event not found`, and `:p` print-only semantics.

3. **`history -a` diff is a stale-file artifact, not a semantic diff.**
   `gnu.out` showed `cat $HISTFILE` printing 8 lines; rb showed 2. Reproduced
   with a *fresh* TMPDIR (`hseq2.sh`): GNU `history -a` appends exactly the
   2 new-since-load lines — identical to rubash — and later commands are
   *not* incrementally appended. GNU C confirms:
   `builtins/history.def:294-295` → `bashhist.c:449-483 maybe_append_history`
   appends `min(history_lines_this_session, where_history())` last entries;
   `history_lines_this_session` counts only lines added via
   `really_add_history` (`bashhist.c:958-965`), not file-loaded ones. The
   artifact's 8-line file is leftover `tmp/newhistory` from the previously
   SIGKILLed run (the `trap 'rm $TMPDIR/newhistory' 0` never ran; the file
   still sits in `true-baseline/history/tmp/`).

## Divergence class table

| # | Class | Repro (`.tmpwork/audit/history/`) | GNU cite | Rubash owner | Verdict | Severity |
|---|-------|-----------------------------------|----------|--------------|---------|----------|
| H1 | In-process `${THIS_SH}` child: no line-driver → no recording, no histexpand, shared parent session, no `BASH_VERSION` init | `parent-probe.sh`+`child-probe.sub`, `h1parent.sh`+`h1child.sub`; whole-suite replay `rb-now.out` is byte-identical to `rb.out` artifact | `execute_cmd.c:6139-6233` child is a real process; `parse.y:2645-2661` `pre_process_line` per input line when `remember_on_history`; `builtins/set.def:650` `set_history`→`bashhist.c:305-316 bash_history_enable`; `variables.c:511-526` `initialize_shell_variables` seeds `BASH_VERSION` | `src/executor/external_finish.rs:45-119` same-shell dispatch; `:121-258` `execute_direct_shell_script` (whole-AST `execute_ast` at :228; exported-only env `child_shell_environment` :260-294; `session_history` Rc shared, not saved/restored); drivers bypassed: `src/main.rs:882-895`, `962-1027`, `1032-1158` | root-caused | **Critical** (all sub-file diffs in both suites) |
| H2 | `fc`/`history`/`history -p` inside `$( )` see an **empty** history list | `fccs.sh`: GNU `$(fc -nl -1)`→`\t echo alpha`, `$(history 1)`→`5  echo "CS2..."`, `$(history -p '!!')`→expansion; rb prints `[]`/`[]`/`history expansion failed` | `fc.def:330-335`: `rh = remember_on_history \|\| (subshell_environment & SUBSHELL_COMSUB) && enable_history_list` — comsubs deliberately consult the list (fork shares `the_history`) | `src/executor/command_substitution.rs:657-659` `command_substitution_executor` sets `session_history: None`; `src/executor/job_builtins.rs:791-796` then fabricates a fresh empty session | root-caused | Medium |
| H3 | `/bin/sh` does not exist for the Windows rubash (`histexp.tests:62` `/bin/sh -c 'echo this is $0'`) | artifact rb.err:8 `./histexp.tests: line 62: /bin/sh: command not found`; gnu.out:27 `this is /bin/sh` | — | — | environment | n/a (host PATH) |
| H4 | GNU baseline truncation + stale `newhistory` | `gnu.rc`=137; `history/tmp/newhistory` (200 B, post-SIGKILL leftover); `run-htests-gnu.sh` fresh-TMPDIR rerun → `cat $HISTFILE` = 2 lines | `history.def:294` `maybe_append_history`; `bashhist.c:449-483` | — | environment / harness | n/a — **baseline needs regeneration** (pty or skip `-i` subs) |

## Class details

### H1 — in-process `${THIS_SH}` children (dominant; Critical)

Every `historyN.sub`/`histexpN.sub` is invoked as `${THIS_SH} ./xN.sub` from
the parent `.tests` file. GNU forks a real shell per `execute_cmd.c:6139-6233`;
the child's parser loop calls `pre_process_line` (`parse.y:2645-2661`) per
input line, so `set -o history`/`set -o histexpand` inside the sub produce
recording + `!` expansion exactly as in the parent.

Rubash routes the invocation to `execute_same_shell_script`
(`external_finish.rs:45`), which expands `${THIS_SH}` to the current exe and
then runs the script text **in the same process** via
`execute_direct_shell_script` → `execute_ast` (`external_finish.rs:228`).
The per-group history pipeline in `main.rs` (`script_uses_history` :931,
`run_script_with_history` :962, `run_history_group` :1032, recording at
:882-895) is only reached for top-level script/stdin drivers — never for
this in-process child. Measured child behavior:

```
${THIS_SH} ./child-probe.sub   # child: echo BV, set -o history, echo aaa/bbb, history
→ child BV=[]            (BASH_VERSION absent: child_shell_environment keeps
                          only EXPORTED vars, external_finish.rs:260-294;
                          init.rs:183 or_insert only runs in Executor::new)
→ aaa bbb child done     (echoes run; `history` prints nothing — no recording)
```

vs direct `rubash.exe ./child-probe.sub`: `BV=5.3.0(1)-release`, listing
`1 echo aaa / 2 echo bbb / 3 history`.

Artifact-confirmed consequences:
- `history1.sub`: rb shows `one two three` then `0` — `history`/`fc -s`
  output gone; `fc -s cat`/`fc -s -1` silently return 0 on the empty list
  (direct-run rb prints `fc: no command found`, rc 1 — an additional
  in-process-path quirk worth noting: the in-process fc error path differs
  from the real-process one).
- `history2.sub`: `echo ${BASH_VERSION%\.*}` → blank lines (empty
  BASH_VERSION); `echo $(fc -nl -1)` → blank (also covered by H2).
- `history3.sub`: `history` listings empty; `history -d 2-4` errors
  `history: 2: history position out of range` (empty list) where GNU deletes
  the range; error *numbers* diverge downstream (`1` vs `200`, `1` vs `-50`,
  `5` vs `5-0xaf`) — all explained by the empty list.
- `history5/6/8.sub`: every `fc -l`/`history` listing empty; rb's extra
  `-1`/`-2`/`5`/`72` "out of range" errors match an empty list.
- `histexp2-6.sub`: `!!`/`!-1`/`!:*` literal, `echo "'!'"` prints `'!'`
  instead of `!': event not found` — histexpand never ran.
- `histexp6.sub`: child's `history` lists the **parent's** entries
  (`1 unset HISTFILE` … `20 ${THIS_SH} ./histexp6.sub`) — the shared
  `session_history` Rc is not saved/restored across
  `execute_direct_shell_script`, so child `history -c`/`-r`/`-d` also leak
  back into the parent session.

Fix direction (not implemented — audit only): either spawn a real process
(the in-process path exists because the wrapper loses buffered stdin lines,
per the comment at `external_finish.rs:49-53`), or route the child's script
text through the same group driver as `run_script_with_history` and seed
fresh-shell variables (`BASH_VERSION`, `BASH_VERSINFO`, `MACHTYPE`, …) into
`child_shell_environment`, and snapshot/restore `session_history` around the
invocation. Do not special-case the subs with content checks.

### H2 — history invisible inside command substitution (Medium)

Minimal repro (`fccs.sh`, verified twice, direct non-child execution):

```sh
set -o history
HISTSIZE=256
HISTFILE=/dev/null
echo alpha
echo "CS=[$(fc -nl -1)]"          # GNU: [   echo alpha]   rb: []
echo "CS2=[$(history 1)]"         # GNU: [    5  echo "CS2=…"]  rb: []
echo "CS3=[$(history -p '!!')]"   # GNU: [echo "CS2=…"]    rb: history expansion failed
```

GNU's comsub is a fork sharing `the_history`; `fc.def:330-335` explicitly
computes `rh` so a comsub in a `set -o history` shell still resolves `-1`
against the real list. Rubash builds the comsub executor with
`session_history: None` (`command_substitution.rs:659`), and
`execute_history` then fabricates a brand-new empty session
(`job_builtins.rs:791-796`). Owner: share `self.session_history.clone()`
into the comsub executor (it is `Rc<RefCell<_>>`, matching the fork-shared
list semantics; mutations like `history -c` inside a comsub would then also
match GNU, which does propagate them).

### H3 — `/bin/sh` missing (environment)

`histexp.tests:62` runs `/bin/sh -c 'echo this is $0'`. GNU ran under WSL
where `/bin/sh` exists; rubash on Windows reports `command not found`.
WinuxCmd/host-owned line, not a rubash semantic.

### H4 — broken GNU baseline (environment/harness)

- `history/gnu.rc` = 137. The hang is inside `history4.sub`'s first
  `printf … | ${THIS_SH} --norc -i 2>/dev/null`: interactive GNU bash with
  non-tty piped stdin never exits under this WSL session (no controlling
  tty). `script -qec 'bash --norc -i' /dev/null` works fine, so a pty
  harness — or skipping `-i` subs — is required to produce a real GNU
  baseline for history4/history7. `histexp` is unaffected (no `-i` subs).
- The phantom 8-line `cat $HISTFILE` in gnu.out is stale
  `tmp/newhistory` content surviving SIGKILL (trap never ran). Fresh-TMPDIR
  GNU rerun prints 2 lines — identical to rubash. `history -a` semantics
  verified identical (`maybe_append_history`, `bashhist.c:449-483`).

## What is NOT divergent

Direct per-sub runs show byte-identical output for: `history1.sub`
(multiline `cat <<!` cmdhist entry, `fc -s cat` re-run + stderr echo,
`fc -s -1` of `(exit 42)`, `echo $?`=42), `history3.sub` (`-d 2-4`,
`-d 6--1`, all five error messages verbatim, `-d @42` "invalid number" /
"numeric argument required"), `history5.sub` (all `fc -l` range/edge
behavior incl. `fc -0` → `history specification out of range`, `fc -s -0` →
`no command found`, comment lines recorded as entries), `history6.sub`
(HISTSIZE=4 stifling + `-d -1`/`-d -2--1`/`-d 5-7` listings), `history8.sub`
(`-d 2` delete + `72`/`-72` errors), `history9.sub` (timestamped HISTFILE
with embedded blank lines — the merged intl-history work holds up),
`histexp1-7.sub` (all `!` contexts, `:p`, `s/…/…/`, extglob word
tokenization, heredoc-in-comsub exclusion). The `history.tests` and
`histexp.tests` main bodies are identical except the `/bin/sh` line.

## Repro index (`.tmpwork/audit/history/`)

- `run-subs-gnu.sh` / `run-subs-gnu2.sh` / `run-hx-gnu.sh` — per-sub GNU
  runs (history4 hangs; 7 times out).
- `run-subs-rb.sh` — per-sub rubash runs (run under Git Bash, not WSL).
- `hseq2.sh` / `hseq2-rb.sh` — `history -a` file-content repro (2 lines both).
- `run-htests-gnu.sh` — fresh-TMPDIR full GNU history.tests (hangs at
  history4; proves the 2-line `-a` file).
- `parent-probe.sh`/`child-probe.sub`, `h1parent.sh`/`h1child.sub`,
  `envparent.sh`/`envchild.sub` — in-process-child repros (empty
  `BASH_VERSION`, no recording, silent `fc -s` rc 0).
- `fccs.sh`/`fccs-rb.sh` — `$(fc -nl -1)`/`$(history)`/`$(history -p)` empty
  in comsub.
- `fc-empty.sh` — `fc -s` on empty list (identical modulo `$0` prefix).
- `pty-test.sh`, `itest*.sh` (prior session) — `-i`/` -in` hang vs pty.

## Suggested next steps (for the parent agent)

1. Regenerate the `history` GNU baseline with a pty harness or with
   history4/history7 excluded; expect the suite to collapse to the H1/H2
   diffs plus H3.
2. H1 fix: route `execute_direct_shell_script` through the line driver and
   seed fresh-shell vars + snapshot `session_history`; verify against the
   full 83-suite ledger (this path is shared with nameref's C1).
3. H2 fix: `session_history: self.session_history.clone()` in
   `command_substitution_executor` — small, self-contained.
