//! Jobs and steps of a workflow as the line reader sees them: each job's
//! direct properties with their line numbers, and each step's direct
//! properties. Deeper lines belong to a property's value.

use crate::yaml_lines::{
    direct_members, entry, indent, is_content, is_list_item, key_column, nested, raw_value,
    top_level_index,
};

#[derive(Debug, PartialEq)]
pub struct Job {
    pub id: String,
    /// Direct `(key, value, line index)` properties of the job, not of its
    /// steps.
    pub props: Vec<(String, String, usize)>,
}

impl Job {
    pub fn prop(&self, key: &str) -> Option<&str> {
        self.props
            .iter()
            .find(|(k, _, _)| k == key)
            .map(|(_, v, _)| v.as_str())
    }

    /// Line index of the direct property `key`.
    pub fn prop_line(&self, key: &str) -> Option<usize> {
        self.props
            .iter()
            .find(|(k, _, _)| k == key)
            .map(|&(_, _, line)| line)
    }
}

pub fn jobs(text: &str) -> Vec<Job> {
    let lines: Vec<&str> = text.lines().collect();
    let Some(at) = top_level_index(&lines, "jobs") else {
        return Vec::new();
    };
    let body = nested(&lines, at);
    let job_indent = body.first().map_or(0, |&i| indent(lines[i]));
    let mut out: Vec<Job> = Vec::new();
    let mut prop_indent = None;
    for i in body {
        let line = lines[i];
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
            job.props.push((e.key.to_string(), e.value.to_string(), i));
        }
    }
    out
}

/// Lines of the job `id`, its header included, joined with `\n`. `None`
/// when the workflow has no such job.
pub fn job_text(text: &str, id: &str) -> Option<String> {
    let lines: Vec<&str> = text.lines().collect();
    let body = nested(&lines, top_level_index(&lines, "jobs")?);
    let job_indent = indent(lines[*body.first()?]);
    let is_header = |i: &usize| indent(lines[*i]) == job_indent;
    let start = body
        .iter()
        .position(|i| is_header(i) && entry(lines[*i]).is_some_and(|e| e.key == id))?;
    let end = body[start + 1..]
        .iter()
        .position(is_header)
        .map_or(body.len(), |n| start + 1 + n);
    let (first, last) = (body[start], body.get(end).map_or(lines.len(), |&i| i));
    Some(lines[first..last].join("\n"))
}

/// Indices of the direct properties of the step whose property is on
/// `lines[at]`: its `- ` marker line plus later lines at the same key column.
/// `None` when no list-item marker owns that line.
pub fn step_properties(lines: &[&str], at: usize) -> Option<Vec<usize>> {
    let column = key_column(lines[at]);
    let is_list_item = |i: usize| lines[i].trim_start().starts_with("- ");
    let outside = |i: &usize| is_content(lines[*i]) && indent(lines[*i]) < column;
    let start = (0..=at).rev().find(outside)?;
    if !is_list_item(start) || key_column(lines[start]) != column {
        return None;
    }
    let end = (start + 1..lines.len())
        .find(outside)
        .unwrap_or(lines.len());
    let siblings = (start + 1..end)
        .filter(|&i| is_content(lines[i]) && indent(lines[i]) == column && !is_list_item(i));
    Some(std::iter::once(start).chain(siblings).collect())
}

/// Direct property lines of each step of `job`. `None` when `steps:` is not a
/// block list whose items are block-mapping steps.
pub fn job_steps(lines: &[&str], job: &Job) -> Option<Vec<Vec<usize>>> {
    let Some(at) = job.prop_line("steps") else {
        return Some(Vec::new());
    };
    if raw_value(lines[at]) != Some("") {
        return None;
    }
    direct_members(lines, at)
        .into_iter()
        .map(|marker| {
            is_list_item(lines[marker])
                .then(|| step_properties(lines, marker))
                .flatten()
        })
        .collect()
}
