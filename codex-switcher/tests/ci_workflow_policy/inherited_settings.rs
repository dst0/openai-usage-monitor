//! Settings the steps of a required job inherit can change what their
//! commands do without touching the steps: a `defaults.run.shell` such as
//! `true {0}` turns every `run:` into a no-op, and a variable such as
//! `RUSTFLAGS` or `CARGO_ENCODED_RUSTFLAGS` can allow or cap every Clippy lint.
//! A step that appends to `$GITHUB_ENV` or `$GITHUB_PATH` does the same for
//! every later step. So the required-check workflow may export only the
//! variables in `INHERITED_ENV_KEYS` from its top level or a required job, set
//! no default shell there, and never name those runner files.

use crate::workflow_jobs::jobs;
use crate::yaml_lines::{direct_members, entry, nested, raw_value, top_level_index};

/// Variables the workflow or a required job may export to all of its steps.
const INHERITED_ENV_KEYS: [&str; 2] = ["CARGO_INCREMENTAL", "CARGO_TERM_COLOR"];
/// Runner files whose lines become the environment and `PATH` of later steps.
const RUNNER_ENV_FILES: [&str; 2] = ["GITHUB_ENV", "GITHUB_PATH"];

pub fn inherited_setting_violations(text: &str, contexts: &[String]) -> Vec<String> {
    let lines: Vec<&str> = text.lines().collect();
    let mut scopes: Vec<(String, usize)> = ["env", "defaults"]
        .into_iter()
        .filter_map(|key| top_level_index(&lines, key))
        .map(|at| ("the workflow".to_string(), at))
        .collect();
    for job in jobs(text) {
        if !job
            .prop("name")
            .is_some_and(|n| contexts.iter().any(|c| c == n))
        {
            continue;
        }
        for key in ["env", "defaults"] {
            if let Some(at) = job.prop_line(key) {
                scopes.push((format!("required job `{}`", job.id), at));
            }
        }
    }
    let mut out = Vec::new();
    for (scope, at) in scopes {
        let key = entry(lines[at]).map_or("", |e| e.key);
        if raw_value(lines[at]) != Some("") {
            out.push(format!(
                "`{key}:` of {scope} must be a block mapping the policy can read"
            ));
        } else if key == "env" {
            out.extend(env_violations(&lines, at, &scope));
        } else if nested(&lines, at)
            .into_iter()
            .any(|i| entry(lines[i]).is_some_and(|e| e.key == "shell"))
        {
            out.push(format!(
                "`defaults` of {scope} sets `shell`, which decides what every `run:` executes"
            ));
        }
    }
    for (i, line) in lines.iter().enumerate() {
        if let Some(file) = RUNNER_ENV_FILES.iter().find(|f| line.contains(*f)) {
            out.push(format!(
                "line {}: `{file}` changes the environment of later required steps",
                i + 1
            ));
        }
    }
    out
}

/// Variables under the `env:` on `lines[at]` that are not allowed to reach
/// every step of `scope`.
fn env_violations(lines: &[&str], at: usize, scope: &str) -> Vec<String> {
    direct_members(lines, at)
        .into_iter()
        .filter_map(|i| {
            let key = entry(lines[i]).filter(|e| !e.list_item).map(|e| e.key);
            match key {
                Some(key) if INHERITED_ENV_KEYS.contains(&key) => None,
                Some(key) => Some(format!(
                    "{scope} exports `{key}` to every step; only {} may be inherited",
                    INHERITED_ENV_KEYS.join(" and ")
                )),
                None => Some(format!(
                    "line {}: `env` of {scope} holds an entry the policy cannot read",
                    i + 1
                )),
            }
        })
        .collect()
}

#[cfg(test)]
#[path = "inherited_settings.test.rs"]
mod tests;
