//! Test steps of a required job that must report after an earlier failure.
//! Under GitHub's implicit `success()`, one failing step skips every later
//! step of its job, so a Clippy failure hid the unit-test results and one
//! failing shell test hid those after it. Such a step sets
//! `TEST_STEP_CONDITION`, which runs it after a failure and, unlike
//! `always()`, not in a cancelled run; a failed step still fails its job.
//! `shell_test_gate.rs` requires it on every shell-test gate, and
//! `unit_test_condition_violations` on every `cargo test --locked` step of a
//! required job.

use crate::locked_cargo::runs_locked;
use crate::workflow_jobs::{job_steps, jobs};
use crate::yaml_lines::{entry, nested};

/// The `if:` of a test step that runs after an earlier failure. A leading `!`
/// would start a YAML tag, hence the `${{ }}` spelling GitHub documents.
pub const TEST_STEP_CONDITION: &str = "${{ !cancelled() }}";

/// Why a step whose `if:` values are `conditions` does not run after an
/// earlier failure as a test step must; `None` when it does.
pub fn condition_problem(conditions: &[&str]) -> Option<String> {
    (conditions != [TEST_STEP_CONDITION]).then(|| {
        format!(
            "`if:` is not set once to exactly `{TEST_STEP_CONDITION}`, which runs the step \
             after an earlier failure but not in a cancelled run"
        )
    })
}

/// One violation for each step of a required job that runs
/// `cargo test --locked` without `TEST_STEP_CONDITION`.
pub fn unit_test_condition_violations(text: &str, contexts: &[String]) -> Vec<String> {
    let lines: Vec<&str> = text.lines().collect();
    let mut out = Vec::new();
    for job in jobs(text) {
        let required = job
            .prop("name")
            .is_some_and(|n| contexts.iter().any(|c| c == n));
        if !required {
            continue;
        }
        for step in job_steps(&lines, &job).unwrap_or_default() {
            let step_lines: Vec<&str> = step
                .iter()
                .flat_map(|&i| std::iter::once(i).chain(nested(&lines, i)))
                .map(|i| lines[i])
                .collect();
            if !runs_locked(&step_lines.join("\n"), "test") {
                continue;
            }
            let conditions: Vec<&str> = step
                .iter()
                .filter_map(|&i| entry(lines[i]))
                .filter(|e| e.key == "if")
                .map(|e| e.value)
                .collect();
            if let Some(problem) = condition_problem(&conditions) {
                out.push(format!(
                    "required job `{}` step at line {} runs `cargo test --locked`: {problem}",
                    job.id,
                    step[0] + 1
                ));
            }
        }
    }
    out
}

#[cfg(test)]
#[path = "test_step_condition.test.rs"]
mod tests;
