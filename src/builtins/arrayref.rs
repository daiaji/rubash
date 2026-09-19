//! GNU arrayfunc.c:1288 tokenize_array_reference /
//! arrayfunc.c:1348 valid_array_reference — the `name[sub]` validity check
//! shared by `read` (read.def:1037/1090), `printf -v` (printf.def:306),
//! and other builtin variable-name operands.
//!
//! The check is flag-sensitive: SET_VFLAGS (builtins/common.h:279) turns
//! the array_expand_once/assoc_expand_once shopt into VA_NOEXPAND, and only
//! then is the base name's associative-ness consulted (arrayfunc.c:1300).
//! Without VA_NOEXPAND the subscript is scanned with arithmetic rules
//! (subst.c:2086 skip_matched_pair, flags=0), so `a[80's]` — an unbalanced
//! single quote — is not a valid reference (assoc9.sub `read a[$b]`).

use std::collections::HashMap;

/// Return true when `word` is a well-formed `name[sub]` array reference:
/// a valid identifier base, a non-empty subscript, and the closing `]`
/// as the last byte. `expand_once` is the array_expand_once shopt state
/// (VA_NOEXPAND); `base_is_assoc` is assoc_p(base) — GNU only performs the
/// assoc lookup when VA_NOEXPAND is set, so callers may pass the raw
/// marked state and this function gates it on `expand_once`.
///
/// GNU's VA_ONEWORD branch (whole-tail accept when the operand word carries
/// W_ARRAYREF) is not modeled: W_ARRAYREF only survives expansion when the
/// word text is unchanged (subst.c:12419), so it cannot apply to the
/// expanded operands this checks.
pub(crate) fn valid_array_reference(
    word: &str,
    expand_once: bool,
    base_is_assoc: bool,
) -> bool {
    let Some(open) = word.find('[') else {
        return false;
    };
    if !valid_identifier(&word[..open]) {
        return false;
    }
    let tail = &word.as_bytes()[open..];
    let close = if expand_once && base_is_assoc {
        // skipsubscript(t, 0, ssflags|1): skip_matched_pair flags&1
        // disables escapes, quote spans, substitutions, and bracket
        // nesting — the first `]` closes the subscript.
        tail.iter().position(|&b| b == b']')
    } else {
        skip_matched_pair_arith(tail)
    };
    // GNU arrayfunc.c:1319: t[len] must be `]`, the subscript must be
    // non-empty (len > 1), and `]` must be the last byte.
    matches!(close, Some(index) if index > 1 && index + 1 == tail.len())
}

/// GNU subst.c:2086 skip_matched_pair(string, 0, '[', ']', 0): scan a
/// bracketed subscript under arithmetic rules — backslash escapes,
/// backquote spans, single/double quoted spans skipped wholesale, `[`/`]`
/// nesting counted, and `$(`/`${` extracted as balanced units. Returns
/// Some(index) of the closing `]` at depth 0, or None when the scan ran
/// off the end (an unterminated quote or unmatched bracket makes the
/// reference invalid).
fn skip_matched_pair_arith(tail: &[u8]) -> Option<usize> {
    debug_assert_eq!(tail.first(), Some(&b'['));
    let mut index = 1usize;
    let mut depth = 1usize;
    while index < tail.len() {
        match tail[index] {
            b'\\' => index += 2,
            b'`' => {
                index += 1;
                while index < tail.len() {
                    match tail[index] {
                        b'\\' => index += 2,
                        b'`' => {
                            index += 1;
                            break;
                        }
                        _ => index += 1,
                    }
                }
            }
            b'[' => {
                depth += 1;
                index += 1;
            }
            b']' => {
                depth -= 1;
                if depth == 0 {
                    return Some(index);
                }
                index += 1;
            }
            b'\'' => {
                index += 1;
                while index < tail.len() && tail[index] != b'\'' {
                    index += 1;
                }
                index += 1;
            }
            b'"' => {
                index += 1;
                while index < tail.len() {
                    match tail[index] {
                        b'\\' => index += 2,
                        b'"' => {
                            index += 1;
                            break;
                        }
                        _ => index += 1,
                    }
                }
            }
            b'$' if matches!(tail.get(index + 1), Some(b'(') | Some(b'{')) => {
                index = skip_balanced_substitution(tail, index);
            }
            _ => index += 1,
        }
    }
    None
}

/// GNU subst.c extract_delimited_string / extract_dollar_brace_string:
/// skip a `$(...)`, `$((...))`, or `${...}` unit starting at `start`
/// (`text[start] == b'$'`) with quote, escape, and nesting awareness.
/// Returns the index just past the unit (or past EOS if unterminated).
fn skip_balanced_substitution(text: &[u8], start: usize) -> usize {
    let (open, close) = if text.get(start + 1) == Some(&b'{') {
        (b'{', b'}')
    } else {
        (b'(', b')')
    };
    let mut index = start + 2;
    let mut depth = 1usize;
    while index < text.len() {
        match text[index] {
            b'\\' => index += 2,
            b'\'' => {
                index += 1;
                while index < text.len() && text[index] != b'\'' {
                    index += 1;
                }
                index += 1;
            }
            b'"' => {
                index += 1;
                while index < text.len() {
                    match text[index] {
                        b'\\' => index += 2,
                        b'"' => {
                            index += 1;
                            break;
                        }
                        _ => index += 1,
                    }
                }
            }
            c if c == open => {
                depth += 1;
                index += 1;
            }
            c if c == close => {
                depth -= 1;
                index += 1;
                if depth == 0 {
                    return index;
                }
            }
            _ => index += 1,
        }
    }
    index
}

/// Convenience for builtin operands: resolve the two inputs GNU derives
/// from live shell state — the array_expand_once shopt (SET_VFLAGS'
/// VA_NOEXPAND) and assoc_p(base) — then run valid_array_reference.
pub(crate) fn valid_array_reference_for_env(
    word: &str,
    env_vars: &HashMap<String, String>,
) -> bool {
    let Some(open) = word.find('[') else {
        return false;
    };
    let expand_once = crate::builtins::shopt::option_enabled(env_vars, "array_expand_once");
    let base_is_assoc = is_marked_assoc(env_vars, &word[..open]);
    valid_array_reference(word, expand_once, base_is_assoc)
}

/// GNU general.c legal_identifier: a shell variable name — leading
/// alpha/underscore, then alphanumeric/underscore.
fn valid_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first == '_' || first.is_ascii_alphabetic())
        && chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}

fn is_marked_assoc(env_vars: &HashMap<String, String>, base: &str) -> bool {
    env_vars
        .get(crate::executor::types::ASSOC_VARS)
        .map(|marked| marked.split('\x1f').any(|entry| entry == base))
        .unwrap_or(false)
}
