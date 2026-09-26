//! `actions/checkout` must not leave the job token in `.git/config`. The action
//! persists credentials unless its own `persist-credentials` input is false, so
//! only that input, as a direct member of the step's single `with:` block,
//! counts. The same text under `env:`, inside another input's value, or in a
//! nested mapping does not.

use crate::yaml_lines::{direct_members, entry, indent, is_content, key_column};

const MISSING: &str =
    "must set `persist-credentials: false` exactly once, directly under its `with:`";

/// Every `actions/checkout` step must set `persist-credentials: false`.
pub fn checkout_credential_violations(text: &str) -> Vec<String> {
    let lines: Vec<&str> = text.lines().collect();
    (0..lines.len())
        .filter(|&i| entry(lines[i]).is_some_and(|e| e.key == "uses" && is_checkout(e.value)))
        .filter_map(|i| {
            credential_problem(&lines, i).map(|p| format!("checkout at line {}: {p}", i + 1))
        })
        .collect()
}

/// Whether `uses:` names the `actions/checkout` repository, at any path or
/// ref. GitHub compares owner and repository names case-insensitively.
fn is_checkout(reference: &str) -> bool {
    let mut segments = reference.split('@').next().unwrap_or("").split('/');
    segments
        .next()
        .is_some_and(|owner| owner.eq_ignore_ascii_case("actions"))
        && segments
            .next()
            .is_some_and(|repo| repo.eq_ignore_ascii_case("checkout"))
}

/// Why the checkout step owning `lines[at]` may persist credentials, if it may.
fn credential_problem(lines: &[&str], at: usize) -> Option<&'static str> {
    let Some(properties) = step_properties(lines, at) else {
        return Some("is not a block-style step list item the policy can read");
    };
    let with_blocks: Vec<usize> = properties
        .into_iter()
        .filter(|&i| entry(lines[i]).is_some_and(|e| e.key == "with"))
        .collect();
    let [with_at] = with_blocks[..] else {
        return Some(MISSING);
    };
    if entry(lines[with_at]).is_some_and(|e| !e.value.is_empty()) {
        return Some("`with:` must be a block mapping, not a flow mapping or alias");
    }
    let settings: Vec<&str> = direct_members(lines, with_at)
        .into_iter()
        .filter_map(|i| entry(lines[i]))
        .filter(|e| !e.list_item && e.key == "persist-credentials")
        .map(|e| e.value)
        .collect();
    (settings != ["false"]).then_some(MISSING)
}

/// Indices of the direct properties of the step whose property is on
/// `lines[at]`: its `- ` marker line plus later lines at the same key column.
/// `None` when no list-item marker owns that line.
fn step_properties(lines: &[&str], at: usize) -> Option<Vec<usize>> {
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

#[cfg(test)]
#[path = "checkout.test.rs"]
mod tests;
