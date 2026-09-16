//! Array-related functions for the executor module.
//!
//! Contains free functions and `Executor` methods for working with
//! indexed arrays, array storage, and array subscripts.

mod executor;
mod mapfile;
mod storage;

pub(super) use mapfile::split_mapfile_input;
pub(super) use storage::{
    array_indices, array_value_at, array_values,
    format_indexed_array_storage, format_indexed_array_values, indexed_array_entries,
    is_array_storage, is_marked_array_var, normalize_array_expanded_value,
    parse_array_integer_subscript, parse_array_numeric_subscript, parse_array_subscript,
    quote_array_value, resolve_indexed_array_subscript, store_indexed_array,
};

use std::collections::{BTreeMap, HashMap};
use std::env;
use std::fs;

use super::{
    apply_parameter_case_mod, assoc_value_at, case_pattern_matches, eval_arith_value,
    eval_conditional_arith_value, is_marked_var, is_shell_name, parse_indirect_pattern_removal,
    parse_parameter_case_mod, parse_parameter_replacement, parse_parameter_transform,
    pattern_contains_glob, remove_parameter_pattern, split_storage_words,
    strip_matching_quotes, unquote_storage_value, Executor, ParameterTransform,
    ARRAY_FIELD_SPLIT_MARKER, ASSOC_VARS,
};
use crate::lexer::remove_shell_quotes;

pub(super) fn is_array_element_assignment_word(word: &str) -> bool {
    let Some((left, _)) = word.split_once('=') else {
        return false;
    };
    let left = left.strip_suffix('+').unwrap_or(left);
    let Some((name, index)) = left.split_once('[') else {
        return false;
    };
    is_shell_name(name) && index.ends_with(']')
}

pub(super) fn append_scalar_value(current: &str, value: &str) -> String {
    let mut output = current.to_string();
    output.push_str(value);
    output
}

fn is_ifs_whitespace(ch: char) -> bool {
    matches!(ch, ' ' | '\t' | '\n')
}

// \x1c marks whitespace that was quoted/escaped inside a `${var-word}`
// style alternate: GNU removes the quotes but keeps such whitespace out of
// field splitting (posixexp2 37). The marker is consumed here and the
// protected char becomes plain field data.
fn split_ifs_whitespace(value: &str, ifs: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut chars = value.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\x1c' {
            if let Some(protected) = chars.next() {
                current.push(protected);
            }
            continue;
        }
        if ifs.contains(ch) {
            if !current.is_empty() {
                fields.push(std::mem::take(&mut current));
            }
        } else {
            current.push(ch);
        }
    }
    if !current.is_empty() {
        fields.push(current);
    }
    fields
}

fn split_mixed_ifs(value: &str, ifs: &str) -> Vec<String> {
    let chars = value.chars().collect::<Vec<_>>();
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut index = 0;

    while index < chars.len() {
        let ch = chars[index];
        if ch == '\x1c' {
            index += 1;
            if index < chars.len() {
                current.push(chars[index]);
                index += 1;
            }
            continue;
        }
        if !ifs.contains(ch) {
            current.push(ch);
            index += 1;
            continue;
        }

        if is_ifs_whitespace(ch) {
            while index < chars.len()
                && ifs.contains(chars[index])
                && is_ifs_whitespace(chars[index])
            {
                index += 1;
            }
            if index < chars.len() && !is_ifs_whitespace(chars[index]) && ifs.contains(chars[index])
            {
                continue;
            }
            if !current.is_empty() {
                fields.push(std::mem::take(&mut current));
            }
            continue;
        }

        fields.push(std::mem::take(&mut current));
        index += 1;
        while index < chars.len() && ifs.contains(chars[index]) && is_ifs_whitespace(chars[index]) {
            index += 1;
        }
    }

    if !current.is_empty() {
        fields.push(current);
    }
    fields
}

