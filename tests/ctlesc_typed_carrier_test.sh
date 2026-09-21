#!/bin/bash
# Test CTLESC marker does not leak to user-visible output
# Verifies that U+0011 (C0 carrier) is resolved before text leaves
# the decode boundary.

set -e

THIS_SH="${THIS_SH:-/mnt/d/repo/rubash/target/debug/rubash.exe}"

echo "Test 1: No CTLESC in simple output"
output=$("$THIS_SH" -c 'echo hello')
# Check for CTLESC (U+0011, byte 0x11 in UTF-8 representation)
# The test passes if output contains no 0x11 byte
if printf "%s" "$output" | od -An -tx1 | grep -q ' 11 '; then
    echo "FAIL: CTLESC leaked to stdout"
    exit 1
else
    echo "PASS: no CTLESC in stdout"
fi

echo "Test 2: No CTLESC in variable assignment"
output=$("$THIS_SH" -c 'x=hello; echo "$x"')
if printf "%s" "$output" | od -An -tx1 | grep -q ' 11 '; then
    echo "FAIL: CTLESC leaked to stdout"
    exit 1
else
    echo "PASS: no CTLESC in variable assignment"
fi

echo "Test 3: No CTLESC in declare -p"
output=$("$THIS_SH" -c 'x=hello; declare -p x')
if printf "%s" "$output" | od -An -tx1 | grep -q ' 11 '; then
    echo "FAIL: CTLESC leaked to declare -p"
    exit 1
else
    echo "PASS: no CTLESC in declare -p"
fi

echo "All CTLESC golden assertion tests passed"
