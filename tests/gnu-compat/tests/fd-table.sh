#!/usr/bin/env bash
# fd-table.sh - promoted fork-hybrid fd probes (experiments/fork-hybrid/
# probes/fd_probes.sh). POSIX fd-table copy semantics per GNU bash 5.3.0:
# dup shares the open file description (offset), fork copies the table.
# Prints one tagged line per probe; the .right file pins GNU's stdout.
workdir="$(dirname "$0")/fd-table-work.$$"
mkdir -p "$workdir" && cd "$workdir" || exit 1
trap 'cd /; rm -rf "$workdir"' EXIT

result() { printf 'P%s: %s
' "$1" "$2"; }

# P1: redirections applied left-to-right; later redirect of same fd wins
p1() { echo ab > ./p1.txt; { read v <&3 3<<EOF
hello
EOF
} 3<./p1.txt; result 1 "v=[$v]"; }

# P2: exec 3<f persists; cmd <&3 shares the offset (cat sees only the rest)
p2() { printf 'line1
line2
' > ./p2.txt; exec 3<./p2.txt; read u <&3; cat <&3; exec 3<&-; result 2 "u=[$u]"; }

# P3: dynamic fd {var}<file allocates fd >=10
p3() { printf 'dyn
' > ./p3.txt; exec {myfd}<./p3.txt; read w <&$myfd; exec {myfd}<&-; result 3 "myfd=$myfd w=[$w]"; }

# P4: cmd 4<&0 - copy stdin to fd 4; cmd reads fd4 then redirected stdin
p4() { printf 'a
b
' > ./p4.txt; { read x; read y <&4; } 4<&0 <./p4.txt; result 4 "x=[$x] y=[$y]"; }

# P5: pipeline stdin of cmd2 is the pipe, then redirected to a file
p5() { printf 'pp
' > ./p5.txt; out=$(echo hello | cat <./p5.txt); result 5 "out=[$out]"; }

# P6: comsub/subshell inherit parent's fds (shared offset -> q is empty)
p6() { printf 'sub-content
' > ./p6.txt; exec 3<./p6.txt; r=$( { read z; } <&3 ); s=$( (read q <&3; echo "q=[$q]") ); exec 3<&-; result 6 "r=[$r] s=[$s]"; }

# P7: background job inherits fd 3 (fork-time table copy)
p7() { printf 'bg-data
' > ./p7.txt; exec 3<./p7.txt; { read bg <&3; echo "bg=[$bg]" > ./p7.out; } & wait; exec 3<&-; result 7 "$(cat ./p7.out)"; }

# P8: <& / >& dup both directions; dup of a closed fd errors on stderr
p8() { exec 8<&1; echo e8 >&8; exec 8>&-; (exec 9<&7) 2>./p8.err; result 8 "err=[$(sed 's/.*: line /line /; s/[0-9]\+/N/g' ./p8.err | head -c 60)]"; }

# P9: numbered heredoc fd shares position with later <&3 reads
p9() { { read a; read b <&3; } 3<<EOF
first
second
EOF
result 9 "a=[$a] b=[$b]"; }

# P10: close in a subshell must not close the parent's fd
p10() { printf 'still-open
' > ./p10.txt; exec 3<./p10.txt; ( exec 3<&- ); read t <&3; exec 3<&-; result 10 "t=[$t]"; }

# P11: { read m; } 3<f 4<&3 - dup ordering; read gets fd0 default
p11() { printf 'move
' > ./p11.txt; { read m; } 3<./p11.txt 4<&3; result 11 "m=[$m]"; }

# P12: comsub reads fd3 opened by the enclosing command's redirect
p12() { printf 'cs
' > ./p12.txt; o=$(cat <&3) 3<./p12.txt; result 12 "o=[$o]"; }

for i in 1 2 3 4 5 6 7 8 9 10 11 12; do "p$i"; done