/// IFS splitting when the value or IFS contains raw-byte markers.
///
/// GNU `subst.c:1148-1250` (`string_extract_verbatim`) walks the string
/// byte-by-byte, uses `mbrtowc` to identify multibyte character boundaries,
/// and checks each complete character against the IFS character list via
/// `wcschr`. A multibyte character like `€` (U+20AC, bytes E2 82 AC) is
/// never split even if one of its bytes appears in IFS.
///
/// Rubash stores bytes >= 0x80 as U+E000 + U+E0xx marker pairs inside Rust
/// Strings. This function decodes both sides to raw bytes, walks the value
/// respecting UTF-8 character boundaries, and re-encodes the result.
fn field_split_with_raw_byte_markers(value: &str, ifs: &str) -> Vec<String> {
    use crate::executor::substitution_metadata::{
        decode_raw_byte_markers, encode_raw_byte_marker, RAW_BYTE_MARKER_ESCAPE,
    };

    let sentinel = char::from_u32(RAW_BYTE_MARKER_ESCAPE).expect("sentinel is valid");

    // Decode both sides to raw bytes.
    let value_bytes = if value.contains(sentinel) {
        decode_raw_byte_markers(value.as_bytes())
    } else {
        value.as_bytes().to_vec()
    };
    let ifs_bytes = if ifs.contains(sentinel) {
        decode_raw_byte_markers(ifs.as_bytes())
    } else {
        ifs.as_bytes().to_vec()
    };

    // Build the IFS character list: each entry is a byte sequence for one
    // IFS character (1 byte for ASCII, 1-4 bytes for UTF-8 multibyte).
    let ifs_chars = split_into_chars(&ifs_bytes);
    let ifs_is_whitespace = |bytes: &[u8]| {
        bytes.len() == 1 && matches!(bytes[0], b' ' | b'\t' | b'\n')
    };

    let mut fields: Vec<Vec<u8>> = Vec::new();
    let mut current_bytes: Vec<u8> = Vec::new();
    let mut index = 0;

    while index < value_bytes.len() {
        // \x1c protection marker: next byte is literal data, not a separator.
        if value_bytes[index] == 0x1c {
            index += 1;
            if index < value_bytes.len() {
                current_bytes.push(value_bytes[index]);
                index += 1;
            }
            continue;
        }

        // Try to parse a UTF-8 character starting here.
        let char_len = utf8_char_len(&value_bytes[index..]);
        let char_bytes = &value_bytes[index..index + char_len];

        // Check if this character matches any IFS character.
        let matched_ifs = ifs_chars.iter().find(|ifs_char| {
            ifs_char.len() == char_bytes.len() && ifs_char.as_slice() == char_bytes
        });

        if let Some(matched) = matched_ifs {
            if ifs_is_whitespace(matched) {
                // Whitespace IFS: skip consecutive whitespace IFS chars.
                fields.push(std::mem::take(&mut current_bytes));
                index += char_len;
                while index < value_bytes.len() {
                    let cl = utf8_char_len(&value_bytes[index..]);
                    let cb = &value_bytes[index..index + cl];
                    let is_ws_sep = ifs_chars.iter().any(|ic| {
                        ic.len() == cb.len() && ic.as_slice() == cb && ifs_is_whitespace(ic)
                    });
                    if is_ws_sep {
                        index += cl;
                    } else {
                        break;
                    }
                }
                continue;
            } else {
                // Non-whitespace IFS: every separator produces a field.
                fields.push(std::mem::take(&mut current_bytes));
                index += char_len;
                continue;
            }
        }

        // Not a separator: add this character to the current field.
        current_bytes.extend_from_slice(char_bytes);
        index += char_len;
    }

    fields.push(std::mem::take(&mut current_bytes));

    // Drop trailing empty field from a trailing non-whitespace separator.
    if fields.last().is_some_and(|f| f.is_empty()) && fields.len() > 1 {
        fields.pop();
    }

    // Re-encode byte fields back to Rust Strings with raw-byte markers.
    fields
        .into_iter()
        .map(|bytes| bytes_to_shell_text(&bytes, &sentinel, &encode_raw_byte_marker))
        .collect()
}

