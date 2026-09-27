//! Shell tests that a required job must run. No Rust test executes
//! `scripts/uninstall.sh`; only `tests/log_permissions_and_uninstall.sh` does,
//! against fake homes and faked system commands. While it ran only locally, a
//! change could break uninstall cleanup and still pass every required check.
//! Each gate is a block-style step of a required job whose `run:` is exactly
//! `bash <script>` from the repository root and which sets no other step key.
//! The rule reads only the step; `required_checks.rs` keeps its job from being
//! skipped and `inherited_settings.rs` rejects a default shell or exported
//! variable that could change the command.

use crate::workflow_jobs::{job_steps, jobs};
use crate::yaml_lines::{entry, nested};

/// Repository shell tests, relative to the repository root, that a required
/// job must run on every pull request.
pub const REQUIRED_SHELL_TESTS: [&str; 1] = ["tests/log_permissions_and_uninstall.sh"];
/// Keys a shell-test step may set. A `working-directory` would make the path
/// name another file, and `shell`, `env`, `if`, or `continue-on-error` could
/// change or skip the command, so any other key fails closed.
const SHELL_TEST_STEP_KEYS: [&str; 2] = ["name", "run"];

pub fn shell_test_gate_violations(text: &str, contexts: &[String]) -> Vec<String> {
    REQUIRED_SHELL_TESTS
        .iter()
        .filter_map(|script| missing_gate(text, contexts, script))
        .collect()
}

/// `None` when a required job runs `script` as a gate, otherwise the
/// violation, with the reason each step that mentions `script` is not one.
fn missing_gate(text: &str, contexts: &[String], script: &str) -> Option<String> {
    let lines: Vec<&str> = text.lines().collect();
    let command = format!("bash {script}");
    let mut near_misses = Vec::new();
    for job in jobs(text) {
        let required = job
            .prop("name")
            .is_some_and(|n| contexts.iter().any(|c| c == n));
        for step in job_steps(&lines, &job).unwrap_or_default() {
            let problem = match gate_problem(&lines, &step, &command) {
                None if required => return None,
                None => format!("job `{}` is not a required check", job.id),
                Some(problem) => problem,
            };
            if mentions(&lines, &step, script) {
                near_misses.push(format!("step at line {}: {problem}", step[0] + 1));
            }
        }
    }
    let mut message =
        format!("no required job runs `{script}`, a step with only `name` and `run: {command}`");
    for near_miss in near_misses {
        message.push_str("; ");
        message.push_str(&near_miss);
    }
    Some(message)
}

/// Why the step whose direct property lines are `step` is not a gate that
/// runs exactly `command`.
fn gate_problem(lines: &[&str], step: &[usize], command: &str) -> Option<String> {
    let mut runs = Vec::new();
    for &i in step {
        let Some(e) = entry(lines[i]) else {
            return Some(format!(
                "has a key the policy cannot read at line {}",
                i + 1
            ));
        };
        if !SHELL_TEST_STEP_KEYS.contains(&e.key) {
            return Some(format!(
                "sets `{}`; the step may set only {}",
                e.key,
                SHELL_TEST_STEP_KEYS.map(|k| format!("`{k}`")).join(", ")
            ));
        }
        if e.key == "run" {
            runs.push(e.value);
        }
    }
    (runs != [command]).then(|| format!("`run:` is not set once to exactly `{command}`"))
}

/// Whether any line of the step, including nested values, names `script`.
fn mentions(lines: &[&str], step: &[usize], script: &str) -> bool {
    step.iter()
        .flat_map(|&i| std::iter::once(i).chain(nested(lines, i)))
        .any(|i| lines[i].contains(script))
}

#[cfg(test)]
#[path = "shell_test_gate.test.rs"]
mod tests;
