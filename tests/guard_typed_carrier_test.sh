#!/bin/bash
# Test function-local guard markers do not leak to user-visible output
# Verifies that U+E314-E317 markers (function-local PUA) are resolved
# before text leaves the decode boundary.

set -e

THIS_SH="${THIS_SH:-/mnt/d/repo/rubash/target/debug/rubash.exe}"

echo "Test 1: No guard markers in simple output"
output=$("$THIS_SH" -c 'echo hello')
# Check for PUA markers U+E314-E317
if printf "%s" "$output" | od -An -tx1 | grep -q 'e3 14\|e3 15\|e3 16\|e3 17'; then
    echo "FAIL: Guard markers leaked to stdout"
    exit 1
else
    echo "PASS: no guard markers in stdout"
fi

echo "Test 2: No guard markers in variable assignment"
output=$("$THIS_SH" -c 'x=hello; echo "$x"')
if printf "%s" "$output" | od -An -tx1 | grep -q 'e3 14\|e3 15\|e3 16\|e3 17'; then
    echo "FAIL: Guard markers leaked to stdout"
    exit 1
else
    echo "PASS: no guard markers in variable assignment"
fi

echo "Test 3: No guard markers in declare -p"
output=$("$THIS_SH" -c 'x=hello; declare -p x')
if printf "%s" "$output" | od -An -tx1 | grep -q 'e3 14\|e3 15\|e3 16\|e3 17'; then
    echo "FAIL: Guard markers leaked to declare -p"
    exit 1
else
    echo "PASS: no guard markers in declare -p"
fi

echo "All guard golden assertion tests passed"
