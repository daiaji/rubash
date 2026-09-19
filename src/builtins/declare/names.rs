/// GNU general.c:317 valid_array_reference: a nameref value (or declared
/// nameref name) may be `name[subscript]` with a valid identifier base and a
/// non-empty subscript. The full GNU version also validates quoted subscripts
/// and `[@]`/`[*]` forms; declare-time checks only need the reject side, so a
/// conservative shape test is enough here.
pub(super) fn valid_array_reference(arg: &str) -> bool {
    let Some(open) = arg.find('[') else {
        return false;
    };
    if !arg.ends_with(']') {
        return false;
    }
    let base = &arg[..open];
    let subscript = &arg[open + 1..arg.len() - 1];
    !subscript.is_empty() && valid_identifier(base)
}

/// GNU general.c:310 valid_nameref_value: a nameref value must be a valid
/// identifier or (flags != 2) a valid array reference.
pub(super) fn valid_nameref_value(value: &str) -> bool {
    !value.is_empty() && (valid_identifier(value) || valid_array_reference(value))
}

/// GNU general.c:327 check_selfref: the value references the nameref variable
/// itself, either directly (`declare -n x=x`) or through an array element of
/// the same array (`declare -n x=x[1]`).
pub(super) fn check_selfref(name: &str, value: &str) -> bool {
    if name == value {
        return true;
    }
    if valid_array_reference(value) {
        if let Some(open) = value.find('[') {
            return &value[..open] == name;
        }
    }
    false
}

pub(super) fn declare_base_name(arg: &str) -> Option<&str> {
    let name = arg.split_once('=').map(|(name, _)| name).unwrap_or(arg);
    let name = name.strip_suffix('+').unwrap_or(name);
    let name = name.split_once('[').map(|(name, _)| name).unwrap_or(name);
    valid_identifier(name).then_some(name)
}

pub(super) fn valid_declare_name(arg: &str) -> bool {
    // GNU builtins validate the EXPANDED word: `declare 'm[x[y]=a'` reaches
    // declare_builtin as `m[x[y]=a`, whose nested `[` makes skipsubscript
    // fail (tokenize_array_reference, arrayfunc.c:1288 -> skip_matched_pair
    // subst.c:2086). Quote/escape characters in the expanded text still act
    // as skip_matched_pair quote syntax; valid_array_reference always runs
    // the flag-0 matched-pair check (declare.def:597).
    let name = arg.split_once('=').map(|(name, _)| name).unwrap_or(arg);
    let name = name.strip_suffix('+').unwrap_or(name);
    if let Some((base, subscript)) = name.split_once('[') {
        if !valid_identifier(base) || subscript.len() < 2 {
            return false;
        }
        return subscript_closes_at_end(subscript);
    }
    valid_identifier(name)
}

fn subscript_closes_at_end(subscript: &str) -> bool {
    let bytes = subscript.as_bytes();
    let mut depth = 0usize;
    let mut single = false;
    let mut double = false;
    let mut index = 0usize;
    while index < bytes.len() {
        let ch = bytes[index];
        if ch == b'\\' && !single {
            index += 2;
            continue;
        }
        if single {
            if ch == b'\'' {
                single = false;
            }
            index += 1;
            continue;
        }
        if double {
            if ch == b'"' {
                double = false;
            }
            index += 1;
            continue;
        }
        match ch {
            b'\'' => single = true,
            b'"' => double = true,
            b'`' => {
                index += 1;
                while index < bytes.len() && bytes[index] != b'`' {
                    index += if bytes[index] == b'\\' { 2 } else { 1 };
                }
            }
            b'$' if bytes.get(index + 1) == Some(&b'(') || bytes.get(index + 1) == Some(&b'{') => {
                index = skip_dollar_pair(bytes, index + 1);
                continue;
            }
            b'[' => depth += 1,
            b']' if depth == 0 => return index == bytes.len() - 1,
            b']' => depth -= 1,
            _ => {}
        }
        index += 1;
    }
    false
}

fn skip_dollar_pair(bytes: &[u8], open_pos: usize) -> usize {
    let (open, close) = if bytes[open_pos] == b'(' {
        (b'(', b')')
    } else {
        (b'{', b'}')
    };
    let mut depth = 1usize;
    let mut single = false;
    let mut double = false;
    let mut index = open_pos + 1;
    while index < bytes.len() {
        let ch = bytes[index];
        if ch == b'\\' && !single {
            index += 2;
            continue;
        }
        if single {
            if ch == b'\'' {
                single = false;
            }
            index += 1;
            continue;
        }
        if double {
            if ch == b'"' {
                double = false;
            }
            index += 1;
            continue;
        }
        match ch {
            b'\'' => single = true,
            b'"' => double = true,
            b'`' => {
                index += 1;
                while index < bytes.len() && bytes[index] != b'`' {
                    index += if bytes[index] == b'\\' { 2 } else { 1 };
                }
            }
            _ if ch == open => depth += 1,
            _ if ch == close => {
                depth -= 1;
                if depth == 0 {
                    return index + 1;
                }
            }
            _ => {}
        }
        index += 1;
    }
    bytes.len()
}

pub(super) fn valid_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first == '_' || first.is_ascii_alphabetic())
        && chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}
