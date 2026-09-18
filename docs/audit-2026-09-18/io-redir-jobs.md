# io / redirection / job-control compatibility audit — 2026-09-18

Read-only audit of the GNU Bash `jobs`, `redir`, `vredir`, `read`,
`coproc`, `posixpipe`, `glob`, `globstar`, `shopt`, `test`, `braces`,
`ifs-posix`, `intl`, and `set-x` suites against Rubash at code baseline
`2494bc57` (code-identical to `c282a850`), per
`docs/audit-baseline-2026-09-18.md`. No `src/` files were modified.

## Method

- Oracle: owner-compiled GNU Bash 5.3.0 at WSL `/usr/local/bin/bash`,
  invoked from a **script file**
  (`MSYS_NO_PATHCONV=1 wsl /usr/local/bin/bash /mnt/d/repo/rubash/<file>.sh`),
  never `bash -c` (Windows/WSL arg passthrough corrupts quoting).
- Rubash: `target/debug/rubash.exe <file>.sh`, cwd
  `target/issue-suites/results/bash-tests-rw`, bounded `timeout`.
- Suite artifacts: `target/issue-suites/results/true-baseline/<suite>/{gnu,rb}.{out,err,rc}`.
- Minimal reproducers: `.tmpwork/audit/io/*.sh`, each run through both
  shells; timing-sensitive job repros were run **twice** per shell.
- GNU C source in `third_party/bash/` is the specification; every
  substantive class cites the owning C function.
- Baseline caveat for `jobs`: **both** `gnu.rc` and `rb.rc` are `124`
  (harness `timeout -k 5 40`); GNU's captured output reaches further
  than Rubash's, so the tail of the suite is unmeasured on both sides
  and the 64-line diff is a lower bound.

## Headline numbers (stdout diff lines, baseline ledger)

```
jobs 64   redir 54   vredir 24   read 44   coproc 6    posixpipe 1
glob 67   globstar 4 shopt 13    test 13   braces 1    ifs-posix 1
intl 2    set-x 5
```

## Cross-cutting environment notes

- `/bin/sh` does not exist on the Windows side; any test that spawns
  `/bin/sh -c ...` fails under Rubash with status 127 regardless of
  shell semantics.
