//! Minimal line-oriented reader for the block-style YAML used in
//! `.github/workflows`. The crate has no YAML dependency; the policy rules fail
//! closed on shapes this reader does not understand (for example flow mappings).

pub fn indent(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

pub fn is_content(line: &str) -> bool {
    let t = line.trim();
    !t.is_empty() && !t.starts_with('#')
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

pub fn entry(line: &str) -> Option<Entry<'_>> {
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

/// Content lines indented deeper than `lines[at]`, up to the first dedent.
pub fn block_after<'a>(lines: &[&'a str], at: usize) -> Vec<&'a str> {
    let base = indent(lines[at]);
    lines[at + 1..]
        .iter()
        .copied()
        .filter(|l| is_content(l))
        .take_while(|l| indent(l) > base)
        .collect()
}

/// Block of the first top-level (`indent == 0`) key named `key`.
pub fn top_level_block<'a>(lines: &[&'a str], key: &str) -> Option<Vec<&'a str>> {
    let at = lines
        .iter()
        .position(|l| indent(l) == 0 && entry(l).is_some_and(|e| e.key == key))?;
    Some(block_after(lines, at))
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
