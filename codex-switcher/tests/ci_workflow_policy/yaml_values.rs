//! Scalar and list values for rules that compare names, such as trigger
//! events and branch filters. Only conservative one-line shapes are read;
//! anything else is `None`, and the calling rule fails closed.

use crate::yaml_lines::{block_after, indent, raw_value};

/// A fully quoted scalar's contents, or a plain scalar of name characters
/// (letters, digits, `-_./*`). `None` for tags, aliases, flow syntax,
/// embedded quotes, or anything else that is not a single name.
pub fn scalar_item(text: &str) -> Option<&str> {
    let t = text.trim();
    let first = *t.as_bytes().first()?;
    if first == b'"' || first == b'\'' {
        let quote = first as char;
        let inner = t[1..].strip_suffix(quote)?;
        return (!inner.is_empty() && !inner.contains(quote)).then_some(inner);
    }
    let plain = (first.is_ascii_alphanumeric() || first == b'_')
        && t.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_./*".contains(&b));
    plain.then_some(t)
}

/// Items of a one-line flow sequence such as `[ main, 'release/**' ]`.
pub fn flow_items(value: &str) -> Option<Vec<&str>> {
    let inner = value.trim().strip_prefix('[')?.strip_suffix(']')?;
    if inner.trim().is_empty() {
        return Some(Vec::new());
    }
    inner.split(',').map(scalar_item).collect()
}

/// Scalar of a `- item` block sequence line.
pub fn sequence_item(line: &str) -> Option<&str> {
    let item = line.trim_start().strip_prefix("- ")?;
    scalar_item(item.split(" #").next().unwrap_or(""))
}

/// Items of the list under the key on `lines[at]`: a one-line flow sequence,
/// or a block sequence of scalars indented past the key. `None` for a lone
/// scalar, a null value, or any other shape.
pub fn list_value<'a>(lines: &[&'a str], at: usize) -> Option<Vec<&'a str>> {
    let value = raw_value(lines[at])?;
    if !value.is_empty() {
        return flow_items(value);
    }
    let block = block_after(lines, at);
    let column = indent(block.first()?);
    block
        .into_iter()
        .map(|line| (indent(line) == column).then(|| sequence_item(line))?)
        .collect()
}

#[cfg(test)]
#[path = "yaml_values.test.rs"]
mod tests;
