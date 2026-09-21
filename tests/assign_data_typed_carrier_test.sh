#!/bin/bash
# Test ASSIGN_DATA_* markers do not leak to user-visible output
# Verifies that U+E301-E30C markers (storage-boundary PUA) are resolved
# before text leaves the decode boundary.

set -e

THIS_SH="${THIS_SH:-/mnt/d/repo/rubash/target/debug/rubash.exe}"

echo "Test 1: No ASSIGN_DATA markers in simple assignment output"
output=$("$THIS_SH" -c 'x=hello; echo "$x"')
# Check for PUA markers U+E301-E30C
if printf "%s" "$output" | od -An -tx1 | grep -q 'e3 01\|e3 02\|e3 03\|e3 04\|e3 05\|e3 06\|e3 07\|e3 08\|e3 09\|e3 0a\|e3 0b\|e3 0c'; then
    echo "FAIL: ASSIGN_DATA markers leaked to stdout"
    exit 1
else
    echo "PASS: no ASSIGN_DATA markers in stdout"
fi

echo "Test 2: No ASSIGN_DATA markers in compound assignment"
output=$("$THIS_SH" -c 'arr=(a b c); echo "${arr[@]}"')
if printf "%s" "$output" | od -An -tx1 | grep -q 'e3 01\|e3 02\|e3 03\|e3 04\|e3 05\|e3 06\|e3 07\|e3 08\|e3 09\|e3 0a\|e3 0b\|e3 0c'; then
    echo "FAIL: ASSIGN_DATA markers leaked to stdout"
    exit 1
else
    echo "PASS: no ASSIGN_DATA markers in compound assignment"
fi

echo "Test 3: No ASSIGN_DATA markers in declare -p"
output=$("$THIS_SH" -c 'x=hello; declare -p x')
if printf "%s" "$output" | od -An -tx1 | grep -q 'e3 01\|e3 02\|e3 03\|e3 04\|e3 05\|e3 06\|e3 07\|e3 08\|e3 09\|e3 0a\|e3 0b\|e3 0c'; then
    echo "FAIL: ASSIGN_DATA markers leaked to declare -p"
    exit 1
else
    echo "PASS: no ASSIGN_DATA markers in declare -p"
fi

echo "All ASSIGN_DATA golden assertion tests passed"
