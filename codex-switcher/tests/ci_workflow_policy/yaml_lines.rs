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
    let list_item = t.starts_with("- ") || t == "-";
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

/// Indices of the content lines nested under the key on `lines[at]`: those
/// indented past its key column, up to the first line that is not.
fn nested(lines: &[&str], at: usize) -> Vec<usize> {
    let base = key_column(lines[at]);
    (at + 1..lines.len())
        .filter(|&i| is_content(lines[i]))
        .take_while(|&i| indent(lines[i]) > base)
        .collect()
}

/// Content lines nested under the key on `lines[at]`.
pub fn block_after<'a>(lines: &[&'a str], at: usize) -> Vec<&'a str> {
    nested(lines, at).into_iter().map(|i| lines[i]).collect()
}

/// Indices of the direct members of the block under `lines[at]`: nested lines
/// at the first nested line's indentation. Deeper lines belong to a member.
pub fn direct_members(lines: &[&str], at: usize) -> Vec<usize> {
    let nested = nested(lines, at);
    let column = nested.first().map_or(0, |&i| indent(lines[i]));
    nested
        .into_iter()
        .filter(|&i| indent(lines[i]) == column)
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

#[derive(Debug, PartialEq)]
pub struct Job {
    pub id: String,
    /// Direct `key: value` properties of the job (not of its steps).
    pub props: Vec<(String, String)>,
}

impl Job {
    pub fn prop(&self, key: &str) -> Option<&str> {
        self.props
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }
}

pub fn jobs(text: &str) -> Vec<Job> {
    let lines: Vec<&str> = text.lines().collect();
    let Some(body) = top_level_block(&lines, "jobs") else {
        return Vec::new();
    };
    let job_indent = body.first().map_or(0, |l| indent(l));
    let mut out: Vec<Job> = Vec::new();
    let mut prop_indent = None;
    for line in body {
        if indent(line) == job_indent {
            let id = entry(line).map_or(line.trim(), |e| e.key).to_string();
            out.push(Job {
                id,
                props: Vec::new(),
            });
            prop_indent = None;
            continue;
        }
        let Some(job) = out.last_mut() else { continue };
        let at = *prop_indent.get_or_insert(indent(line));
        if indent(line) != at {
            continue;
        }
        if let Some(e) = entry(line).filter(|e| !e.list_item) {
            job.props.push((e.key.to_string(), e.value.to_string()));
        }
    }
    out
}
