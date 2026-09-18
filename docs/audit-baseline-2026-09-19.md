# Audit Baseline — 2026-09-19 (post-merge full 83-suite true-baseline)

Supersedes `docs/audit-baseline-2026-09-18.md` (kept as the pre-merge record).

## Provenance

- **Commit**: `652c1042` (master tip; includes the nameref/RC-3 audit batches and
  the #116/#118/#119/#121/#122/#129/#130/#109/#83 issue fixes merged since
  `2494bc57`)
- **Binary**: `target/debug/rubash.exe` built from `652c1042`
- **Harness**: `scripts/true-baseline.sh` (canonical), invoked with no args = all 83 suites
  - `MSYS_NO_PATHCONV=1 wsl bash /mnt/d/repo/rubash/scripts/true-baseline.sh`
- **GNU oracle**: `/usr/local/bin/bash` — owner-compiled **5.3.0** (verified by harness)
- **Env**: same frozen methodology as the 09-18 baseline (CR-stripped copies in
  `bash-tests-rw`, `__RUBASH_NO_UPSTREAM_SCRIPTS=1`, stdin `</dev/null`,
  `timeout -k 5 40`, stdout-only ledger, stderr captured per suite)
- **Artifacts**: `target/issue-suites/results/true-baseline/` +
  `true-baseline-ledger.log`

## Headline numbers

| Bucket | Suites |
|---|---|
| PASS (0 diff) | 45 |
| DIFF 1-50 | 26 |
| DIFF 51-250 | 12 |
| DIFF 251+ | 0 |
| **Total** | **1247 lines** |

Delta vs 2026-09-18 baseline (`2494bc57`, 1833 lines, 42 zero-diff):
**−586 lines, +3 zero-diff suites.**

## Per-suite ledger (stdout diff lines)

```
alias 69        appendop 0      arith-for 0     arith 0         array 112
assoc 142       attr 0          braces 1        builtins 0      casemod 0
case 0          complete 0      comsub2 44      comsub-eof 0    comsub-posix 11
comsub 17       cond 19         coproc 6        cprint 0        dbg-support2 0
dbg-support 0   dstack2 0       dstack 0        dynvar 0        errors 3
exportfunc 0    exp 8           extglob2 0      extglob3 0      extglob 32
func 0          getopts 0       glob-bracket 0  globstar 4      glob 67
heredoc 0       herestr 0       histexp 79      history 119     ifs-posix 1
ifs 0           intl 2          invert 0        invocation 2    iquote 58
jobs 63         lastpipe 0      mapfile 0       more-exp 1      nameref 7
new-exp 59      nquote1 0       nquote2 0       nquote3 0       nquote4 0
nquote5 0       nquote 12       parser 0        posix2 3        posixexp2 0
posixexp 7      posixpat 0      posixpipe 1     precedence 0    printf 0
procsub 13      quotearray 56   quote 0         read 44         redir 55
rhs-exp 0       rsh 0           set-e 2         set-x 5         shopt 0
strip 0         test 13         tilde2 0        tilde 0         trap 0
type 6          varenv 80       vredir 24
```

## Movement vs 2026-09-18

Improved: nameref 281→7, errors 129→3, arith 53→0, type 39→6, assoc 171→142,
array 137→112, iquote 76→58, quotearray 64→56, comsub2 48→44, shopt 13→0,
trap 5→0, history 121→119, more-exp 6→1, varenv 85→80, jobs 64→63.

Increased (needs audit, all small): new-exp 51→59 (+8), exp 6→8 (+2),
procsub 11→13 (+2), alias 68→69 (+1), redir 54→55 (+1), set-e 1→2 (+1).

## Known measurement caveats

- **One GNU-side process was Killed** by `timeout -k 5 40` during the run
  (harness line 102). Checked the largest positive delta (`new-exp` +8): its
  `gnu.out` ends at a normal `expect` diagnostic line and the diff content is
  real `declare -ai` formatting divergence, not truncation. Treat the six
  small increases above as real until individually audited, not as kill noise.
- **arith (0)**: contains `$RANDOM` histogram tests; prior baseline observed
  53↔57 across identical runs. A zero reading is plausible after the expr.c
  error-model fix (`bcb381c4`) but is also inside the noise band of this suite —
  confirm on the next run before retiring the suite.
- **trap (0)**: previously racy at 5↔7 (SIGCHLD/`wait` section). This run's
  zero is consistent with the ERR/DEBUG trap ownership fix (`6e4066dd`); keep
  the flake note in mind on future runs.
- **history (119)**: still contains the known baseline artifact — interactive
  `bash --norc -i` hangs on non-tty piped stdin under WSL, truncating gnu.out
  at history4; the `history -a` line-count diff can also be a stale
  `tmp/newhistory` left by SIGKILL skipping `trap rm`. True residual per the
  09-18 audit: H1+H2 only.
- **jobs (63)**: both sides hit the 40s harness timeout; the figure is a lower
  bound, not a full account.
- **glob (67)**: audited during the #121 merge — environment-bound
  (Windows filename/locale), not a rubash semantic gap.
- **stderr is not counted** in this ledger; several suites have additional
  stderr-only divergences (see per-suite `.err` artifacts).

## Residual cluster map (audit backlog → open issues)

- **#117 architecture umbrella** — word-level comsub/substitution fast paths:
  drives comsub (17), comsub2 (44), comsub-posix (11), iquote (58), new-exp
  (59), quotearray (56) residuals. `"$(echo x '|' y)"` and
  `"a=$(echo x '|' y)"` reproducers were fixed in `cfaa9125`/`64579580`;
  remaining work is the RC-4/RC-5 scanner convergence.
- **#77** — `declare -A`/`-ai` print echo (assoc family; in-flight in
  `D:/repo/rubash-i77` worktree, uncommitted).
- **assoc (142), array (112)** — audit reports `docs/audit-2026-09-18/assoc.md`
  and `array-quotearray.md`; part of the drop already landed (RC-3 consumption,
  compound storage carriers).
- **varenv (80), histexp (79), alias (69)** — see
  `docs/audit-2026-09-18/varenv-alias-arith.md` and `history-histexp.md`.
- **#62** — compatibility-attribution metadata; this document is its input.

## Audit instructions (unchanged)

Each audit must: (1) cite the owning GNU C function in `third_party/bash/`
before claiming a root cause; (2) reproduce via a script file through both
`target/debug/rubash.exe` and `wsl /usr/local/bin/bash`; (3) distinguish
rubash-owned diffs from WinuxCmd/environment-owned ones; (4) flag text-layer
word-level fast paths instead of patching them (rubash#117).
