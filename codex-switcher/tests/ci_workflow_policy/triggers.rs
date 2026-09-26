//! Triggers of the workflow whose jobs are required branch-protection checks.

use crate::yaml_lines::{entry, top_level_block};

/// Trigger filters that can keep a required check from reporting on a PR.
const TRIGGER_FILTERS: [&str; 3] = ["paths", "paths-ignore", "branches-ignore"];

pub fn trigger_filter_violations(text: &str) -> Vec<String> {
    let lines: Vec<&str> = text.lines().collect();
    top_level_block(&lines, "on")
        .unwrap_or_default()
        .into_iter()
        .filter_map(|line| entry(line).filter(|e| TRIGGER_FILTERS.contains(&e.key)))
        .map(|e| format!("trigger filter `{}` can hide required checks", e.key))
        .collect()
}
