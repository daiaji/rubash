pub(in crate::builtins::declare) fn split_storage_words(
    value: &str,
) -> impl Iterator<Item = String> + '_ {
    StorageWordIter {
        input: value,
        offset: 0,
    }
}

struct StorageWordIter<'a> {
    input: &'a str,
    offset: usize,
}

impl Iterator for StorageWordIter<'_> {
    type Item = String;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(ch) = self.input.get(self.offset..)?.chars().next() {
            if !ch.is_ascii_whitespace() {
                break;
            }
            self.offset += ch.len_utf8();
        }

        let mut word = String::new();
        let mut in_double = false;
        let mut in_single = false;
        let mut escaped = false;
        for (relative, ch) in self.input[self.offset..].char_indices() {
            if escaped {
                word.push(ch);
                escaped = false;
                continue;
            }
            if ch == '\\' && in_double {
                word.push(ch);
                escaped = true;
                continue;
            }
            // GNU parse.y:5368-5397 read_token_word: a backslash outside
            // any quote removes itself and keeps the next char literal. We
            // keep the backslash in the token so pathname expansion can see
            // it and skip globbing; unquote_storage_value removes it later.
            if ch == '\\' && !in_double && !in_single {
                word.push(ch);
                escaped = true;
                continue;
            }
            // Expansion-produced whitespace tagged by the compound walker
            // (embedded_mutations expansion_ws_marked): glue the marker and
            // its whitespace into the word so assoc kv-pairs keep them;
            // indexed callers re-split on the marker. The \x1c
            // IFS-protection sentinel takes the same glued form here.
            if ch == '\x1c' || ch == crate::executor::COMPOUND_EXPANSION_WS_TAG {
                word.push(ch);
                escaped = true;
                continue;
            }
            if ch == '\'' && !in_double {
                in_single = !in_single;
                word.push(ch);
                continue;
            }
            if ch == '"' && !in_single {
                in_double = !in_double;
                word.push(ch);
                continue;
            }
            if ch.is_ascii_whitespace() && !in_double && !in_single {
                self.offset += relative + ch.len_utf8();
                return Some(word);
            }
            word.push(ch);
        }
        self.offset = self.input.len();
        (!word.is_empty()).then_some(word)
    }
}

pub(in crate::builtins::declare) fn unquote_storage_value(value: &str) -> String {
    if let Some(inner) = value
        .strip_prefix("$'")
        .and_then(|value| value.strip_suffix('\''))
    {
        return unquote_ansi_c_storage(inner);
    }

    let Some(inner) = value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
    else {
        // Bare value (not wrapped in quotes): GNU expand_word_internal
        // quote removal removes backslashes outside any quote, keeping
        // the next char literal (e.g. `\for` -> `for`, `\*` -> `*`).
        let bare = value
            .strip_prefix('\'')
            .and_then(|value| value.strip_suffix('\''))
            .unwrap_or(value);
        let mut decoded = String::new();
        let mut escaped = false;
        for ch in bare.chars() {
            if escaped {
                decoded.push(ch);
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else {
                decoded.push(ch);
            }
        }
        if escaped {
            decoded.push('\\');
        }
        // \x1c is the expansion-whitespace tag (expansion_ws_marked): the
        // whitespace it precedes is data, the tag itself is not.
        return decoded.replace('\x1c', "").replace(crate::executor::COMPOUND_EXPANSION_WS_TAG, "");
    };

    let mut unquoted = String::new();
    let mut escaped = false;
    for ch in inner.chars() {
        if escaped {
            unquoted.push(ch);
            escaped = false;
        } else if ch == '\\' {
            escaped = true;
        } else {
            unquoted.push(ch);
        }
    }
    if escaped {
        unquoted.push('\\');
    }
    unquoted.replace('\x1c', "").replace(crate::executor::COMPOUND_EXPANSION_WS_TAG, "")
}

fn unquote_ansi_c_storage(value: &str) -> String {
    let mut output = String::new();
    let mut chars = value.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            output.push(ch);
            continue;
        }
        match chars.next() {
            Some('n') => output.push('\n'),
            Some('r') => output.push('\r'),
            Some('t') => output.push('\t'),
            Some('\\') => output.push('\\'),
            Some('\x27') => output.push('\x27'),
            Some(other) => output.push(other),
            None => output.push('\\'),
        }
    }
    output
}

/// GNU arrayfunc.c:610 expand_words_no_vars field-splits every indexed
/// compound element's expansion, so the \x1c-tagged expansion whitespace
/// (embedded_mutations expansion_ws_marked) is a split boundary for
/// indexed arrays even though the same bytes stay glued for associative
/// words. Empty fields drop like GNU's field splitting.
pub(in crate::builtins::declare) fn split_indexed_tagged_token(token: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut chars = token.chars().peekable();
    while let Some(ch) = chars.next() {
        if (ch == '\x1c' || ch == crate::executor::COMPOUND_EXPANSION_WS_TAG) && matches!(chars.peek(), Some(' ' | '\t' | '\n')) {
            chars.next();
            if !current.is_empty() {
                parts.push(std::mem::take(&mut current));
            }
            continue;
        }
        current.push(ch);
    }
    if !current.is_empty() {
        parts.push(current);
    }
    parts
}

pub(in crate::builtins::declare) fn parse_array_tokens(value: &str) -> Vec<String> {
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
    split_storage_words(inner).collect()
}
