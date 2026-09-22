#!/usr/bin/env bash
# Deterministic divergences from the fork-hybrid differential fuzzer
# (experiments/fork-hybrid/differential-fuzz/divergent/).
#
# case_0026: a `\"` inside a command-substitution body is an escaped-quote
# token kept raw by parse_comsub (parse.y:4451, PST_NOEXPAND); the inner
# lexer dequotes it at execution. Bodies routed through the mutable
# embedded walker (any word containing `$((`) lost the backslash and the
# inner parse saw a syntactic quote instead.
echo $(echo $(( x )) | echo "${x:-d}" '$x' "\"q\"" "${x:-d}")
echo $(echo "\"a\" \"b\"")
echo $(echo $((x))"\"q\"")
echo $(echo "\"q\"" ; echo $((x)))

# case_0030: an unquoted expansion producing nothing contributes no field,
# regardless of IFS (subst.c:13219 expand_word_list_internal). IFS=''
# disables splitting only; a null $x must still vanish from the list.
unset x
IFS=''
for v in v 1 a $x; do echo "<$v>"; done
x=' '
for v in 1 $x a; do echo "[$v]"; done
unset x
for v in 1 "$x" a; do echo "{$v}"; done
