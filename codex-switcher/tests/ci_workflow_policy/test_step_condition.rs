//! Test steps of a required job that must report after an earlier failure.
//! Under GitHub's implicit `success()`, one failing step skips every later
//! step of its job, so a Clippy failure hid the unit-test results and one
//! failing shell test hid those after it. Such a step sets
//! `TEST_STEP_CONDITION`, which runs it after a failure and, unlike
//! `always()`, not in a cancelled run; a failed step still fails its job.
//! `shell_test_gate.rs` requires it on every shell-test gate, and
//! `unit_test_condition_violations` on every step of a required job whose
//! `run:` runs `cargo test`, however it is spelled.

use crate::locked_cargo::cargo_commands;
use crate::workflow_jobs::{job_steps, jobs};
use crate::yaml_lines::{entry, nested};

/// The `if:` of a test step that runs after an earlier failure. A leading `!`
/// would start a YAML tag, hence the `${{ }}` spelling GitHub documents.
pub const TEST_STEP_CONDITION: &str = "${{ !cancelled() }}";
/// Cargo options before the subcommand whose value is the next word, so that
/// value is not read as the subcommand (`cargo --color test build`).
const CARGO_VALUE_OPTIONS: [&str; 5] = ["--color", "--config", "--explain", "-C", "-Z"];

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

/// Whether the cargo command `words`, starting with `cargo`, runs the tests:
/// its first word that is neither an option nor an option's value is `test`
/// or cargo's built-in alias `t` (`cargo --locked test`, `cargo -v t`).
fn runs_tests(words: &[String]) -> bool {
    let mut rest = words.iter().skip(1);
    while let Some(word) = rest.next() {
        if CARGO_VALUE_OPTIONS.contains(&word.as_str()) {
            rest.next();
        } else if !word.starts_with(['-', '+']) {
            return matches!(word.as_str(), "test" | "t");
        }
    }
    false
}

/// One violation for each step of a required job whose `run:` runs
/// `cargo test` without `TEST_STEP_CONDITION`.
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
            let run_lines: Vec<&str> = step
                .iter()
                .filter(|&&i| entry(lines[i]).is_some_and(|e| e.key == "run"))
                .flat_map(|&i| std::iter::once(i).chain(nested(&lines, i)))
                .map(|i| lines[i])
                .collect();
            let commands = cargo_commands(&run_lines.join("\n"));
            if !commands.iter().any(|(_, words)| runs_tests(words)) {
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
                    "required job `{}` step at line {} runs `cargo test`: {problem}",
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
