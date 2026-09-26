//! A required job must lint every target, tests included, on the pinned
//! toolchain. Without that step, `cargo clippy --all-targets` findings
//! accumulated unnoticed until a toolchain change exposed them. The gate is a
//! block-style step of a required job, whose `run:` is exactly
//! `CLIPPY_GATE_COMMAND` from `codex-switcher/`, where `rust-toolchain.toml`
//! applies, and which sets no other step key. The rule reads only the step;
//! `required_checks.rs` keeps its job from being skipped,
//! `inherited_settings.rs` rejects a default shell or exported variable that
//! could change the command, and `yaml_limits.rs` rejects YAML that would make
//! libyaml read a different command than this rule does. Repository files
//! Clippy reads, such as `.cargo/config.toml`, `Cargo.toml` `[lints]`, and
//! crate-level lint attributes, are outside this workflow scan.

use crate::workflow_jobs::{job_steps, jobs};
use crate::yaml_lines::{entry, nested};

/// The gate command, identical to the local command in AGENTS.md. Any other
/// spelling fails closed: extra flags could allow or cap lints after the
/// deny, and `--manifest-path` from elsewhere bypasses the toolchain pin.
pub const CLIPPY_GATE_COMMAND: &str =
    "cargo clippy --workspace --all-targets --locked -- -D warnings";
/// Directory whose `rust-toolchain.toml` pins the Clippy release.
const CLIPPY_GATE_DIRECTORY: &str = "codex-switcher";
/// Keys the gate step may set. A step-level `env`, `shell`, `if`, or
/// `continue-on-error` can weaken or skip the command, so any key outside
/// this list fails closed; the job's `timeout-minutes` bounds the step.
const CLIPPY_GATE_STEP_KEYS: [&str; 3] = ["name", "working-directory", "run"];

pub fn clippy_gate_violations(text: &str, contexts: &[String]) -> Vec<String> {
    let lines: Vec<&str> = text.lines().collect();
    let mut near_misses = Vec::new();
    for job in jobs(text) {
        let required = job
            .prop("name")
            .is_some_and(|n| contexts.iter().any(|c| c == n));
        for step in job_steps(&lines, &job).unwrap_or_default() {
            let problem = match gate_problem(&lines, &step) {
                None if required => return Vec::new(),
                None => format!("job `{}` is not a required check", job.id),
                Some(problem) => problem,
            };
            if mentions_clippy(&lines, &step) {
                near_misses.push(format!("step at line {}: {problem}", step[0] + 1));
            }
        }
    }
    let mut message = format!(
        "no required job runs the Clippy gate, a step with only \
         `working-directory: {CLIPPY_GATE_DIRECTORY}` and `run: {CLIPPY_GATE_COMMAND}`"
    );
    for near_miss in near_misses {
        message.push_str("; ");
        message.push_str(&near_miss);
    }
    vec![message]
}

/// Why the step whose direct property lines are `step` is not the gate.
fn gate_problem(lines: &[&str], step: &[usize]) -> Option<String> {
    let mut properties = Vec::new();
    for &i in step {
        let Some(e) = entry(lines[i]) else {
            return Some(format!(
                "has a key the policy cannot read at line {}",
                i + 1
            ));
        };
        if !CLIPPY_GATE_STEP_KEYS.contains(&e.key) {
            return Some(format!(
                "sets `{}`; the gate may set only {}",
                e.key,
                CLIPPY_GATE_STEP_KEYS.map(|k| format!("`{k}`")).join(", ")
            ));
        }
        properties.push(e);
    }
    // The value of `key` when the step sets it exactly once.
    let only = |key: &str| {
        let mut values = properties.iter().filter(|e| e.key == key).map(|e| e.value);
        match (values.next(), values.next()) {
            (Some(value), None) => Some(value),
            _ => None,
        }
    };
    if only("run") != Some(CLIPPY_GATE_COMMAND) {
        return Some(format!(
            "`run:` is not set once to exactly `{CLIPPY_GATE_COMMAND}`"
        ));
    }
    if only("working-directory") != Some(CLIPPY_GATE_DIRECTORY) {
        return Some(format!(
            "`working-directory:` is not set once to exactly `{CLIPPY_GATE_DIRECTORY}`"
        ));
    }
    None
}

/// Whether any line of the step, including nested values, mentions Clippy.
fn mentions_clippy(lines: &[&str], step: &[usize]) -> bool {
    step.iter()
        .flat_map(|&i| std::iter::once(i).chain(nested(lines, i)))
        .any(|i| lines[i].to_ascii_lowercase().contains("clippy"))
}

#[cfg(test)]
#[path = "clippy_gate.test.rs"]
mod tests;
