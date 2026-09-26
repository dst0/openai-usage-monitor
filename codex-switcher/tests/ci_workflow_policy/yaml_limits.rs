//! Lines whose meaning the line reader in `yaml_lines.rs` cannot see. A quoted
//! scalar or flow collection left open at the end of a line turns the lines
//! after it into text, a `<<` merge key copies keys from another node, an
//! explicit `?` key hides its key from `entry`, an anchor, alias, or tag
//! changes what a key or value is, a double-quoted escape spells a different
//! name (`"u\x73es"` is `uses`), and a lone carriage return, NEL, U+2028, or
//! U+2029 is a line break to libyaml but not to `str::lines`. libyaml-based
//! parsers accept all of these, so the reader would see keys a workflow does
//! not have, or miss keys it does. The policy rejects such lines instead of
//! misreading them.

use crate::yaml_lines::{indent, is_content, key_column};

/// Characters libyaml treats as line breaks but `str::lines` does not.
const FOREIGN_LINE_BREAKS: [char; 4] = ['\r', '\u{85}', '\u{2028}', '\u{2029}'];

pub fn unreadable_line_violations(text: &str) -> Vec<String> {
    unreadable_lines(text)
        .into_iter()
        .map(|n| {
            format!(
                "line {n}: YAML the policy scan cannot read; keep quoted and flow values \
                 on one line (or use a `|` block scalar), use `\\n` line endings, and avoid \
                 `?` and `<<` keys, anchors, aliases, tags, and double-quoted escapes"
            )
        })
        .collect()
}

/// 1-based numbers of lines with a foreign line break anywhere, and of
/// content lines outside block scalars that open a node continuing past the
/// line, use an explicit or merge key, carry an anchor, alias, or tag, or
/// hold a double-quoted escape in a key or value.
pub fn unreadable_lines(text: &str) -> Vec<usize> {
    let mut out = Vec::new();
    let mut block_scalar_base = None;
    for (i, line) in text.lines().enumerate() {
        if line.contains(FOREIGN_LINE_BREAKS) {
            out.push(i + 1);
            continue;
        }
        if !is_content(line) || block_scalar_base.is_some_and(|base| indent(line) > base) {
            continue;
        }
        block_scalar_base = None;
        let node = node_text(line);
        let (base, value) = match value_after_key(node) {
            Some(value) => (key_column(line), value.trim_start()),
            None => (indent(line), node),
        };
        let mut unreadable = node == "?"
            || node.starts_with("? ")
            || node.starts_with("<<")
            || [node, value]
                .iter()
                .any(|s| has_node_property(s) || has_escape(s));
        if is_block_scalar_header(without_node_properties(value)) {
            block_scalar_base = Some(base);
        } else {
            unreadable |= stays_open(value);
        }
        if unreadable {
            out.push(i + 1);
        }
    }
    out
}

/// Whether `s` starts with an anchor (`&`), alias (`*`), or tag (`!`).
fn has_node_property(s: &str) -> bool {
    s.starts_with(['&', '*', '!'])
}

/// Whether `s` starts with a double-quoted scalar holding an escape that can
/// spell another character, such as `\x65` for `e` or `\/` for `/`. `\"` and
/// `\\` only yield characters no key or compared name contains, and are
/// common in JSON-valued inputs.
fn has_escape(s: &str) -> bool {
    let Some(end) = s.starts_with('"').then(|| quoted_end(s)).flatten() else {
        return false;
    };
    let inner = &s.as_bytes()[1..end - 1];
    let mut i = 0;
    while i < inner.len() {
        if inner[i] == b'\\' {
            if !matches!(inner.get(i + 1), Some(b'"' | b'\\')) {
                return true;
            }
            i += 1;
        }
        i += 1;
    }
    false
}

/// `value` after any leading anchors and tags, so an anchored block scalar
/// header is still recognised and its content skipped.
fn without_node_properties(value: &str) -> &str {
    let mut v = value;
    while v.starts_with(['&', '!']) {
        v = v
            .split_once([' ', '\t'])
            .map_or("", |(_, rest)| rest.trim_start());
    }
    v
}

/// The line without indentation and `- ` list markers.
fn node_text(line: &str) -> &str {
    let mut node = line.trim_start();
    while let Some(rest) = node.strip_prefix("- ") {
        node = rest.trim_start();
    }
    node
}

/// Text after the `key:` of a mapping entry; `None` for a lone scalar or an
/// unterminated quoted key.
fn value_after_key(node: &str) -> Option<&str> {
    let key_end = match node.as_bytes().first()? {
        b'"' | b'\'' => quoted_end(node)?,
        b'[' | b'{' => return None,
        _ => 0,
    };
    let bytes = node.as_bytes();
    for i in key_end..bytes.len() {
        let colon_ends_key = bytes[i] == b':'
            && bytes
                .get(i + 1)
                .is_none_or(|next| next.is_ascii_whitespace());
        if colon_ends_key {
            return Some(&node[i + 1..]);
        }
        let comment = bytes[i] == b'#' && (i == 0 || bytes[i - 1].is_ascii_whitespace());
        // After a quoted key only spaces may precede the colon.
        if comment || (key_end > 0 && !bytes[i].is_ascii_whitespace()) {
            return None;
        }
    }
    None
}

/// `|` or `>` with optional indentation/chomping indicators and comment.
fn is_block_scalar_header(value: &str) -> bool {
    let header = value.split(" #").next().unwrap_or("").trim_end();
    header.starts_with(['|', '>'])
        && header.len() <= 3
        && header[1..]
            .bytes()
            .all(|b| b.is_ascii_digit() || b == b'+' || b == b'-')
}

/// Byte length of the quoted scalar that opens `s`, if it closes on this line.
fn quoted_end(s: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    let quote = bytes[0];
    let mut i = 1;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' if quote == b'"' => i += 1,
            b'\'' if quote == b'\'' && bytes.get(i + 1) == Some(&b'\'') => i += 1,
            b if b == quote => return Some(i + 1),
            _ => {}
        }
        i += 1;
    }
    None
}

/// Whether a quoted scalar or flow collection starting `value` is still open
/// at the end of the line. Quotes open only where a scalar may start, so a
/// plain value such as `echo it's` is not mistaken for one.
fn stays_open(value: &str) -> bool {
    let bytes = value.as_bytes();
    let mut depth = 0usize;
    let mut scalar_start = true;
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        match b {
            b'"' | b'\'' if scalar_start => match quoted_end(&value[i..]) {
                Some(len) => {
                    i += len;
                    scalar_start = false;
                    continue;
                }
                None => return true,
            },
            b'#' if i == 0 || bytes[i - 1].is_ascii_whitespace() => break,
            b'[' | b'{' if scalar_start || depth > 0 => depth += 1,
            b']' | b'}' if depth > 0 => depth -= 1,
            _ => {}
        }
        if !b.is_ascii_whitespace() {
            scalar_start = depth > 0 && matches!(b, b'[' | b'{' | b',' | b':');
        }
        i += 1;
    }
    depth > 0
}

#[cfg(test)]
#[path = "yaml_limits.test.rs"]
mod tests;
