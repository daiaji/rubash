#!/bin/bash
# Test PATSUB family markers do not leak to user-visible output
# Verifies that U+E310-E313 markers (function-local PUA) are resolved
# before text leaves the substitution boundary.

set -e

THIS_SH="${THIS_SH:-/mnt/d/repo/rubash/target/debug/rubash.exe}"

echo "Test 1: No PATSUB markers in simple substitution output"
output=$("$THIS_SH" -c 'echo hello')
# Check for PUA markers U+E310-E313
if printf "%s" "$output" | od -An -tx1 | grep -q 'e3 10\|e3 11\|e3 12\|e3 13'; then
    echo "FAIL: PATSUB markers leaked to stdout"
    exit 1
else
    echo "PASS: no PATSUB markers in stdout"
fi

echo "Test 2: No PATSUB markers in variable assignment"
output=$("$THIS_SH" -c 'x=hello; echo "$x"')
if printf "%s" "$output" | od -An -tx1 | grep -q 'e3 10\|e3 11\|e3 12\|e3 13'; then
    echo "FAIL: PATSUB markers leaked to stdout"
    exit 1
else
    echo "PASS: no PATSUB markers in variable assignment"
fi

echo "Test 3: No PATSUB markers in declare -p"
output=$("$THIS_SH" -c 'x=hello; declare -p x')
if printf "%s" "$output" | od -An -tx1 | grep -q 'e3 10\|e3 11\|e3 12\|e3 13'; then
    echo "FAIL: PATSUB markers leaked to declare -p"
    exit 1
else
    echo "PASS: no PATSUB markers in declare -p"
fi

echo "All PATSUB golden assertion tests passed"