/// Split a byte sequence into individual characters (UTF-8 aware).
/// Invalid UTF-8 bytes are treated as single-byte characters.
fn split_into_chars(bytes: &[u8]) -> Vec<Vec<u8>> {
    let mut chars = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        let len = utf8_char_len(&bytes[index..]);
        chars.push(bytes[index..index + len].to_vec());
        index += len;
    }
    chars
}

/// Return the length (in bytes) of the UTF-8 character starting at the
/// beginning of `bytes`. Invalid bytes are treated as 1-byte characters.
fn utf8_char_len(bytes: &[u8]) -> usize {
    if bytes.is_empty() {
        return 0;
    }
    let first = bytes[0];
    if first < 0x80 {
        1
    } else if first & 0xE0 == 0xC0 {
        2.min(bytes.len())
    } else if first & 0xF0 == 0xE0 {
        3.min(bytes.len())
    } else if first & 0xF8 == 0xF0 {
        4.min(bytes.len())
    } else {
        1
    }
}

/// Re-encode raw bytes as a Rust String, using raw-byte markers for bytes
/// >= 0x80 so the result is valid UTF-8 and round-trips through the executor.
fn bytes_to_shell_text(
    bytes: &[u8],
    sentinel: &char,
    encode_raw_byte_marker: &dyn Fn(u8) -> String,
) -> String {
    let mut output = String::new();
    let mut index = 0;
    while index < bytes.len() {
        let len = utf8_char_len(&bytes[index..]);
        let slice = &bytes[index..index + len];
        if let Ok(text) = std::str::from_utf8(slice) {
            output.push_str(text);
        } else {
            for &byte in slice {
                if byte < 0x80 {
                    output.push(char::from(byte));
                } else {
                    output.push_str(&encode_raw_byte_marker(byte));
                }
            }
        }
        index += len;
    }
    let _ = sentinel; // suppress unused warning
    output
}

pub(super) fn field_split_values_with_ifs(value: &str, ifs: Option<&str>) -> Vec<String> {
    let Some(ifs) = ifs else {
        return split_ifs_whitespace(value, " \t\n");
    };
    if ifs.is_empty() {
        return vec![value.to_string()];
    }

    // Bash defines IFS whitespace narrowly as space, tab, and newline.
    // Other Unicode whitespace remains data unless explicitly listed in IFS.
    if ifs == " \t\n" {
        return split_ifs_whitespace(value, ifs);
    }

    // When the value or IFS contains raw-byte markers (multibyte chars
    // stored as U+E000 + U+E0xx pairs), char-level `ifs.contains(ch)` would
    // falsely match the marker sentinel. Decode to bytes and split at the
    // byte level while respecting UTF-8 character boundaries, matching GNU
    // subst.c:1210-1240 (mbrtowc + wcschr against the IFS wchar list).
    let sentinel = char::from_u32(crate::executor::substitution_metadata::RAW_BYTE_MARKER_ESCAPE)
        .expect("raw-byte sentinel is a valid char");
    if value.contains(sentinel) || ifs.contains(sentinel) {
        return field_split_with_raw_byte_markers(value, ifs);
    }

    if ifs.chars().all(is_ifs_whitespace) {
        return split_ifs_whitespace(value, ifs);
    }

    if ifs.chars().any(is_ifs_whitespace) {
        return split_mixed_ifs(value, ifs);
    }

    // Non-whitespace IFS: every separator produces a field. Bash keeps leading
    // and internal empty fields (`IFS=:; set -- :a::b`) and drops only the
    // final empty field produced by a trailing delimiter.
    // \x1c marks literal IFS characters that must not be split (GNU CTLESC:
    // literal word characters are marked during expansion, only expansion
    // results are eligible for IFS splitting).
    let chars: Vec<char> = value.chars().collect();
    let mut fields: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut index = 0;
    while index < chars.len() {
        let ch = chars[index];
        if ch == '\x1c' {
            // Protected literal character — add next char to current field
            index += 1;
            if index < chars.len() {
                current.push(chars[index]);
                index += 1;
            }
            continue;
        }
        if ifs.contains(ch) {
            fields.push(std::mem::take(&mut current));
            index += 1;
            continue;
        }
        current.push(ch);
        index += 1;
    }
    fields.push(current);
    if fields.last().is_some_and(|field| field.is_empty()) && fields.len() > 1 {
        fields.pop();
    }
    fields
}

