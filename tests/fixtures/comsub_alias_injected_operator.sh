shopt -s expand_aliases
alias p='echo hi | wc -l'
echo "$(p)"
v=$(cat <<EOF
x\"y
EOF
)
echo "[$v]"
