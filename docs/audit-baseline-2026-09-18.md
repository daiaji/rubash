# Audit Baseline — 2026-09-18 (post-merge full 83-suite true-baseline)

## Provenance

- **Commit**: `2494bc57` (code-identical to `c282a850`; the only delta is README docs)
- **Binary**: `target/debug/rubash.exe` built from `c282a850` code
- **Harness**: `scripts/true-baseline.sh` (canonical), invoked with no args = all 83 suites
  - `MSYS_NO_PATHCONV=1 wsl bash /mnt/d/repo/rubash/scripts/true-baseline.sh`
- **GNU oracle**: `/usr/local/bin/bash` — owner-compiled **5.3.0** (verified by harness)
- **Env**: `LC_ALL=en_US.UTF-8` (harness locale-gen), `__RUBASH_NO_UPSTREAM_SCRIPTS=1` on the
  Rubash side (real executor, no canned handlers), stdin `</dev/null`, `timeout -k 5 40`,
  cwd `target/issue-suites/results/bash-tests-rw`, WinuxCmd 1.0.6 not on PATH for the RB side
  (harness limitation — od/expr format diffs are NOT rubash bugs)
- **Ledger semantics**: counts stdout diffs only; stderr captured separately per suite at
  `target/issue-suites/results/true-baseline/<suite>/{gnu,rb}.{out,err}`
- **Artifacts**: `target/issue-suites/results/true-baseline/` + `true-baseline-ledger.log`

## Headline numbers

| Bucket | Suites |
|---|---|
| PASS (0 diff) | 42 |
| DIFF 1-50 | 26 |
| DIFF 51-250 | 14 |
| DIFF 251+ | 1 |
| **Total** | **1833 lines** |

## Per-suite ledger (stdout diff lines)

```
alias 68        appendop 0      arith-for 0     arith 53        array 137
assoc 171       attr 0          braces 1        builtins 0      casemod 0
case 0          complete 0      comsub2 48      comsub-eof 0    comsub-posix 11
comsub 17       cond 19         coproc 6        cprint 0        dbg-support2 0
dbg-support 0   dstack2 0       dstack 0        dynvar 0        errors 129
exportfunc 0    exp 6           extglob2 0      extglob3 0      extglob 32
func 0          getopts 0       glob-bracket 0  globstar 4      glob 67
heredoc 0       herestr 0       histexp 79      history 121     ifs-posix 1
ifs 0           intl 2          invert 0        invocation 2    iquote 76
jobs 64         lastpipe 0      mapfile 0       more-exp 6      nameref 281
new-exp 51      nquote1 0       nquote2 0       nquote3 0       nquote4 0
nquote5 0       nquote 12       parser 0        posix2 3        posixexp2 0
posixexp 7      posixpat 0      posixpipe 1     precedence 0    printf 0
procsub 11      quotearray 64   quote 0         read 44         redir 54
rhs-exp 0       rsh 0           set-e 1         set-x 5         shopt 13
strip 0         test 13         tilde2 0        tilde 0         trap 5
type 39         varenv 85       vredir 24
```

## Known measurement caveats

- **trap (5)**: SIGCHLD/`wait` section is racy — same binary measured 5↔7 on
  consecutive runs. Do not treat trap as a hard regression.
- **arith (53)**: stdout section contains `$RANDOM`-dependent tests; observed
  53↔57 across identical runs. stderr classification was fixed in `8c7bbf92`.
- **errors (129)**: bulk of the diff is RB continuing to execute after a point
  where GNU aborts (missing-fatal semantics), not wording drift. Verified
  pre-merge binary produced 157 — this predates the 9/17 merge.
- **histexp (79)**: identical pre/post-merge (103 in the isolated rerun
  context) — predates the merge.
- **intl (2)**: prior 1209 figure was missing-locale noise, not a semantic gap.
- **stderr is not counted** in this ledger; several suites have additional
  stderr-only divergences (see per-suite `.err` artifacts).

## Audit instructions

Each audit must: (1) cite the owning GNU C function in `third_party/bash/`
before claiming a root cause; (2) reproduce via a script file through both
`target/debug/rubash.exe` and `wsl /usr/local/bin/bash`; (3) distinguish
rubash-owned diffs from WinuxCmd/environment-owned ones; (4) flag text-layer
word-level fast paths instead of patching them (rubash#117).