pub(super) fn field_split_array_values_with_ifs(
    values: Vec<String>,
    ifs: Option<&str>,
) -> Vec<String> {
    values
        .into_iter()
        .flat_map(|value| field_split_values_with_ifs(&value, ifs))
        .collect()
}

pub(super) fn field_split_positional_values_with_ifs(
    values: Vec<String>,
    ifs: Option<&str>,
) -> Vec<String> {
    let value_count = values.len();
    values
        .into_iter()
        .enumerate()
        .flat_map(|(index, value)| {
            let is_last = index + 1 == value_count;
            let mut fields = if value.is_empty() {
                if is_last {
                    Vec::new()
                } else {
                    vec![value]
                }
            } else if let Some(ifs) = ifs.filter(|ifs| ifs.chars().any(|ch| !ch.is_whitespace())) {
                if ifs.chars().any(is_ifs_whitespace) {
                    split_mixed_ifs(&value, ifs)
                } else {
                    value
                        .split(|ch| ifs.contains(ch))
                        .map(str::to_string)
                        .collect()
                }
            } else {
                field_split_values_with_ifs(&value, ifs)
            };
            if is_last {
                while fields.last().is_some_and(|field| field.is_empty()) {
                    fields.pop();
                }
            }
            fields
        })
        .collect()
}

#[cfg(test)]
mod field_split_tests {
    use super::field_split_values_with_ifs;

    #[test]
    fn non_whitespace_ifs_keeps_leading_empty_fields() {
        assert_eq!(field_split_values_with_ifs(":", Some(":")), vec![""]);
        assert_eq!(field_split_values_with_ifs("::", Some(":")), vec!["", ""]);
        assert_eq!(field_split_values_with_ifs(":a:", Some(":")), vec!["", "a"]);
    }

    #[test]
    fn mixed_ifs_keeps_leading_empty_fields() {
        assert_eq!(field_split_values_with_ifs(" : ", Some(": ")), vec![""]);
        assert_eq!(field_split_values_with_ifs(": :", Some(": ")), vec!["", ""]);
    }

    #[test]
    fn custom_newline_ifs_preserves_spaces_inside_fields() {
        assert_eq!(
            field_split_values_with_ifs("a b\na c\nx z", Some("\n")),
            vec!["a b", "a c", "x z"]
        );
    }

    #[test]
    fn default_ifs_still_splits_shell_whitespace() {
        assert_eq!(
            field_split_values_with_ifs("a b\na c", Some(" \t\n")),
            vec!["a", "b", "a", "c"]
        );
    }

    #[test]
    fn custom_space_ifs_collapses_repeated_spaces() {
        assert_eq!(
            field_split_values_with_ifs("a  b", Some(" ")),
            vec!["a", "b"]
        );
    }

    #[test]
    fn custom_newline_ifs_keeps_spaces_in_fields() {
        assert_eq!(
            field_split_values_with_ifs("a  b\nc  d", Some("\n")),
            vec!["a  b", "c  d"]
        );
    }

    #[test]
    fn default_ifs_does_not_split_vertical_tab_or_form_feed() {
        assert_eq!(
            field_split_values_with_ifs("a\x0bb\x0cc", None),
            vec!["a\x0bb\x0cc"]
        );
        assert_eq!(
            field_split_values_with_ifs("a\x0bb\x0cc", Some(" \t\n")),
            vec!["a\x0bb\x0cc"]
        );
    }

    #[test]
    fn default_ifs_does_not_split_non_ascii_whitespace() {
        assert_eq!(
            field_split_values_with_ifs("a\u{00a0}b\u{2003}c", None),
            vec!["a\u{00a0}b\u{2003}c"]
        );
    }
}

