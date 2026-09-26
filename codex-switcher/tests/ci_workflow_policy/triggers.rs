//! Triggers of the workflow whose jobs are the required branch-protection
//! checks. Those checks report on a pull request only if this workflow runs
//! for it, so the workflow needs a `pull_request` trigger that no filter can
//! suppress for a pull request into the protected branch. Rejecting filters
//! alone is not enough: `on: push` has none and never runs for a PR.

use crate::yaml_lines::{block_after, direct_members, entry, raw_value, top_level_index};
use crate::yaml_values::{list_value, scalar_item};

/// Trigger filters that can keep a required check from reporting on a PR.
const TRIGGER_FILTERS: [&str; 3] = ["paths", "paths-ignore", "branches-ignore"];
/// Activity types `pull_request` runs for without a `types:` filter. A push
/// to an open pull request is `synchronize`.
const DEFAULT_ACTIVITY_TYPES: [&str; 3] = ["opened", "synchronize", "reopened"];
const MISSING: &str =
    "`on:` needs exactly one unconditional `pull_request` trigger so required checks report on every PR";

pub fn pull_request_trigger_violations(text: &str, protected_branch: &str) -> Vec<String> {
    let lines: Vec<&str> = text.lines().collect();
    let Some(on) = top_level_index(&lines, "on") else {
        return vec![MISSING.to_string()];
    };
    let mut out: Vec<String> = block_after(&lines, on)
        .into_iter()
        .filter_map(|line| entry(line).filter(|e| TRIGGER_FILTERS.contains(&e.key)))
        .map(|e| format!("trigger filter `{}` can hide required checks", e.key))
        .collect();
    let events = direct_members(&lines, on);
    let is_mapping = raw_value(lines[on]) == Some("")
        && events
            .first()
            .is_some_and(|&i| entry(lines[i]).is_some_and(|e| !e.list_item));
    if is_mapping {
        out.extend(event_mapping_violations(&lines, &events, protected_branch));
        return out;
    }
    match event_list(&lines, on) {
        Some(names) if names.contains(&"pull_request") => {}
        Some(_) => out.push(MISSING.to_string()),
        None => out.push(
            "`on:` must be an event name, a one-line list, a block list, or a block mapping"
                .to_string(),
        ),
    }
    out
}

/// Events of an `on:` written as one name, a one-line list, or a block list.
fn event_list<'a>(lines: &[&'a str], on: usize) -> Option<Vec<&'a str>> {
    let value = raw_value(lines[on])?;
    if value.is_empty() || value.starts_with('[') {
        list_value(lines, on)
    } else {
        scalar_item(value).map(|event| vec![event])
    }
}

/// `on:` as a block mapping of events: exactly one `pull_request`, empty or
/// with filters that keep every pull request into `protected_branch`.
fn event_mapping_violations(
    lines: &[&str],
    events: &[usize],
    protected_branch: &str,
) -> Vec<String> {
    let mut out = Vec::new();
    let mut pull_requests = Vec::new();
    for &i in events {
        match entry(lines[i]) {
            Some(e) if !e.list_item => {
                if e.key == "pull_request" {
                    pull_requests.push(i);
                }
            }
            _ => out.push(format!(
                "`on:` line {} is not an event key the policy can read",
                i + 1
            )),
        }
    }
    let [pull_request] = pull_requests[..] else {
        out.push(MISSING.to_string());
        return out;
    };
    if raw_value(lines[pull_request]) != Some("") {
        out.push("`pull_request:` must be empty or a block mapping of filters".to_string());
        return out;
    }
    for i in direct_members(lines, pull_request) {
        let Some(filter) = entry(lines[i]).filter(|e| !e.list_item) else {
            out.push(format!(
                "`pull_request` line {} is not a filter the policy can read",
                i + 1
            ));
            continue;
        };
        let items = list_value(lines, i);
        let problem = match filter.key {
            "branches" => (!items.is_some_and(|branches| {
                branches.contains(&protected_branch) && !branches.iter().any(|b| b.starts_with('!'))
            }))
            .then(|| {
                format!("`pull_request.branches` must list `{protected_branch}` and no `!` pattern")
            }),
            "types" => (!items
                .is_some_and(|types| DEFAULT_ACTIVITY_TYPES.iter().all(|t| types.contains(t))))
            .then(|| {
                "`pull_request.types` must keep `opened`, `synchronize`, and `reopened`".to_string()
            }),
            // Already reported by the scan of the whole `on:` block.
            key if TRIGGER_FILTERS.contains(&key) => None,
            key => Some(format!("unrecognized `pull_request` filter `{key}`")),
        };
        out.extend(problem);
    }
    out
}

#[cfg(test)]
#[path = "triggers.test.rs"]
mod tests;
