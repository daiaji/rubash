# Rubash

A GNU Bash-compatible shell implementation written in Rust.

[中文](README.zh-CN.md)

[![CI](https://github.com/unixwin/rubash/actions/workflows/ci.yml/badge.svg)](https://github.com/unixwin/rubash/actions/workflows/ci.yml)
[![Rust Version](https://img.shields.io/badge/rust-1.70+-blue)](https://www.rust-lang.org)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue)](LICENSE)

## What Is Rubash

Rubash is a from-scratch reimplementation of GNU Bash in Rust — lexer, parser, expansion engine, executor, builtins, and all. It targets byte-level compatibility with GNU Bash 5.3.0 and runs on Windows natively.

< HEAD
**Current status**: 45 out of 83 GNU Bash upstream test suites pass with zero difference. Total remaining diff across all 83 suites is 1247 lines, down from 3427 on Sep 9 (−64%). (The previously reported `intl`=1209 was missing-locale environment noise; the harness now generates `en_US.UTF-8`, and `intl` measures 2 lines.) Full details in [`docs/COMPATIBILITY-STATUS.md`](docs/COMPATIBILITY-STATUS.md).
=======
**Current status**: 46 out of 83 GNU Bash upstream test suites pass with zero difference. Total remaining diff across all 83 suites is 1105 lines, down from 3427 on Sep 9 (−68%). (The previously reported `intl`=1209 was missing-locale environment noise; the harness now generates `en_US.UTF-8`, and `intl` measures 2 lines.) Full details in [`docs/COMPATIBILITY-STATUS.md`](docs/COMPATIBILITY-STATUS.md).


## Compatibility at a Glance

```
GNU Bash 5.3.0 test suite — 83 files, true-baseline measurement
< HEAD
(ledger: 2026-09-19 full re-run, master `652c1042`)

  PASS (0 diff):   45 suites  ██████████████████░░░░░░░░░░░░░  54%
  DIFF (1-50):     26 suites  ██████████░░░░░░░░░░░░░░░░░░░░░  31%
  DIFF (51-250):   12 suites  █████░░░░░░░░░░░░░░░░░░░░░░░░░  14%
  DIFF (251+):      0 suites  ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░   0%
  ────────────────────────────────────────────────────────────────
  Total diff:      1247 lines (intl=2 after harness locale fix)
  Was 3427 on Sep 9 → −64% in 10 days
=======
(ledger: 2026-09-19 full re-run, master `652c1042`; assoc re-verified 0-diff 2026-09-20, `4883ad0b`)

  PASS (0 diff):   46 suites  ██████████████████░░░░░░░░░░░░░  55%
  DIFF (1-50):     26 suites  ██████████░░░░░░░░░░░░░░░░░░░░░  31%
  DIFF (51-250):   11 suites  ████░░░░░░░░░░░░░░░░░░░░░░░░░░  13%
  DIFF (251+):      0 suites  ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░   0%
  ────────────────────────────────────────────────────────────────
  Total diff:      1105 lines (intl=2 after harness locale fix)
  Was 3427 on Sep 9 → −68% in 11 days

```

### Fully passing suites (zero diff)

< HEAD
`appendop` `arith` `arith-for` `attr` `builtins` `case` `casemod` `comsub-eof` `complete` `cprint` `dbg-support` `dbg-support2` `dstack` `dstack2` `dynvar` `exportfunc` `extglob2` `extglob3` `func` `getopts` `glob-bracket` `heredoc` `herestr` `ifs` `invert` `lastpipe` `mapfile` `nquote1` `nquote2` `nquote3` `nquote4` `nquote5` `parser` `posixexp2` `posixpat` `precedence` `printf` `quote` `rhs-exp` `rsh` `shopt` `strip` `tilde` `tilde2` `trap`
=======
`appendop` `arith` `arith-for` `attr` `assoc` `builtins` `case` `casemod` `comsub-eof` `complete` `cprint` `dbg-support` `dbg-support2` `dstack` `dstack2` `dynvar` `exportfunc` `extglob2` `extglob3` `func` `getopts` `glob-bracket` `heredoc` `herestr` `ifs` `invert` `lastpipe` `mapfile` `nquote1` `nquote2` `nquote3` `nquote4` `nquote5` `parser` `posixexp2` `posixpat` `precedence` `printf` `quote` `rhs-exp` `rsh` `shopt` `strip` `tilde` `tilde2` `trap`


### Major recent fixes (Sep 2026)

| Area | Before → After | What changed |
|------|----------------|-------------|
| **dbg-support** | 635 → 0 | AND-list dual fire, source-scope trap inheritance, `{` regression |
| **rsh** | 194 → 0 | `set +o restricted` silent lift, full restricted-shell enforcement |
| **invocation** | 14 → 0 | `BASH_ARGV0`, long options, `--pretty-print`, `-o`/`-O` prologs |
| **trap** | 3 → ~5 (racy) | ERR line binding, SIGCHLD queue, background child trap isolation; residual diff is SIGCHLD/`wait` timing, nondeterministic |
| **func** | 58 → 0 | POSIX funcname rules, AST printer, special-builtin precedence |
| **complete** | 115 → 0 | Multi-operand compspec registration |
| **history** | 190 → 127 | `history -d start-end` range deletion (GNU 5.3 feature) |
| **globstar** | 182 → 4 | Multiplicity fix, command lookup cache, `checkhash` bypass (residual is WinuxCmd `ls` ordering, not rubash) |
| **array/assoc** | 444+358 → 148+187 | Compound assignment quote grouping, `"$@"`/`$0` expansion, arithmetic subscript side effects (`count++`) |
| **compound-array quoting** | intl 1194→1192 fails | Escaped `\"`/`\'`/`\\`/`` \` `` preserved through compound RHS; `quote_array_value` double-escape fix; data/syntax quote distinction in unquoted assignment RHS (`EChar=${Array[0x0022]}`) |
| **signals** | BSD table → Linux table | USR1=10, CHLD=17, RTMIN=34, matching GNU 5.3.0 WSL contract |

### Fixed today (Sep 16, 2026 — PR #111 + local batch landed on master)

| Area | Before → After | What changed |
|------|----------------|-------------|
| **CRLF scripts (niubash #106)** | v1.1.2 regression → fixed | `\r\n` is stripped as a line terminator at lexer line-split time (main loop + heredoc bodies, so `<<EOF` delimiters match again); a lone `\r` not followed by `\n` is still ordinary word text, keeping the GNU-fidelity case intact |
| **`-c` option parsing (niubash #107)** | broken → GNU-conformant | `-c` takes the *first non-option argument* as the command string; `bash -c -l 'script'` works and unblocks AI-agent/invoker tooling; bare `bash -c` keeps GNU's usage error (rc 2) |
| **`type` output capture (niubash #108)** | leaked → captured | `$(type -t ls)` now returns `file` instead of printing to the process stdout and assigning an empty string |
| **nameref** | 558 → 226 diff lines (run-83 check) | Indirect expansion, unset propagation and scoping fixes from the local batch |
| **history** | 323 → 250 diff lines (run-83 check) | Nested same-shell script output ordering and IFS isolation fixes from the local batch |
| **trap EXIT in `$( )` / `printf` exit path** | debug leftovers stripped | WIP `[DEBUG]` eprintln instrumentation removed before landing |

### What Rubash can already run

- **bashdb** — core debugger loop (list, step, next, where, continue, quit) works under rubash
- **Complex Bash scripts** — arrays, associative arrays, arithmetic, conditionals, namerefs, command substitution, brace expansion, process substitution, coproc, `eval`, `trap`, `source`
- **GNU Bash test suite** — 83 upstream test files with automated diff measurement

## Quick Start

### Build from Source

```bash
git clone https://github.com/unixwin/rubash.git
cd rubash
cargo build
target/debug/rubash --version
```

### Run a Script

```bash
target/debug/rubash path/to/script.sh
target/debug/rubash -c 'echo hello from rubash'
```

### Run the Compatibility Suite

```bash
# Full 83-suite measurement (requires WSL with GNU Bash 5.3.0)
MSYS_NO_PATHCONV=1 wsl bash scripts/true-baseline.sh

# Single suite
MSYS_NO_PATHCONV=1 wsl bash scripts/true-baseline.sh array
```

## Architecture

```
src/
├── lexer/           Tokenizer (quoting, escaping, heredocs, continuations)
├── parser/          Recursive-descent (simple cmds, pipelines, case, arith-for, [[ ]])
├── executor/        Command execution, builtins, expansion, glob, arrays, traps
├── builtins/        40+ builtin implementations (declare, read, printf, kill, ...)
└── lib.rs           Core types and error handling
```

- **Lexer**: Bash-style quoting, escaping, comments, variables, command substitution, arithmetic expansion, here-doc/here-string tokens, common redirects.
- **Parser**: Simple commands, pipelines, AND/OR lists, functions, brace/subshell groups, `if`, `for`, arithmetic `for`, `while`, `until`, `case`, `select`, `[[ ... ]]`, `coproc`, `time` prefixes.
- **Executor**: External commands, pipelines, redirects, temporary assignments, function calls, `source`/`.`, `eval`, shebangless script fallback, Windows/Git Bash path bridging.
- **Expansion**: Variables, positional parameters, indexed and associative arrays, command substitution, arithmetic expansion, brace expansion, tilde expansion, pathname globbing, `${parameter...}` operators, case/replacement transforms.
- **Builtins**: `alias`, `cd`, `declare`/`typeset`/`local`, `echo`, `eval`, `exec`, `export`/`readonly`, `getopts`, `hash`, `jobs`, `kill`, `let`, `mapfile`, `printf`, `pushd`/`popd`/`dirs`, `read`, `return`, `set`, `shopt`, `source`, `test`/`[`, `trap`, `type`, `ulimit`, `umask`, `unset`, `wait`, and more.

## Testing

```bash
# Unit + integration tests
cargo test --lib

# bashdb compatibility
cargo test --test cli_tests bashdb_compat -- --nocapture

# Source expansion
cargo test --test cli_tests source_expands -- --nocapture
```

## Documentation

- [`docs/COMPATIBILITY-STATUS.md`](docs/COMPATIBILITY-STATUS.md) — **single source of truth** for Rubash ↔ GNU Bash compatibility status
- [`docs/builtins.md`](docs/builtins.md) — builtin inventory and dispatch model
- [`docs/bashdb-debugging-rubash.md`](docs/bashdb-debugging-rubash.md) — bashdb fixture setup and smoke test
- [`docs/bash-upstream-tests.md`](docs/bash-upstream-tests.md) — how to run GNU Bash upstream tests

## Development Principles

- Fix by root cause subsystem, not by individual expected-output lines.
- Keep bashdb external and clean; temporary instrumentation is for diagnosis only.
- Every failing bashdb command is an opportunity to find and fix a Rubash compatibility gap.
- Compatibility baseline is GNU Bash 5.3.0 (owner-compiled at `/usr/local/bin/bash`).

## License

MIT — see [`LICENSE`](LICENSE).

## Contributing

Issues, compatibility reproductions, focused regression tests, and implementation patches welcome. Read [`AGENTS.md`](AGENTS.md) before compatibility work.

## Acknowledgements

- GNU Bash team — the original implementation being re-emplemented
- Trepan-Debuggers/bashdb — external debugger and compatibility stress test
- Rust community — language and tooling

---

*Last updated: 2026-09-11*
