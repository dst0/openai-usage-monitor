//! Minimal line-oriented reader for the block-style YAML used in
//! `.github/workflows`. The crate has no YAML dependency; the policy rules fail
//! closed on shapes this reader does not understand (for example flow mappings),
//! and `yaml_limits.rs` rejects lines that would make it misread the rest.

pub fn indent(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

pub fn is_content(line: &str) -> bool {
    let t = line.trim();
    !t.is_empty() && !t.starts_with('#')
}

/// Column where the line's key or scalar starts, after any `- ` list marker.
/// Properties of a list-item mapping share this column.
pub fn key_column(line: &str) -> usize {
    match line.trim_start().strip_prefix('-') {
        Some(rest) if rest.starts_with(' ') => line.len() - rest.trim_start().len(),
        _ => indent(line),
    }
}

/// One `key: value` line with list prefix, key/value quotes and trailing
/// comment removed.
#[derive(Debug, PartialEq)]
pub struct Entry<'a> {
    pub key: &'a str,
    pub value: &'a str,
    pub comment: &'a str,
    pub list_item: bool,
}

fn unquote(s: &str) -> &str {
    for q in ['"', '\''] {
        if let Some(inner) = s.strip_prefix(q).and_then(|r| r.strip_suffix(q)) {
            return inner;
        }
    }
    s
}

/// `(key, text after the colon, list_item)` of a `key: value` line.
fn split_entry(line: &str) -> Option<(&str, &str, bool)> {
    let mut t = line.trim_start();
    let list_item = is_list_item(t);
    if list_item {
        t = t[1..].trim_start();
    }
    let (key, rest) = match t.chars().next()? {
        q @ ('"' | '\'') => {
            let end = t[1..].find(q)? + 1;
            (&t[1..end], &t[end + 1..])
        }
        _ => {
            let end = t.find(':')?;
            (&t[..end], &t[end..])
        }
    };
    let key_ok = !key.is_empty()
        && key
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
    let rest = rest.strip_prefix(':')?;
    if !key_ok || !(rest.is_empty() || rest.starts_with([' ', '\t'])) {
        return None;
    }
    Some((key, rest, list_item))
}

pub fn entry(line: &str) -> Option<Entry<'_>> {
    let (key, rest, list_item) = split_entry(line)?;
    let (value, comment) = match rest.find(" #") {
        Some(i) => (&rest[..i], rest[i + 2..].trim()),
        None => (rest, ""),
    };
    Some(Entry {
        key,
        value: unquote(value.trim()),
        comment,
        list_item,
    })
}

/// Value of a `key: value` line with its quotes kept and comment removed, for
/// rules that must tell `[main]` from `"[main]"`.
pub fn raw_value(line: &str) -> Option<&str> {
    let (_, rest, _) = split_entry(line)?;
    Some(rest.split(" #").next().unwrap_or("").trim())
}

/// Whether the line starts with a `- ` sequence marker.
pub fn is_list_item(line: &str) -> bool {
    let t = line.trim_start();
    t.starts_with("- ") || t == "-"
}

/// Indices of the content lines nested under the key on `lines[at]`: those
/// indented past its key column, plus the `- ` items of a compact sequence at
/// that column when the key has no inline value, up to the first line that
/// is neither.
pub fn nested(lines: &[&str], at: usize) -> Vec<usize> {
    let base = key_column(lines[at]);
    let compact = raw_value(lines[at]) == Some("");
    (at + 1..lines.len())
        .filter(|&i| is_content(lines[i]))
        .take_while(|&i| {
            indent(lines[i]) > base
                || (compact && indent(lines[i]) == base && is_list_item(lines[i]))
        })
        .collect()
}

/// Content lines nested under the key on `lines[at]`.
pub fn block_after<'a>(lines: &[&'a str], at: usize) -> Vec<&'a str> {
    nested(lines, at).into_iter().map(|i| lines[i]).collect()
}

/// Indices of the direct members of the block under `lines[at]`: nested lines
/// at the first nested line's indentation. Deeper lines belong to a member,
/// and so do the `- ` items of a member's compact sequence, which share the
/// members' indentation in a mapping.
pub fn direct_members(lines: &[&str], at: usize) -> Vec<usize> {
    let nested = nested(lines, at);
    let Some(&first) = nested.first() else {
        return Vec::new();
    };
    let column = indent(lines[first]);
    let sequence = is_list_item(lines[first]);
    nested
        .into_iter()
        .filter(|&i| indent(lines[i]) == column && (sequence || !is_list_item(lines[i])))
        .collect()
}

/// Index of the first top-level (`indent == 0`) key named `key`.
pub fn top_level_index(lines: &[&str], key: &str) -> Option<usize> {
    lines
        .iter()
        .position(|l| indent(l) == 0 && entry(l).is_some_and(|e| e.key == key))
}

/// Block of the first top-level (`indent == 0`) key named `key`.
pub fn top_level_block<'a>(lines: &[&'a str], key: &str) -> Option<Vec<&'a str>> {
    Some(block_after(lines, top_level_index(lines, key)?))
}