pub(super) fn word_is_unquoted_array_list_expansion(word: &str) -> bool {
    if word.starts_with('"') || word.starts_with('\'') || word.starts_with('\x1d') {
        return false;
    }

    let Some(inner) = word
        .strip_prefix("${")
        .and_then(|word| word.strip_suffix('}'))
    else {
        return false;
    };
    let name = inner.split_once(':').map_or(inner, |(name, _)| name);
    if let Some((_var_name, transform)) = parse_parameter_transform(name) {
        match transform {
            // GNU subst.c parameter_brace_transform renders @A/@a/@K as one
            // word and never field-splits array transform results (no
            // W_SPLITSPACE is set for VT_ARRAYVAR results).
            ParameterTransform::Assignment
            | ParameterTransform::Attributes
            | ParameterTransform::KeyValueQuoted => return false,
            _ => {}
        }
    }
    let name = parse_parameter_transform(name)
        .map(|(name, _)| name)
        .or_else(|| parse_indirect_pattern_removal(name).map(|(name, _, _)| name))
        .or_else(|| parse_parameter_replacement(name).map(|(name, _, _, _)| name))
        .or_else(|| parse_parameter_case_mod(name).map(|(name, _, _)| name))
        .unwrap_or(name);
    name.ends_with("[@]") || name.ends_with("[*]")
}

pub(super) fn pathname_expand_array_token(token: &str) -> Option<Vec<String>> {
    if token.starts_with('"') || token.starts_with('\'') || !pattern_contains_glob(token) {
        return None;
    }
    if token.contains('/') || token.contains('\\') {
        return None;
    }
    let include_dotfiles = token.starts_with('.');
    let mut matches = fs::read_dir(env::current_dir().ok()?)
        .ok()?
        .filter_map(Result::ok)
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| include_dotfiles || !name.starts_with('.'))
        .filter(|name| case_pattern_matches(token, name))
        .collect::<Vec<_>>();
    if matches.is_empty() {
        return None;
    }
    matches.sort();
    Some(matches)
}

/// Restores the lexer carrier bytes (\x14 backslash, \x17 single quote,
/// \x18 double quote, \x1f `$`, \x1a backtick, ANSI-C quote markers) that
/// survive into raw assignment tokens, after operator-level quote removal
/// has consumed the structural quotes (niubash #103 follow-up).
fn restore_quote_carriers(value: &str) -> String {
    value
        .replace('\x1f', "$")
        .replace('\x1a', "`")
        .replace('\x14', "\\")
        .replace('\x17', "'")
        .replace('\x18', "\"")
        .replace(crate::lexer::ANSI_C_QUOTE_MARKER_STR, "'")
        .replace(crate::lexer::ANSI_C_DQUOTE_MARKER_STR, "\"")
}

