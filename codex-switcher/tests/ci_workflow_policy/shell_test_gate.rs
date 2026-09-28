//! Shell tests that a required job must run. No Rust test executes the
//! repository's shell tests: `tests/log_permissions_and_uninstall.sh` alone
//! exercises `scripts/uninstall.sh`, and the `tests/install_*.sh` checks guard
//! the installer's staging, signing, and rollback order. While such a test ran
//! only locally, a change could break what it guards and still pass every
//! required check. Every tracked `*.sh` file under `tests/`, at any depth, is a
//! shell test, so adding one fails this rule until a required job runs it.
//! Each gate is a block-style step of a required job whose `run:` is exactly
//! `bash <script>` from the repository root, whose `if:` is exactly
//! `SHELL_TEST_CONDITION`, and which sets no other step key but `name`. The
//! rule reads only the step; `required_checks.rs` keeps its job from being
//! skipped and `inherited_settings.rs` rejects a default shell or exported
//! variable that could change the command.
//!
//! A helper or fixture script under `tests/` is required as a test too. A line
//! reader cannot tell it from a test, nor prove that a gated test really
//! sources or runs it, so a carve-out could hide a test that never runs. The
//! violation for a script no step mentions says to move such a file instead.

use crate::git_repo::GitRepo;
use crate::workflow_jobs::{job_steps, jobs};
use crate::yaml_lines::{entry, nested};

/// Git pathspec of the shell tests. Its `*` also matches `/`, so it lists
/// scripts in subdirectories of `tests/` too.
const SHELL_TEST_PATHSPEC: &str = "tests/*.sh";
/// Keys a shell-test step may set. A `working-directory` would make the path
/// name another file, and `shell`, `env`, or `continue-on-error` could change
/// or mask the command, so any other key fails closed.
const SHELL_TEST_STEP_KEYS: [&str; 3] = ["name", "if", "run"];
/// The `if:` every gate sets. Under the default `success()`, one failing step
/// skips every later step of its job, so a failing test would hide the result
/// of each shell test after it. `!cancelled()` still runs after a failure and,
/// unlike `always()`, stops once the run is cancelled; a failed gate still
/// fails its job either way. A leading `!` would start a YAML tag, hence the
/// `${{ }}` spelling GitHub documents.
pub const SHELL_TEST_CONDITION: &str = "${{ !cancelled() }}";

/// Tracked shell tests, relative to the repository root. Tracked includes a
/// staged new test, so the rule fails before that test is committed; an
/// untracked local script is not in a clean checkout and is not required.
pub fn shell_tests(repo: &GitRepo) -> Result<Vec<String>, String> {
    repo.tracked_files(SHELL_TEST_PATHSPEC)
}

/// The rule as the live policy applies it: the shell tests `repo` tracks,
/// each gated in `text`. A listing that fails or comes back empty is itself a
/// violation, because it would otherwise require nothing.
pub fn repository_shell_test_violations(
    repo: &GitRepo,
    text: &str,
    contexts: &[String],
) -> Vec<String> {
    match shell_tests(repo) {
        Ok(scripts) if scripts.is_empty() => vec![format!(
            "git lists no shell tests for `{SHELL_TEST_PATHSPEC}`, so none would be required"
        )],
        Ok(scripts) => shell_test_gate_violations(text, contexts, &scripts),
        Err(e) => vec![format!("cannot list the shell tests: {e}")],
    }
}

/// One violation for each of `scripts` that no required job runs as a gate.
pub fn shell_test_gate_violations(
    text: &str,
    contexts: &[String],
    scripts: &[String],
) -> Vec<String> {
    scripts
        .iter()
        .filter_map(|script| missing_gate(text, contexts, script))
        .collect()
}

/// Whether `bash <script>` names exactly `script` as one plain word of ASCII
/// letters, digits, `/`, `.`, `_`, and `-`. A name with a space, quote, `#`,
/// `$`, or glob character would be split, expanded, or cut short by YAML or
/// the shell, so no gate could run it as written.
fn plain_word(script: &str) -> bool {
    script
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'/' | b'.' | b'_' | b'-'))
}

/// `None` when a required job runs `script` as a gate, otherwise the
/// violation, with the reason each step that mentions `script` is not one.
fn missing_gate(text: &str, contexts: &[String], script: &str) -> Option<String> {
    if !plain_word(script) {
        return Some(format!(
            "shell test `{script}` cannot run as `bash <script>`; rename it with only \
             ASCII letters, digits, `/`, `.`, `_`, and `-`"
        ));
    }
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
    let mut message = format!(
        "no required job runs `{script}`, a step with only `name`, \
         `if: {SHELL_TEST_CONDITION}`, and `run: {command}`"
    );
    if near_misses.is_empty() {
        message.push_str(&format!(
            "; if `{script}` is a helper or fixture rather than a test, move it out of \
             `tests/` and keep its `.sh` suffix so the locked-cargo scan still reads it"
        ));
    }
    for near_miss in near_misses {
        message.push_str("; ");
        message.push_str(&near_miss);
    }
    Some(message)
}

/// Why the step whose direct property lines are `step` is not a gate that
/// runs exactly `command` under `SHELL_TEST_CONDITION`.
fn gate_problem(lines: &[&str], step: &[usize], command: &str) -> Option<String> {
    let mut runs = Vec::new();
    let mut conditions = Vec::new();
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
        match e.key {
            "run" => runs.push(e.value),
            "if" => conditions.push(e.value),
            _ => {}
        }
    }
    if runs != [command] {
        return Some(format!("`run:` is not set once to exactly `{command}`"));
    }
    (conditions != [SHELL_TEST_CONDITION]).then(|| {
        format!(
            "`if:` is not set once to exactly `{SHELL_TEST_CONDITION}`, so an earlier \
             failure in the job would skip it"
        )
    })
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
