#!/usr/bin/env bash
# License: MIT (full text: LICENSE at the repository root).
# Copyright 2024-2026 Rust-Shell Contributors.
# true-baseline.sh variant that pins the GNU side to /usr/bin/bash 5.2.21
# (the system bash) by REMOVING /usr/local/bin from PATH.  Context: a
# bash 5.3.0 was installed at /usr/local/bin/bash on 2026-09-09 and
# silently became the GNU baseline for the default true-baseline.sh;
# the v5->v6 ledger comparison spanned that install.  This variant
# exists to A/B the GNU-version effect.  Ledger/out paths are separate
# so the two harnesses never clobber each other's artifacts.
set -u

REPO=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
BASE="$REPO/target/issue-suites/results/bash-tests-rw"
OUT="$REPO/target/issue-suites/results/true-baseline-521"
RUB="$REPO/target/debug/rubash.exe"
LOG="$REPO/target/issue-suites/results/true-baseline-521-ledger.log"
TESTS_SRC="$REPO/third_party/bash/tests"

mkdir -p "$BASE"
if [ ! -f "$BASE/recho" ] && [ -d "$TESTS_SRC" ]; then
  cp -r "$TESTS_SRC/." "$BASE/"
  find "$BASE" -type f \( -name "*.tests" -o -name "run-*" -o -name "*.right" -o -name "*.sub" \) \
    -exec sh -c 'tr -d "\r" < "$1" > "$1.lf" && mv "$1.lf" "$1"' _ {} \;
fi
sync_suite() {
  [ -f "$TESTS_SRC/$1.tests" ] || return 1
  tr -d "\r" < "$TESTS_SRC/$1.tests" > "$BASE/$1.tests"
}
ensure_test_helpers() {
  local h
  for h in recho zecho; do
    if [ ! -x "$BASE/$h" ] && [ -f "$REPO/third_party/bash/support/$h.c" ]; then
      gcc -O1 -o "$BASE/$h" "$REPO/third_party/bash/support/$h.c" 2>/dev/null || true
    fi
  done
}
ensure_test_helpers

if [ $# -eq 0 ]; then
  SUITES=$(cd "$TESTS_SRC" && ls *.tests 2>/dev/null | sed "s/[.]tests$//")
else
  SUITES="$*"
fi

# ---- /bin/sh fixture (rb side only) ----------------------------------------
# Same as true-baseline.sh: executor/path.rs degrades /bin/sh|/usr/bin/sh to
# a `sh` found on PATH; the fixture dir ships sh.exe = a niubash build
# mounted on this rubash worktree so /bin/sh children run our own engine.
SHFIX="$REPO/target/sh-fixture"
NIU_SRC="${NIUBASH_BIN:-$REPO/../niubash/target/debug/niu.exe}"
mkdir -p "$SHFIX"
if [ -x "$NIU_SRC" ]; then
  cp -f "$NIU_SRC" "$SHFIX/sh.exe" 2>/dev/null || true
fi
RB_PATH="$SHFIX"
[ -x "$SHFIX/sh.exe" ] || echo "WARN: no sh fixture; /bin/sh falls back to PATH" >&2

# WSL -> Win32 env propagation is opt-in (see true-baseline.sh): without
# WSLENV /w entries the __RUBASH_* and TMPDIR vars never reach rubash.exe.
export WSLENV="__RUBASH_NO_UPSTREAM_SCRIPTS/w:TMPDIR/p"

mkdir -p "$OUT"
: > "$LOG"
for name in $SUITES; do
  sync_suite "$name" || { echo "$name SKIP(no-source)" >> "$LOG"; continue; }
  w="$OUT/$name"; mkdir -p "$w/tmp"
  # timeout(1) without --foreground puts the command in a new process group;
  # suite pieces that touch the terminal (history4.sub's `bash --norc -i`,
  # jobs.tests `set -m`) then stop on SIGTTIN/SIGTTOU and ignore the TERM,
  # surfacing as a fake 40s SIGKILL timeout. --foreground keeps them in the
  # caller's group so the tty ops succeed. jobs.tests additionally needs >40s
  # of real wall-clock sleeps, so it gets a larger bound.
  case "$name" in
    jobs) tmo=120 ;;
    *)    tmo=40 ;;
  esac
  ( cd "$BASE" && PATH="$BASE:/usr/bin:/bin" TMPDIR="$w/tmp" \
      THIS_SH=/usr/bin/bash timeout --foreground -k 5 "$tmo" /usr/bin/bash "./$name.tests" \
      > "$w/gnu.out" 2> "$w/gnu.err" ) < /dev/null
  echo $? > "$w/gnu.rc"
  ( cd "$BASE" && PATH="$RB_PATH:$BASE:/usr/bin:/bin" TMPDIR="$w/tmp" \
      __RUBASH_NO_UPSTREAM_SCRIPTS=1 \
      timeout --foreground -k 5 "$tmo" "$RUB" "./$name.tests" \
      > "$w/rb.out" 2> "$w/rb.err" ) < /dev/null
  echo $? > "$w/rb.rc"
  n=$(diff "$w/gnu.out" "$w/rb.out" 2>/dev/null | grep -c "^[<>]")
  echo "$name $n" >> "$LOG"
done
echo TRUE-DONE
