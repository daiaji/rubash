use super::{ArithLValue, ConditionalArithParser};
use crate::executor::arithmetic::{assignment_operator_at, skip_arith_ws};
use crate::executor::{
    is_marked_var, is_shell_name, is_shell_name_char, is_shell_name_start, strip_matching_quotes,
    ASSOC_VARS, NAMEREF_VARS,
};
use std::collections::HashSet;

impl ConditionalArithParser<'_> {
    /// Parse an lvalue for an assignment target. GNU expr.c:1395-1401: the
    /// STR's value (and subscript) is evaluated at token time only when the
    /// next token is NOT `=` — `a[sub] = rhs` keeps the raw subscript text
    /// and the assignment machinery re-expands it at bind time, after the
    /// RHS (`a[n]=++n` stores at a[1]). Compound assignment operators
    /// (`a[sub] += rhs`) are not EQ, so expr_streval runs early and the
    /// precomputed index is bound (`a[n]+=++n` stores at a[0]).
    pub(super) fn parse_lvalue_for_assignment(&mut self) -> Option<ArithLValue> {
        self.skip_ws();
        let start = self.pos;
        let first = self.peek()? as char;
        if !is_shell_name_start(first) {
            return None;
        }
        self.pos += 1;
        while self.peek().is_some_and(|ch| is_shell_name_char(ch as char)) {
            self.pos += 1;
        }
        let name = std::str::from_utf8(&self.input[start..self.pos])
            .ok()?
            .to_string();

        // GNU expr.c:1350: `[` must immediately follow the name — `a [0]`
        // is STR `a` followed by the junk char `[`.
        if self.peek() != Some(b'[') {
            let name = self.resolved_lvalue_name(&name);
            return Some(ArithLValue::Scalar(name));
        }
        self.pos += 1;

        let resolved_name = self.resolved_lvalue_name(&name);
        if is_marked_var(self.env_vars, ASSOC_VARS, &resolved_name) {
            // Associative arrays use the key verbatim; no deferred evaluation.
            let key = self.parse_assoc_subscript()?;
            return Some(ArithLValue::Assoc {
                name: resolved_name,
                key,
            });
        }

        if self.peek() == Some(b']') {
            // `a[]` — STR token `a[]`; the bind fails with
            // `` `a[]': not a valid identifier `` (expr_bind_variable ->
            // sh_invalidid) but the expression keeps evaluating.
            self.pos += 1;
            return Some(ArithLValue::InvalidElement {
                display: format!("{resolved_name}[]"),
            });
        }

        let subscript = self.collect_raw_subscript(start)?;

        // GNU readtok's peektok: `=` (exactly EQ) defers the subscript;
        // any other assignment operator evaluated it during expr_streval.
        let mut op_pos = self.pos;
        skip_arith_ws(self.input, &mut op_pos);
        match assignment_operator_at(self.input, op_pos) {
            Some("=") => Some(ArithLValue::IndexedRaw {
                name: resolved_name,
                subscript,
            }),
            _ => {
                let index = self.eval_subscript_index(&subscript)?;
                Some(ArithLValue::Indexed {
                    name: resolved_name,
                    index,
                })
            }
        }
    }

    /// Collect the raw text between `[` and `]` without evaluating it.
    /// `str_start` is the position of the STR token's name — GNU's lasttp
    /// for an unterminated subscript (expr.c:1365-1367 evalerror
    /// `bad array subscript`).
    fn collect_raw_subscript(&mut self, str_start: usize) -> Option<String> {
        let start = self.pos;
        let mut depth = 1usize;
        let mut single = false;
        let mut double = false;
        while self.pos < self.input.len() {
            match self.input[self.pos] {
                b'\\' => {
                    self.pos += 1;
                }
                b'\'' if !double => single = !single,
                b'"' if !single => double = !double,
                b'[' if !single && !double => {
                    depth += 1;
                }
                b']' if !single && !double => {
                    depth -= 1;
                    if depth == 0 {
                        let text = std::str::from_utf8(&self.input[start..self.pos])
                            .ok()?
                            .to_string();
                        self.pos += 1;
                        return Some(text);
                    }
                }
                _ => {}
            }
            self.pos += 1;
        }
        self.fail("bad array subscript", str_start);
        None
    }

    pub(super) fn parse_lvalue(&mut self) -> Option<ArithLValue> {
        self.skip_ws();
        let start = self.pos;
        let first = self.peek()? as char;
        if !is_shell_name_start(first) {
            return None;
        }
        self.pos += 1;
        while self.peek().is_some_and(|ch| is_shell_name_char(ch as char)) {
            self.pos += 1;
        }
        let name = std::str::from_utf8(&self.input[start..self.pos])
            .ok()?
            .to_string();

        // GNU expr.c:1350: `[` must immediately follow the name
        // characters — `a [0]` is STR `a` then junk `[` -> "invalid
        // arithmetic operator" (verified: `(( a [0] ))` reports
        // `(error token is "[0] ")`).
        if self.peek() != Some(b'[') {
            let name = self.resolved_lvalue_name(&name);
            return Some(ArithLValue::Scalar(name));
        }
        self.pos += 1;

        let resolved_name = self.resolved_lvalue_name(&name);
        if is_marked_var(self.env_vars, ASSOC_VARS, &resolved_name) {
            let key = self.parse_assoc_subscript()?;
            return Some(ArithLValue::Assoc {
                name: resolved_name,
                key,
            });
        }

        if self.peek() == Some(b']') {
            // `a[]` — STR `a[]`; reads report `a[]: bad array subscript`
            // (twice — array_variable_part and get_array_value), writes
            // report `` `a[]': not a valid identifier ``. `a[ ]`/`a[""]`
            // are NOT this: their subscript text is non-empty and
            // evaluates to 0.
            self.pos += 1;
            return Some(ArithLValue::InvalidElement {
                display: format!("{resolved_name}[]"),
            });
        }

        // The subscript expression is captured raw; it is evaluated when
        // the lvalue's value is fetched (readtok's STR processing runs
        // expr_streval, expr.c:1397) — once, not twice, since GNU's
        // get_array_value reuses array_variable_part's work for the
        // side-effectful cases.
        let subscript = self.collect_raw_subscript(start)?;
        Some(ArithLValue::IndexedRaw {
            name: resolved_name,
            subscript,
        })
    }

    pub(super) fn resolved_lvalue_name(&self, name: &str) -> String {
        let mut current = name;
        let mut seen = HashSet::new();
        for _ in 0..16 {
            if !seen.insert(current.to_string()) {
                return name.to_string();
            }
            if !is_marked_var(self.env_vars, NAMEREF_VARS, current) {
                return current.to_string();
            }
            let Some(target) = self.env_vars.get(current) else {
                return current.to_string();
            };
            if !is_shell_name(target) {
                return current.to_string();
            }
            current = target;
        }
        name.to_string()
    }

    pub(super) fn parse_assoc_subscript(&mut self) -> Option<String> {
        let start = self.pos;
        let mut depth = 0usize;
        while self.pos < self.input.len() {
            match self.input[self.pos] {
                b'[' => {
                    depth += 1;
                    self.pos += 1;
                }
                b']' if depth == 0 => {
                    // The raw subscript is data: GNU expand_subscript_string
                    // keeps IFS whitespace that surrounds or makes up the key
                    // (`k=$'\t'; A[$k]=2` keys on the tab, and `A[ $k ]` keys
                    // on ` x `), so it must not be trimmed away.
                    let key = std::str::from_utf8(&self.input[start..self.pos])
                        .ok()?
                        .to_string();
                    self.pos += 1;
                    // A pre-expanded key (the Executor-side
                    // expand_subscript_string pass) is already the final
                    // string: use it verbatim and never expand it again.
                    if let Some(literal) = super::super::decode_arithmetic_assoc_key(&key) {
                        return Some(literal);
                    }
                    return Some(self.expand_assoc_subscript_key(&key));
                }
                b']' => {
                    depth -= 1;
                    self.pos += 1;
                }
                _ => self.pos += 1,
            }
        }
        None
    }

    pub(super) fn expand_assoc_subscript_key(&self, key: &str) -> String {
        // A wholly single-quoted subscript is literal data: GNU's
        // expand_subscript_string removes the quotes but runs no expansion
        // inside a single-quoted span, so `A['$var']` keys on the text `$var`
        // and `A['a b']` keys on `a b`.
        if let Some(literal) =
            crate::executor::subscript_expansion::wholly_single_quoted_literal(key)
        {
            return literal;
        }

        let mut output = String::new();
        let mut chars = key.chars().peekable();

        while let Some(ch) = chars.next() {
            if ch != '$' {
                output.push(ch);
                continue;
            }

            match chars.peek().copied() {
                Some('{') => {
                    chars.next();
                    let mut name = String::new();
                    for name_ch in chars.by_ref() {
                        if name_ch == '}' {
                            break;
                        }
                        name.push(name_ch);
                    }
                    output.push_str(self.env_vars.get(&name).map(String::as_str).unwrap_or(""));
                }
                Some(first) if is_shell_name_start(first) => {
                    chars.next();
                    let mut name = String::from(first);
                    while chars.peek().copied().is_some_and(is_shell_name_char) {
                        name.push(chars.next().unwrap());
                    }
                    output.push_str(self.env_vars.get(&name).map(String::as_str).unwrap_or(""));
                }
                _ => output.push(ch),
            }
        }

        // Quote removal only: an associative subscript is a string key, so
        // surrounding IFS whitespace is data, not padding (GNU keeps it).
        strip_matching_quotes(&output).to_string()
    }

    pub(super) fn consume_assignment_operator(&mut self) -> Option<&'static str> {
        let op = assignment_operator_at(self.input, self.pos)?;
        self.pos += op.len();
        Some(op)
    }
}