pub(super) fn append_array_value(
    current: &str,
    value: &str,
    integer: bool,
    ifs: Option<&str>,
    env_vars: &HashMap<String, String>,
) -> String {
    let mut entries = indexed_array_entries(current);
    let mut next_index = entries
        .keys()
        .next_back()
        .map(|index| index + 1)
        .unwrap_or(0);
    // Bash appends `arr+=str` (no parens) to arr[0] for any array, not just
    // integer arrays; `arr+=(x y)` appends new elements.
    let scalar_append = !value.starts_with('(');
    let brace_expand = crate::builtins::set::shell_option_enabled(env_vars, "braceexpand");
    let tokens = array_assignment_tokens(value)
        .into_iter()
        .flat_map(|token| {
            if brace_expand && !token.contains("${") && !token.contains('=') {
                crate::expand::braces::expand_braces(&token)
            } else {
                vec![token]
            }
        })
        .collect::<Vec<_>>();
    for token in tokens {
        if let Some(matches) = pathname_expand_array_token(&token) {
            for value in matches {
                entries.insert(next_index, value);
                next_index += 1;
            }
            continue;
        }

        if let Some((left, rhs)) = token.split_once("+=") {
            if let Some(index) = array_assignment_index(left, &entries, env_vars) {
                let current = entries.get(&index).cloned().unwrap_or_default();
                let rhs = unquote_storage_value(rhs);
                let value = if integer {
                    (eval_arith_value(&current) + eval_arith_value(&rhs)).to_string()
                } else {
                    append_scalar_value(&current, &rhs)
                };
                entries.insert(index, value);
                next_index = index + 1;
                continue;
            }
            if array_assignment_has_subscript(left) {
                continue;
            }
        }

        if let Some((left, rhs)) = token.split_once('=') {
            if let Some(index) = array_assignment_index(left, &entries, env_vars) {
                let decoded = unquote_storage_value(rhs);
                entries.insert(index, decoded);
                next_index = index + 1;
                continue;
            }
            if array_assignment_has_subscript(left) {
                continue;
            }
        }

        let command_subst_token = token.starts_with("\"$(") && token.ends_with('"');
        // A compound word quoted with EITHER quote family stays one element
        // ('a b' and "a b" each store a single element; only unquoted
        // whitespace splits). Mirrors the declare storage copy.
        let quoted_token = (token.starts_with('"') && token.ends_with('"') && !command_subst_token)
            || (token.starts_with('\'') && token.ends_with('\'') && token.len() >= 2);
        if let Some(token) = token.strip_prefix(ARRAY_FIELD_SPLIT_MARKER) {
            let token = unquote_storage_value(token);
            if let Some(matches) = pathname_expand_array_token(&token) {
                for value in matches {
                    entries.insert(next_index, value);
                    next_index += 1;
                }
            } else {
                entries.insert(next_index, token);
                next_index += 1;
            }
            continue;
        }
        // GNU field splitting for compound assignment tokens: only
        // whitespace OUTSIDE quote pairs splits a field. `'a b'` is one
        // element, and so is `'a b'c` -- the quoted span continues the
        // same field after the closing quote (array6.sub
        // a2=(-iname 'abc -iname 'def) stores (-iname, "abc -iname def")).
        let split_needed = token_has_unquoted_whitespace(&token);
        // A token is "partially quoted" only if it contains an UNESCAPED
        // quote character. A `\"` from ANSI-C decoding ($'a"b') is data,
        // not a quote operator, so it must not trigger the
        // remove_shell_quotes path (issue #109: x=($'a"b') stored "ab"
        // instead of "a\"b").
        let partially_quoted = !quoted_token && has_unescaped_quote(&token);
        // GNU order (niubash #103 follow-up): quote removal on the raw
        // token, then restore the lexer carrier bytes, then unquote the
        // storage escaping. Unquoting first collapsed `x=(q\"q)` to `qq`
        // and left raw 0x18 carrier bytes in stored elements.
        let token = if partially_quoted
            && !(token.starts_with("$'") && token.ends_with('\''))
            && !token.starts_with('\x1d')
        {
            restore_quote_carriers(&remove_shell_quotes(&token))
        } else {
            token
        };
        let token = unquote_storage_value(&token);
        if let Some(expanded_array) = token.strip_prefix('\x1d') {
            for value in field_split_values_with_ifs(expanded_array, ifs) {
                if let Some(matches) = pathname_expand_array_token(&value) {
                    for value in matches {
                        entries.insert(next_index, value);
                        next_index += 1;
                    }
                } else {
                    entries.insert(next_index, value.to_string());
                    next_index += 1;
                }
            }
            continue;
        }
        if split_needed {
            for value in field_split_values_with_ifs(&token, ifs) {
                entries.insert(next_index, value.to_string());
                next_index += 1;
            }
            continue;
        }
        if scalar_append && !entries.is_empty() {
            let current = entries.get(&0).cloned().unwrap_or_default();
            let appended = if integer {
                (eval_arith_value(&current) + eval_arith_value(&token)).to_string()
            } else {
                append_scalar_value(&current, &token)
            };
            entries.insert(0, appended);
        } else {
            entries.insert(next_index, token);
            next_index += 1;
        }
    }

    if integer {
        for element in entries.values_mut() {
            *element = eval_arith_value(element).to_string();
        }
    }

    format_indexed_array_storage(entries)
}