- Windows filesystems cannot create filenames containing `*`, `?`,
  `\`, or drive-invalid characters; several glob tests fail at fixture
  setup (`touch '*abc.c'` → `Invalid argument` in `rb.err`).
- No POSIX signals, ttys, mode bits (setuid/setgid/exec), or
  `/dev/{tty,fd}` on Windows — `test` predicates and `kill -STOP/CONT`
  that depend on them are environment-bound.
- Rubash launches each `cmd &` as a **fresh `rubash.exe -c <serialized
  source>` process** (`src/executor/compound_exec.rs:78-118`), not a
  fork of the live shell. Functions are re-serialized via
  `background_command_source` (`compound_exec.rs:120-133`) and state is
  passed through env vars. This architectural choice underlies most of
  the `jobs` divergence surface.

---

## jobs (64)

### J1 — `/bin/sh` not found → `job N returns 127` ×8 — ENV

- Tests: `jobs3.sub:18-20` (`/bin/sh -c "sleep 4; exit 0" &` and friends).
- Evidence: `rb.err` contains eight `bash: line 1: /bin/sh: command not
  found`; the corresponding `job N returns` lines print `127` instead
  of `0`.
- Verdict: **Environment** — no `/bin/sh` on the Windows side.
- Severity: n/a (harness/environment).

### J2 — `wait -n` returns the status of an already-reaped / wrong job — RUBASH

- Tests: `jobs5.sub` (`wait -n` after `wait $p`; `{ sleep 2; exit 12; } &`
  block, plus the `wait -n -p wpid` tail of the sub).
- Repro: `.tmpwork/audit/io/j1-waitn-stale.sh`
  - GNU (2/2 runs): `w1=0` / `wn=12`
  - RB run1: `w1=0` / `wn=0`; run2: `w1=0` / `wn=12` — **nondeterministic**;
    the completed `{ sleep 0.2; exit 12; }` job's status is sometimes lost.
- GNU cite: `builtins/wait.def:238` `wait_for_any_job (wflags, &pstat)`
  → `jobs.c:3456 wait_for_any_job`; completed-status retention is the
  job table's (`jobs.c` `js.c_reaped` / `procstat` plumbing).
- RB owner: `src/executor/job_builtins.rs:122 execute_wait`,
  `:221 wait_any_background_request`; shallow shim at
  `src/builtins/wait.rs` (returns 127 for numeric operands without
  consulting the job table); job state in `src/jobs/table.rs`.
- Verdict: Rubash bug — `wait -n` races against the child's exit
  notification and reports 0 when the job already exited.
- Severity: **High** — exit status corruption visible to scripts.

### J3 — `wait -p var -n %1 %2` stores a garbage pid and wrong status — RUBASH

- Tests: `jobs5.sub:33-56` (`wait -p wvar -n %2 %3`, `-n %8 $!`,
  `-n -p wpid %1 %2 %3 %4` → the `ok 1/2/3` vs `bad 1/2/3` lines).
- Repro: `.tmpwork/audit/io/j2-waitn-operands.sh`
  - GNU (2/2): `wvar=<pid> want=<pid> status=5` — `wvar` equals `$!`
    of the job that finished first; status is that job's exit code.
  - RB (2/2): `wvar=4260 want=17472 status=4` and
    `wvar=34308 want=29568 status=4` — `wvar` is a wrong/translated
    value (never equals `$!`), status is the *last* job's rather than
    the *first-finishing* job's.
- GNU cite: `builtins/wait.def:277-316` (`wait_for_single_pid` /
  `wait_for_job` with `JWAIT_PERROR`, `-p` assignment of the reaped
  pid) → `jobs.c:2729 wait_for_single_pid`, `jobs.c:3064 wait_for`.
- RB owner: `src/executor/job_builtins.rs:122-246`
  (`execute_wait`/`wait_for_background_operands`/
  `wait_any_background_request`).
- Verdict: Rubash bug — pid-identity mapping (Windows pid ↔ GNU-style
  job pid) and "first finisher" selection are both wrong.
- Severity: **High**.

### J4 — `jobs` output: no current/previous markers, narrower padding, stale dead entries — RUBASH

- Tests: `jobs.tests` `jobs` after `sleep 2 &` fan-out; `jobs7.sub`
  `echo $(jobs)` / `echo $(fg %% ; jobs)`.
- Repro: `.tmpwork/audit/io/j4-jobsfmt.sh` (2/2 runs identical)
  - GNU: `[1]-  Running                    sleep 5 &` /
    `[2]+  Running                    sleep 5 &`
  - RB:  `[1]  Running                sleep 5 &` /
    `[2]  Running                sleep 5 &` — no `-`/`+` markers,
    different column width.
- Suite evidence: RB additionally lists stale `Exit 137 sleep 60 &` /
  `Exit 137 sleep 30 &` entries that GNU reaped before listing
  (`notify_and_cleanup`, `jobs.c:3385`; `delete_job`, `jobs.c:1473`).
- GNU cite: `builtins/jobs.def` `jobs_builtin` → `jobs.c` job listing /
  current-previous tracking (`current_job_index`/`previous_job_index`,
  `jobs.c:1407+` region, `reap_dead_jobs`/`notify_and_cleanup`).
- RB owner: `src/executor/job_builtins.rs:410 background_jobs_output`,
  `:446 ordered_background_jobs`, `src/builtins/jobs.rs:60 execute_jobs`,
  `src/jobs/table.rs`.
- Verdict: Rubash bug — formatting (markers, width) and dead-job GC.
- Severity: **Medium** (cosmetic but observable; stale rows also
  poison `%`-spec resolution).

### J5 — `wait -f %1` / STOP-CONT job — RUBASH + ENV

- Tests: `jobs6.sub` (`set -m; sleep 5 &; ( kill -STOP/CONT ...)&; wait -f %1`).
- Suite: GNU `child1 exit status 0`; RB `child1 exit status 127`.
- Note: `kill -STOP/-CONT` have no POSIX equivalent on Windows — the
  *signal delivery* is environment-limited, but `wait -f %1` returning
  127 means the job-spec/`wait -f` path itself failed.
- GNU cite: `builtins/wait.def:316 wait_for_job (job, wflags, &pstat)`
  with `JWAIT_FORCE`; `jobs.c` `wait_for_job`.
- RB owner: `src/executor/job_builtins.rs:122 execute_wait`.
- Verdict: Rubash bug for the `wait -f` exit status; signal semantics
  environment-bound.
- Severity: **Medium**.

### J6 — `wait` not interrupted by trapped USR1 — ENV-limited / inconclusive

- Tests: `jobs9.sub` (`trap ... USR1; sleep 10 &; ( sleep 2; kill -USR1 $$ )&; wait`).
- Repro: `.tmpwork/audit/io/j6-wait-sig.sh` (2/2)
  - GNU: `got USR1` / `status=138`
  - RB: (no trap output) / `status=0`, and the suite prints
    `wait status not greater than 128`.
- GNU cite: `jobs.c:3064 wait_for` returns >128 when a trapped signal
  interrupts the wait; `trap.c` signal delivery.
- Verdict: **Environment-limited** — Windows has no `kill -USR1`
  delivery to a console process; whether Rubash would interrupt `wait`
  correctly cannot be fully exercised here. Record as inconclusive
  leaning env.
- Severity: Low (untestable on this platform).

### J7 — suite tail (`wait-for-pid` … `forked`) absent — TIMEOUT, not semantic

- Both `gnu.rc` and `rb.rc` are `124`. GNU's out file simply reached
  further before the 40s cutoff (cumulative real `sleep`s + per-spawn
  overhead are larger on the Windows side, and `fg %1` without job
  control is also a candidate stall point).
- Verdict: measurement artifact; do not treat the missing tail as
  proof of additional divergence, but note the jobs diff is a lower
  bound.

---

## redir (54)

### R1 — `exec 0<` does not replace the shell's script input stream — RUBASH

- Tests: `redir1.sub:4,6` driven from `redir.tests:95`
  (`${THIS_SH} < redir1.sub`; the sub does `exec 0< redir2.sub`).
- Repro: `.tmpwork/audit/io/r1-execstdin.sh`
  - GNU: `this is r1-child` / `this is r1-inner` — after `exec 0<`,
    the next command read is from the new fd 0.
  - RB: `this is r1-child` / `BUG: after exec in r1-child` — Rubash
    keeps reading the *original* stream (and also drops the CR bytes
    GNU surfaces in `read` results, see R7).
- GNU cite: shell input is a buffered stream bound to fd 0
  (`input.c` `bash_input`, `parse.y` `yy_read`), so `exec` redirections
  (`execute_cmd.c` permanent `do_redirections`, `redir.c`) transparently
  retarget subsequent command reads.
- RB owner: `src/main.rs:837 run_stdin_script` reads the **process's
  real stdin** via `read_unbuffered_line` (comment at :839-842
  acknowledges the mismatch) and never consults `fd_table` for fd 0;
  `executor.inherit_process_stdin()` (`main.rs:786`).
- Verdict: Rubash bug — stdin-fed script execution is not routed
  through the redirectable fd-0 abstraction.
- Severity: **High** — also poisons every later test in the suite
  (cascading missing output accounts for a large share of the 54 lines).

### R2 — async commands lose the compound's redirected stdin — RUBASH

- Tests: `redir5.sub` area (`{ read line; } &` inside
  `for ... done <<EOF` / redirected compound).
- Repro: `.tmpwork/audit/io/r2-async-stdin.sh`
  - GNU: `got:ab` `got:cd` `got:ef`
  - RB: `got:` `got:` `got:` — each background `read` sees EOF.
- GNU cite: `execute_cmd.c:595 async_redirect_stdin` opens `/dev/null`
  **only** when `should_redir_stdin` (`execute_cmd.c:1589-1591`) —
  i.e. async **and** no explicit stdin redirect **and** `stdin_redir==0`;
  `execute_cmd.c:2835-2841` sets `CMD_STDIN_REDIR` only when
  `(subshell_environment || !job_control) && !stdin_redir`, and a
  redirect on the enclosing compound sets `stdin_redir`
  (`execute_cmd.c:1733-1745`).
- RB owner: `src/executor/compound_exec.rs:89` —
  `child.stdin(Stdio::null())` unconditionally for every `&` child,
  with no inhibition when the enclosing compound redirected stdin and
  no `stdin_redir` equivalent.
- Verdict: Rubash bug — the implicit-`/dev/null` rule is applied
  unconditionally instead of being gated on enclosing stdin state.
- Severity: **High** — silent data loss for background readers.

### R3 — nested fd dup `1>&3` inside `( ( ) 3>&1 ) >/dev/null 2>&1` leaks — RUBASH

- Repro: `.tmpwork/audit/io/r3-fdalias.sh`
  - GNU: `done:0` (inner `echo hello 1>&3` is aliased to the outer
    stdout, which is `/dev/null` → suppressed).
  - RB: `hello` / `done:0` — fd 3 aliasing across the nested subshell
    is lost.
- GNU cite: `redir.c` `do_redirections` applies `r_dupe_output` in
  order at each subshell boundary.
- RB owner: `src/executor/redirection.rs` fd-dup handling
  (`state.fds` propagation across nested subshell redirects).
- Verdict: Rubash bug.
- Severity: **Medium**.

### R4 — high-fd input dup chain `exec 10<infile; exec 0<&10; cat <&10` reads nothing — RUBASH

- Repro: `.tmpwork/audit/io/r5-fdswap.sh`
  - GNU: prints `1 2 3 4` (twice, per script flow).
  - RB: only `---` — the fd-10 input produced no output.
- GNU cite: `redir.c` `r_dupe_input`/`r_input_direction` for
  arbitrary fds.
- RB owner: `src/executor/redirection.rs` input-dup/`fd_table` read
  endpoints (`src/executor/read_io.rs:317 read_virtual_fd_stdin`).
- Verdict: Rubash bug (interacts with R1's virtual-stdin model).
- Severity: **Medium**.

### R5 — `&>` / `&>>` on a *function call* drops stderr — RUBASH

- Repro: `.tmpwork/audit/io/r6b.sh`, refined by `r6c.sh`
  - `func &> fB`: GNU `B: o3 e3`; RB `B: o3` with `e3` on console.
  - Group `{ …; } &> f` and external `&> f` work in RB; only the
    function-call path loses stderr.
- GNU cite: `&>`/`&>>` parse to `r_err_and_out`/`r_append_err_and_out`
  (`command.h:32-35`); applied at `redir.c:899-900` and `:1030`.
- RB owner: parser maps the kinds correctly
  (`src/parser/redirections.rs:455-460` → `CombinedOutput`/
  `CombinedAppend`); the general redirector handles them
  (`src/executor/redirection.rs:295-301`), but the **function-call**
  redirect path (`src/executor/function_env.rs:212-226`) fails to
  route the function's stderr to the combined target.
- Verdict: Rubash bug, function-redirect scope only.
- Severity: **Medium**.

### R6 — redirection word expansion sees the temporary environment — RUBASH

- Tests: `redir.tests` `a=2 echo foo 2>&1 >&$a` family.
- Repro: `.tmpwork/audit/io/r8c.sh`, `r8e.sh`
  - GNU: `a=2 echo foo >&$a` → `$a: Bad file descriptor` (expands to
    the *outer* `a=42`, i.e. fd 42), `status=1`; `b=9 echo bar >&$b` →
    `$b: ambiguous redirect` (temp env not bound → `$b` empty);
    `>&$nosuch` → `ambiguous redirect`.
  - RB: expands `$a`/`$b` to the temp values (`foo` written, `s1=0`);
    `>&$nosuch` → `: No such file or directory` instead of
    `ambiguous redirect`.
- GNU cite: `redir.c:298 redirection_expand` runs against the
  environment in effect **before** the simple command's temp-env
  bindings (assignment tempenv push happens in
  `execute_cmd.c execute_simple_command` after word/redirect
  expansion); empty/unspecified targets report
  `report_ambiguous_redirect` (`redir.c:846-870` region).
- RB owner: `src/executor/redirection.rs` `self.expand_word(&redirect
  .target)` is evaluated with temp-env already visible, and the
  empty-target diagnostic path emits `No such file or directory`
  instead of `ambiguous redirect`.
- Verdict: Rubash bug, two parts: (a) temp-env visibility in redirect
  expansion; (b) wrong diagnostic for empty redirect targets.
- Severity: **Medium**.

### R7 — CRLF bytes in redirected input lost — RUBASH (minor)

- Suite: GNU `read` results from `redir2.sub` retain `\r` bytes
  (`read line1 ab\r`); RB strips them.
- RB owner: `src/main.rs` stdin line reader /
  `src/executor/read_io.rs` normalizes line endings.
- Verdict: Rubash-owned byte-fidelity gap; low severity but contaminates
  several `redir1.sub` output lines.

### R8 — `2>&1 |` writes stderr before stdout (buffering order) — KNOWN LIMITATION

- Repro: `.tmpwork/audit/io/r9-pipeamp.sh` — `|&` itself works;
  `func 2>&1 | cat` prints `to stderr` before `to stdout` under RB.
- This is the documented Windows line-buffered-stderr ordering issue
  (AGENTS.md "STDERR Output Ordering"), not a redirection semantic bug.
- Severity: Low / known.

### R9 — `local -i a` does not see the temp-env value — RUBASH (var scope)

- Repro: `.tmpwork/audit/io/r7-localenv.sh` (`a=4 b=7 foo` where `foo`
  does `local -i a; a+=3`)
  - GNU: `in-func a=7` (local `a` initialized from the temp env's `a=4`).
  - RB: `in-func a=3` (local `a` starts at 0).
- GNU cite: `variables.c` temp-env binding + `declare -i` local
  creation (`builtins/declare.def`); the local inherits the temp value.
- RB owner: `declare`/`local` builtin + function temp-env push in
  `src/executor/function_env.rs`.
- Verdict: Rubash bug (variable-scope, surfaced via redir suite).
- Severity: **Medium**.

---

## vredir (24)

### V1 — `read -u $fd` / `read -u ${var}` yields EOF on a var-allocated fd — RUBASH

- Tests: `vredir2.sub` tail (`while read -r -u ${fd} … done {fd}<$SHELLSFILE`
  → the six `/bin/*` lines missing under RB).
- Repro: `.tmpwork/audit/io/v3-readu.sh`
  - GNU: `read line <&$v` works; `read -r -u $w` prints both lines.
  - RB: `read line <&$v` works; `read -u $w` prints **nothing**.
- GNU cite: `builtins/read.def:369-377` `-u` sets `fd`; the read loop
  consumes that fd via `zread* (fd, …)` (`read.def:734-738`).
- RB owner: `src/executor/read_builtin.rs` (`read_fd` plumbing,
  `read_fd_is_available:2134`) / `src/executor/read_io.rs`
  (`read_virtual_fd_stdin:317`) — the `-u` fd is not routed to the
  fd-table read endpoint that `<&$v` uses.
- Verdict: Rubash bug.
- Severity: **Medium**.

### V2 — fd-number allocation drifts (leaked/extra open fds accumulate) — RUBASH

- Suite evidence: `10`→`11`, `11`→`12`, then `10 11`→`13 14`,
  `12 10`→`15 13`, `12 10`→`17 14` — Rubash allocates progressively
  higher fds where GNU reuses 10/11/12. Isolated repro
  (`v1-fdreuse.sh`, `v1b.sh`) shows matching numbers, so the offset is
  accumulated state across the suite — leaked fds that GNU closes.
- GNU cite: `redir.c` fd allocation (`move_to_high_fd`,
  `fcntl(F_DUPFD_CLOEXEC, 10)`) and `exec {var}>&-` close path.
- RB owner: `src/executor/fd_table` (fd allocator) +
  `redirection.rs` `{var}` close bookkeeping; `vredir4.sub`'s
  `swizzle`/`nameref` cycles leak.
- Verdict: Rubash bug — fd lifecycle leak; the numbers are
  user-observable so it is a real (if cosmetic-adjacent) divergence.
- Severity: **Medium**.

---

## read (44)

### RD1 — `read -d`/`-n` do not consume from the shared input stream — RUBASH

- Tests: `read.tests` pipe-sharing cases.
- Repro: `.tmpwork/audit/io/rd1-consume.sh`
  - GNU: `read -d '|' a` leaves `xyz` for `cat`; `read -n 3` leaves
    `defg`.
  - RB: `cat -` re-reads the *entire* pipe (`abcdefg|xyz`, `abcdefg`) —
    the reads were satisfied from a buffer copy, not the shared fd.
- GNU cite: `builtins/read.def` reads byte-at-a-time from `fd`
  (`zread`/`zreadn`, `read.def:734-738,1186-1190`) so post-read bytes
  remain for the next consumer.
- RB owner: `src/executor/read_io.rs:32 read_input_for_command` and
  the `read_virtual_fd_stdin`/`read_inherited_process_stdin` paths —
  the pipe is slurped/buffered per-command instead of consumed
  incrementally.
- Verdict: Rubash bug.
- Severity: **High** — stream-position corruption between commands.

### RD2 — `read -d` delimiter treated as character, not byte — RUBASH

- Repro: `.tmpwork/audit/io/rd2-delim-byte.sh`
  (`IFS= read -rd $'\200'` over `\200`-delimited input)
  - GNU: `<winter> <$'spring\375'> <summer> <automn>` — splits on raw
    byte 0x80.
  - RB: `<'spring'> <\376> <'summer'> <'automn> <'>` — the UTF-8-ish
    `$'\200'` delimiter never matches byte 0x80 and the byte stream is
    mangled (lossy UTF-8 handling of `\375`).
- GNU cite: `builtins/read.def:384-…` `-d` sets `delim` to the first
  **byte** (`set_eol_delim`/`eol_delim`, `read.def:126-127,242`); input
  is byte-oriented (`zreadc`).
- RB owner: `src/executor/read_builtin.rs` `-d` parse +
  `read_io.rs`/`read_split.rs` — char (`char`) vs byte semantics.
- Verdict: Rubash bug.
- Severity: **Medium**.

### RD3 — `read -t` failure on `/dev/tty` poisons subsequent reads — RUBASH

- Repros: `.tmpwork/audit/io/rd3c.sh`, `rd3d.sh`, `rd3e.sh`
  - `rd3c.sh` GNU: `t1=142 t2=142 t3=142 t4=1 t05=[abcde]`
  - `rd3c.sh` RB:  `t1=1 t2=1 t3=142 t4=1 t05=[]` — after the
    `/dev/tty` timeout/failure, later `read -t` calls and even a plain
    pipe read (`t05`) return failure/empty.
  - `rd3d.sh` (isolated invalid-timeout): later pipe read still works
    → the contamination specifically follows the `/dev/tty` path.
- GNU cite: `builtins/read.def` timeout handling (`read_timeout->fd`,
  `check_read_timeout`, `read.def:153,521`); a failed `-t` leaves no
  global residue.
- RB owner: `src/executor/read_builtin.rs` timeout path +
  `src/executor/read_io.rs` — failed `-t` on a missing device leaves
  the input layer in a state where subsequent reads return EOF.
- Verdict: Rubash bug (state contamination); the `/dev/tty` absence
  itself is environmental, but the *subsequent* corruption is not.
- Severity: **Medium**.

### RD4 — `IFS=: read -a A` drops empty fields — RUBASH

- Repro: `.tmpwork/audit/io/rd4-empty-array.sh`
  - GNU: `len=3` (`[] [] []`) for `IFS=: read -a A <<< ":::"`.
  - RB: `len=0`.
- GNU cite: `read.def` field splitting keeps empty fields when the IFS
  char is non-whitespace (same rule as `field_split`/`list_string` in
  `subst.c`); `strip_trailing_ifs_whitespace` at `read.def:1001,1106,1119`.
- RB owner: `src/executor/read_split.rs`
  (`split_read_field_ranges:45`, `read_scalar_fields*:162-178`).
- Verdict: Rubash bug.
- Severity: **Medium**.

### RD5 — trailing non-blank IFS chars not stripped from last field — RUBASH

- Repro: `.tmpwork/audit/io/rd7-ifs-trail.sh`
  (`IFS=$'\t\r\f\v'` over `  line\tb \t\r\f\v\n`)
  - GNU: `var2="b "` — trailing `\t\r\f\v` consumed as IFS terminators.
  - RB: `var2="b \t\r\f\v"` — trailing IFS bytes retained.
- GNU cite: `read.def:1001,1106` `strip_trailing_ifs_whitespace` (plus
  the non-whitespace-IFS delimiter consumption in field splitting).
- RB owner: `src/executor/read_split.rs:126 trim_trailing_unescaped_ifs`.
- Verdict: Rubash bug.
- Severity: **Medium**.

---

## coproc (6)

### C1 — diagnostic prefix `./coproc.tests:` vs `bash:` — ENV/formatting

- Suite: `./coproc.tests: line 53: xcase: command not found` (GNU) vs
  `bash: line 53: …` (RB). Repro `c2-diagprefix.sh` confirms only the
  argv[0]/script-name component differs
  (`/mnt/d/…/c2-diagprefix.sh` vs `.tmpwork/audit/io/c2-diagprefix.sh`).
- Verdict: environment/path formatting (invocation name), not a
  coprocess semantic. Coproc fds themselves match (`63 60` both).
- Severity: Low.

### C2 — `cat /etc/passwd | grep root` → missing `/etc/passwd` — ENV

- Suite: GNU `root`; RB `/usr/bin/cat: /etc/passwd: No such file or
  directory` (merged via `exec 2>&1`). No `/etc/passwd` on Windows.
- Verdict: environment.
- Severity: n/a.

---

## posixpipe (1)

### P1 — suite-name-keyed fast path prints literal `4` — RUBASH (text-layer shortcut)

- Suite: `time -p … | …` pipeline prints `5` under GNU; RB prints `4`.
- Repro: `.tmpwork/audit/io/pp1.sh` — GNU `5`, RB `4`; `pp2.sh` shows
  RB emits a bare `4` with no timing formatting.
- RB owner: **hardcoded suite-name shortcut** —
  `src/executor/external_finish.rs:296-327`,
  `src/executor/pipeline_exec.rs:325-331`,
  `src/executor/external_inner.rs:384-392,466-475` print a literal `4`
  and skip real pipeline processing when `__RUBASH_SCRIPT_NAME` ends
  with `posixpipe.tests`.
- GNU cite: `execute_cmd.c` `time` pipeline (`CMD_TIME_PIPELINE`) +
  `time`-reserved-word timing output.
- Verdict: Rubash bug *class*: a text-layer/suite-specific fast path —
  exactly the rubash#117 pattern. Do **not** extend the predicate;
  implement real `time` pipeline semantics and delete the shortcut.
- Severity: **High** (masked semantics — real behavior unknown).

---

## glob (67)

### G1 — fixture creation fails for Windows-invalid filenames — ENV

- Tests: `glob.tests:54-59` (`touch '*abc.c'`), `:125-129`
  (`mkdir 'a\*b'`), `glob8.sub:26` (`touch 'a*b' 'a\*b'`), the `qwe/`
  dirs.
- Evidence: `rb.err` `cannot touch '*abc.c'` / `Invalid argument`;
  diff lines `*abc.c`→`\**.c`, `a*b/ooo`→`a*b/*`, `a\*b`→`a\*b*`.
- Verdict: environment (NTFS-invalid names). Do not fix in Rubash.
- Severity: n/a.

### G2 — glob result ordering does not match GNU's comparator — RUBASH

- Suite: `.a a .aa aa .b b .bb bb` (GNU, dotglob interleaved) vs
  `.a .aa .b .bb a aa b bb` (RB, dot-prefix byte sort);
  `mailcheck.o make_cmd.o …` vs RB's `make_cmd.o mailcheck.o …`;
  `b bb bcd bdir Beware` vs `Beware b bb bcd bdir` (case ordering);
  `aa ab ac` vs `ac ab aa` (GLOBSORT-driven reversal).
- GNU cite: `pathexp.c:459,486 sh_sortglob` → `pathexp.c:761
  globsort_namecmp` → `lib/sh/stringvec.c:152 strvec_posixcmp`
  (POSIX-locale collating order; `GLOBSORT` comparators
  `pathexp.c:761-842`), `stringvec.c:187 strvec_sort`.
- RB owner: `src/executor/glob.rs:1783-1789` — Rust `.sort()` (byte
  order) instead of the POSIX/strcoll comparator; `GLOBSORT`
  comparator semantics only partially ported.
- Verdict: Rubash bug — comparator semantics (case, dot, `_`,
  GLOBSORT keys) diverge from `strvec_posixcmp`/`strcoll`.
- Severity: **Medium** (ordering is user-observable).

### G3 — `[qwe\/qwe]` under `nullglob`: GNU removes, RB echoes literal — RUBASH

- Tests: `glob7.sub:9-11` (POSIX 2.13.3: slash inside a bracket expr;
  here with `\/` escaped).
- Suite: GNU `4:`/`6:` empty (pattern unmatched → nullglob removes);
  RB prints `[qwe/qwe]` / `[qwe/]` literally.
- GNU cite: `pathexp.c` bracket-expression scan + `glob.c` matching;
  `nullglob` removes unmatched patterns
  (`pathexp.c` `globname_is_directory`/`expand` path).
- RB owner: `src/executor/glob.rs` bracket-expr handling — RB treats
  `\/` inside `[…]` as disqualifying the pattern (literal output)
  where GNU still treats it as a glob pattern.
- Verdict: Rubash bug (GNU is the spec; GNU emitted empty).
- Severity: **Low-Medium**.

---

## globstar (4)

### GS1 — `**` follows directory symlinks — RUBASH

- Tests: `globstar3.sub` (`mkdir a b; ln -s a c; shopt -s globstar;
  echo **`).
- Suite: GNU `a a/aa a/ab b b/bb b/bc c` — symlink `c` listed but not
  traversed; RB adds `c/aa c/ab`. Same for `**/*b` (`c/ab` extra).
- GNU cite: `glob.c` `glob_vector`/`glob_dir_to_array` `**` recursion —
  symlinked directories are not descended (dirent `lstat` vs `stat`
  handling in the `**` walk).
- RB owner: `src/executor/glob.rs` globstar recursion — follows
  `symlink_metadata`/`metadata` without GNU's no-descend-symlink rule.
- Verdict: Rubash bug.
- Severity: **Medium**.

---

## shopt (13)

### S1 — non-GNU options enumerated: `completion_strip_exe`, `igncr`, `restricted` — RUBASH

- Suite: RB adds `shopt -u completion_strip_exe`,
  `completion_strip_exe off`, `set +o igncr`, `set +o restricted`,
  `igncr off`, `restricted off` lines (13 diff lines).
- GNU cite: `builtins/shopt.def` `shopt_vars[]` (the `restricted_shell`
  entry is `:262`, guarded by `set_restricted_shell` `:750-757` which
  prevents modification) and `builtins/set.def:194-241 o_options[]` —
  **no** `igncr`, `restricted`, or `completion_strip_exe` upstream;
  `igncr` is a Cygwin-only configure feature (`configure:24090`).
- RB owner: `src/builtins/shopt/support.rs:23`
  (`completion_strip_exe`), `:60` (`restricted_shell`);
  `src/builtins/set/options.rs:53` (`igncr`), `:117` (`restricted`).
- Verdict: Rubash bug — option tables enumerate names GNU 5.3 does
  not have, so `shopt`/`set -o` dumps differ. (Keeping the options
  internally may be intended Windows behavior, but the enumerable
  surface must match GNU.)
- Severity: **Low-Medium**.

---

## test (13)

### T1 — `-ef` does not detect hard links — RUBASH

- Tests: `test.tests:269-271` (`ln /tmp/abc /tmp/ghi; t /tmp/abc -ef
  /tmp/ghi` → GNU `0`, RB `1`).
- GNU cite: `test.c:330 filecomp` — `EF` compares `st_dev`+`st_ino`
  (`test.c:443` `case 'f'`).
- RB owner: `src/builtins/test.rs:789-797` — compares canonicalized
  paths (or metadata equality that misses NTFS hard-link identity).
- Caveat: NTFS hard links work and `ln` produced no error in `rb.err`,
  so this is rubash-owned, but verify the `ln` actually succeeded in a
  rerun before fixing.
- Severity: **Medium**.

### T2 — `-N` semantics (and parse) differ — RUBASH

- Suite: `t -N /tmp/abc` GNU `1`, RB `0`; a focused probe also showed
  `test: -N: binary operator expected` in some arg shapes.
- GNU cite: `test.c:569 case 'N'` — file modified more recently than
  accessed (`filecomp`-family stat comparison).
- RB owner: `src/builtins/test.rs:686-697` (`-N` impl compares mtime
  vs atime), plus the argument-classification path that can raise
  `binary operator expected`.
- Severity: **Medium**.

### T3 — tty/device and mode-bit predicates — ENV

- `t -c /dev/tty` (GNU 0 / RB 1), `t -t 0 < /dev/tty` (redirect fails —
  `rb.err` `line 118: No such file or directory`), `t -g/-u` on
  `chmod g+s/u+s` files (GNU 0 / RB 1), `t -x` after `chmod u-x`
  (GNU 1 / RB 0).
- Verdict: environment — no `/dev/tty`, no POSIX mode bits on Windows.
  `-x` may warrant a follow-up if Rubash intends to fake mode bits,
  but chmod on Windows genuinely cannot clear exec permission.
- Severity: n/a (env).

---

## braces (1)

### B1 — stray `'` line inside `${a#'$(\'}`-family output — RUBASH (minor)

- Tests: `braces.tests:55-60` (`echo "${a#'$(\'}"`, `${a-…}`, `${a+…}`,
  and the `aaaa'$(aaaa'…` variants).
- Suite: GNU emits four `4` lines; RB emits `4 ' 4 4` — one expansion
  produces a bare `'`.
- GNU cite: `subst.c:9777 parameter_brace_expand` /
  `subst.c:7663 parameter_brace_expand_word` +
  `parse.y:3877 parse_matched_pair` — embedded `'$(…'\'` quoting inside
  `${…}` is handled by the matched-pair scanner, not by brace code.
- RB owner: parameter-expansion word/quote handling
  (`src/executor` param expansion + `src/expand`) — an isolated repro
  (`br2.sh`) diverges further from the suite context, so the precise
  trigger line is unconfirmed.
- Verdict: Rubash bug, narrow (quote/quote-removal edge inside
  `${var#…}`/`${var±…}`); brace expansion itself matches GNU.
- Severity: **Low**.

---

## ifs-posix (1) — PERFORMANCE, not semantics

- GNU: `# tests 6856 passed 6856 failed 0`.
- RB: **zero stdout** — a direct run under `timeout 45` was killed at
  45s (`rc=124`) before reaching the summary; GNU completes the same
  file within the harness window.
- Verdict: Rubash throughput problem in the 6856-iteration
  nested-loop/`split`-function workload (per-call overhead; possibly
  amplified by Windows process spawn if `split` spawns). Cannot
  distinguish slow-vs-hang without a longer run — mark
  **inconclusive/perf**, not a correctness diff.
- RB owner: executor function-call / loop hot path (not isolated to a
  single file).
- Severity: **Medium** (suite-level timeout).

---

## intl (2)

### I1 — error message prints raw/lossy filename instead of `$'…'` quoting — RUBASH (cosmetic)

- Tests: `unicode3.sub` (`cd "$payload"` where payload is non-UTF-8
  bytes; output merged via `2>&1`).
- Suite: GNU `cd: $'5\247@3\231+\306S8\237\242\352\263': No such file
  or directory`; RB prints the raw bytes (lossy UTF-8 on output).
- GNU cite: `general.c:1019 printable_filename` → `ansic_quote`
  (`general.c:1024`) — non-printing filenames are `$'…'`-escaped in
  diagnostics.
- RB owner: `cd` builtin diagnostic path (`src/builtins/cd.rs`) /
  diagnostic-prefix plumbing — no printable-filename quoting.
- Verdict: Rubash bug, cosmetic.
- Severity: **Low**.

---

## set-x (5)

### X1 — `BASH_XTRACEFD` not implemented — RUBASH (missing feature)

- Tests: `set-x1.sub` (`BASH_XTRACEFD=4; exec 4>$TRACEFILE; set -x;
  echo 1..4; unset BASH_XTRACEFD; cat $TRACEFILE`).
- Suite: GNU prints the trace lines (`+ echo 1` … `+ unset
  BASH_XTRACEFD`) from the tracefile; RB's tracefile is empty — xtrace
  always goes to stderr.
- GNU cite: `variables.c:699` `find_variable("BASH_XTRACEFD")` and the
  special-variable hook `variables.c:5761 { "BASH_XTRACEFD",
  sv_xtracefd }`; xtrace writes to `xtrace_fd`.
- RB owner: xtrace emission writes only stderr —
  `src/executor/command_dispatch.rs:21-53` and
  `src/executor/command_prepare.rs:204+`; no `BASH_XTRACEFD` variable
  hook exists (`grep BASH_XTRACEFD src/` → no hits).
- Verdict: Rubash bug — missing special variable.
- Severity: **Medium**.

---

## Summary table

| Suite | Rubash-owned | Environment | Inconclusive/perf | Severity peak |
|---|---|---|---|---|
| jobs | wait -n/-p semantics, jobs markers+GC, wait -f status | /bin/sh missing, USR1 delivery | tail truncation (both rc=124) | High |
| redir | exec-0< stream, async stdin, fd-dup nesting, fn `&>`/`&>>`, temp-env redir, ambiguous-redirect diag, CRLF | — | — | High |
| vredir | `read -u` EOF, fd-number drift | — | — | Medium |
| read | -d/-n non-consuming, -d byte vs char, -t poisoning, `-a` empty fields, trailing IFS | /dev/tty absence | — | High |
| coproc | — | diag prefix, /etc/passwd | — | Low |
| posixpipe | suite-name fast path (rubash#117 class) | — | — | High |
| glob | sort comparator, `[…\/…]`+nullglob | Windows-invalid filenames | — | Medium |
| globstar | symlink traversal | — | — | Medium |
| shopt | extra enumerated options | — | — | Low-Med |
| test | -ef, -N | tty/mode-bit predicates | — | Medium |
| braces | stray `'` in `${a±…'$(…'…}` edge | — | trigger unconfirmed | Low |
| ifs-posix | — | — | >45s timeout (perf) | Medium |
| intl | `$'…'` diagnostic quoting | — | — | Low |
| set-x | BASH_XTRACEFD unimplemented | — | — | Medium |

## Notes for the fix pass

- `posixpipe`'s literal-`4` shortcut is a rubash#117-style suite
  admission; delete rather than extend.
- `jobs` baseline is timeout-bound on both sides — after fixes, rerun
  with a longer per-suite timeout to measure the tail.
- Repro inventory under `.tmpwork/audit/io/`: `j1`–`j6`, `r1`–`r9`,
  `v1`–`v3`, `rd1`–`rd7`, `c1`–`c2`, `pp1`–`pp2`, `t1`, `br1`–`br2`.
