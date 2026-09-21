#!/bin/bash
# Test PREEXPANDED_STDIN_BODY typed carrier migration
# Verifies that heredoc/here-string bodies use StdinBody::Preexpanded
# and that raw 0x05 bytes at body start are not mistaken for the sentinel

set -e

THIS_SH="${THIS_SH:-/mnt/d/repo/rubash/target/debug/rubash.exe}"

echo "Test 1: Unquoted heredoc expansion exactly once"
output=$("$THIS_SH" -c 'x=$(cat <<EOF
$((1+1))
EOF
); echo "$x"')
expected="2"
if [ "$output" = "$expected" ]; then
    echo "PASS: heredoc expands exactly once"
else
    echo "FAIL: heredoc expansion (got '$output', expected '$expected')"
    exit 1
fi

echo "Test 2: Here-string expansion exactly once"
output=$("$THIS_SH" -c 'x=$(cat <<< $((1+1))); echo "$x"')
expected="2"
if [ "$output" = "$expected" ]; then
    echo "PASS: here-string expands exactly once"
else
    echo "FAIL: here-string expansion (got '$output', expected '$expected')"
    exit 1
fi

echo "Test 3: Quoted heredoc body literal"
output=$("$THIS_SH" -c 'x=$(cat <<'"'"'EOF'"'"'
$((1+1))
EOF
); echo "$x"')
expected='$((1+1))'
if [ "$output" = "$expected" ]; then
    echo "PASS: quoted heredoc literal"
else
    echo "FAIL: quoted heredoc (got '$output', expected '$expected')"
    exit 1
fi

echo "Test 4: Raw 0x05 at heredoc body start (B1 collision test)"
# Create a heredoc with raw ENQ (0x05) as first byte
# This should be preserved as literal data, not mistaken for PREEXPANDED_STDIN_BODY
printf '\x05hello' > /mnt/d/repo/rubash/target/enq_test.txt
cat > /mnt/d/repo/rubash/target/test4.sh <<'TEST4EOF'
x=$(cat <<EOF
$(cat /mnt/d/repo/rubash/target/enq_test.txt)
EOF
)
printf "%s" "$x" | od -An -tx1
TEST4EOF
output=$("$THIS_SH" /mnt/d/repo/rubash/target/test4.sh)
# Expected: 05 68 65 6c 6c 6f (ENQ + "hello")
expected=' 05 68 65 6c 6c 6f'
if [ "$output" = "$expected" ]; then
    echo "PASS: raw 0x05 preserved at body start"
else
    echo "FAIL: raw 0x05 collision (got '$output', expected '$expected')"
    exit 1
fi
rm -f /mnt/d/repo/rubash/target/enq_test.txt /mnt/d/repo/rubash/target/test4.sh

echo "Test 5: No PREEXPANDED_STDIN_BODY sentinel in stdout"
output=$("$THIS_SH" -c 'cat <<EOF
$HOME
EOF
')
# Check for raw 0x05 byte using od
if printf "%s" "$output" | od -An -tx1 | grep -q ' 05'; then
    echo "FAIL: sentinel leaked to stdout"
    exit 1
else
    echo "PASS: no sentinel in stdout"
fi

echo "Test 6: Nested comsub in quoted heredoc is literal"
cat > /mnt/d/repo/rubash/target/test6.sh <<'TEST6EOF'
x=$(cat <<'EOF'
$(echo nested)
EOF
)
echo "$x"
TEST6EOF
output=$("$THIS_SH" /mnt/d/repo/rubash/target/test6.sh)
expected='$(echo nested)'
if [ "$output" = "$expected" ]; then
    echo "PASS: nested comsub in quoted heredoc is literal"
else
    echo "FAIL: nested comsub (got '$output', expected '$expected')"
    exit 1
fi
rm -f /mnt/d/repo/rubash/target/test6.sh

echo "Test 7: Nested comsub in unquoted heredoc expands"
cat > /mnt/d/repo/rubash/target/test7.sh <<'TEST7EOF'
x=$(cat <<EOF
$(echo nested)
EOF
)
echo "$x"
TEST7EOF
output=$("$THIS_SH" /mnt/d/repo/rubash/target/test7.sh)
expected="nested"
if [ "$output" = "$expected" ]; then
    echo "PASS: nested comsub in unquoted heredoc expands"
else
    echo "FAIL: nested comsub (got '$output', expected '$expected')"
    exit 1
fi
rm -f /mnt/d/repo/rubash/target/test7.sh

echo "All typed carrier tests passed"