pub(super) fn array_assignment_index(
    left: &str,
    entries: &BTreeMap<usize, String>,
    env_vars: &HashMap<String, String>,
) -> Option<usize> {
    let expression = left.strip_prefix('[')?.strip_suffix(']')?;
    let index = eval_conditional_arith_value(expression, env_vars)?;
    if index >= 0 {
        return usize::try_from(index).ok();
    }
    let max_index = entries.keys().next_back().copied()?;
    let resolved = i128::try_from(max_index)
        .ok()?
        .checked_add(1)?
        .checked_add(index)?;
    usize::try_from(resolved).ok()
}

pub(super) fn array_assignment_has_subscript(left: &str) -> bool {
    left.strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
        .is_some()
}

/// True when the raw token has whitespace outside every quote pair (GNU
/// field splitting for compound assignment tokens): `'a b'` has none (one
/// field), `'a b'c d` does (split after the quoted span), and
/// `'a b'c` does not (the quoted span continues the same field).
fn token_has_unquoted_whitespace(token: &str) -> bool {
    let mut single = false;
    let mut double = false;
    let mut escaped = false;
    for ch in token.chars() {
        if escaped {
            escaped = false;
            continue;
        }
        match ch {
            '\\' if !single => escaped = true,
            '\'' if !double => single = !single,
            '"' if !single => double = !double,
            c if c.is_whitespace() && !single && !double => return true,
            _ => {}
        }
    }
    false
}

/// Check if the token contains an unescaped quote character (single or
/// double). A `\"` (backslash-escaped double quote) is data, not a quote
/// operator, so it does not count. This prevents `remove_shell_quotes`
/// from stripping data quotes that came from ANSI-C decoding (issue #109:
/// x=($'a"b') stored "ab" instead of "a\"b").
fn has_unescaped_quote(token: &str) -> bool {
    let mut escaped = false;
    for ch in token.chars() {
        if escaped {
            escaped = false;
            continue;
        }
        match ch {
            '\\' => escaped = true,
            '\'' | '"' => return true,
            _ => {}
        }
    }
    false
}

pub(super) fn array_assignment_tokens(value: &str) -> Vec<String> {
    let Some(inner) = value
        .strip_prefix('(')
        .and_then(|value| value.strip_suffix(')'))
    else {
        return if value.is_empty() {
            Vec::new()
        } else {
            vec![value.to_string()]
        };
    };

    let tokens: Vec<String> = split_storage_words(inner).collect();
    tokens
}

pub(super) fn array_parameter_slice(
    value: &str,
    offset: isize,
    length: Option<usize>,
) -> Vec<String> {
    // GNU array_subrange (array.c:377-419) with the offset resolution from
    // subst.c:8432-8449: the offset is an ARRAY INDEX, not a position in the
    // value list. The slice is every element whose index >= start; unset
    // holes are skipped and empty-string elements still count toward the
    // length. A negative offset counts back from one past the maximum index
    // (subst.c:8446: len = array_max_index(a) + (*e1p < 0); *e1p += len),
    // and a start below the smallest index yields nothing.
    let entries = indexed_array_entries(value);
    if entries.is_empty() {
        return Vec::new();
    }
    let max_index = *entries.keys().next_back().expect("non-empty entries");
    let start: i128 = if offset < 0 {
        max_index as i128 + 1 + offset as i128
    } else {
        offset as i128
    };
    if start < 0 {
        return Vec::new();
    }

    entries
        .into_iter()
        .filter(|(index, _)| *index as i128 >= start)
        .take(length.unwrap_or(usize::MAX))
        .map(|(_, element)| element)
        .collect()
}

pub(super) fn slice_array_values(
    values: Vec<String>,
    offset: isize,
    length: Option<usize>,
) -> Vec<String> {
    let start = if offset < 0 {
        values.len().saturating_sub(offset.unsigned_abs())
    } else {
        offset as usize
    };

    values
        .into_iter()
        .skip(start)
        .take(length.unwrap_or(usize::MAX))
        .collect()
}

pub(super) fn is_noassign_bash_array(name: &str) -> bool {
    matches!(
        name,
        "BASH_ARGC" | "BASH_ARGV" | "BASH_LINENO" | "BASH_SOURCE" | "FUNCNAME" | "PIPESTATUS"
    )
}
