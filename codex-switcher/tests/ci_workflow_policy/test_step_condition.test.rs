use super::{condition_problem, unit_test_condition_violations, TEST_STEP_CONDITION};
use crate::fixtures::{compliant, with};

/// The compliant fixture's unit-test step in the required `Build` job.
const TEST_STEP: &str = "      - run: cargo test --locked\n";

fn contexts() -> Vec<String> {
    vec!["Build".to_string(), "Lint".to_string()]
}

fn conditioned(condition: &str) -> String {
    with(
        TEST_STEP,
        &format!("      - if: {condition}\n        run: cargo test --locked\n"),
    )
}

#[test]
fn only_the_exact_condition_set_once_passes() {
    assert_eq!(condition_problem(&[TEST_STEP_CONDITION]), None);
    for conditions in [
        vec![],
        vec![TEST_STEP_CONDITION, TEST_STEP_CONDITION],
        vec!["always()"],
        vec!["success()"],
        vec!["!cancelled()"],
        vec!["${{ !cancelled() && github.event_name == 'push' }}"],
    ] {
        let problem = condition_problem(&conditions).expect("rejected");
        assert!(
            problem.contains("`if:` is not set once to exactly `${{ !cancelled() }}`"),
            "{conditions:?}: {problem}"
        );
    }
}

/// Regression: the unit tests ran under the default `success()`, so a Clippy
/// failure earlier in the job hid whether they passed.
#[test]
fn a_required_unit_test_step_that_an_earlier_failure_would_skip_is_rejected() {
    let v = unit_test_condition_violations(&compliant(), &contexts());
    assert_eq!(v.len(), 1, "{v:?}");
    assert!(
        v[0].contains("required job `build` step at line 20 runs `cargo test`"),
        "{v:?}"
    );
    for other in ["success()", "always()", "failure()"] {
        let v = unit_test_condition_violations(&conditioned(other), &contexts());
        assert_eq!(v.len(), 1, "{other}: {v:?}");
    }
}

#[test]
fn a_conditioned_unit_test_step_complies() {
    let text = conditioned(TEST_STEP_CONDITION);
    assert_eq!(
        unit_test_condition_violations(&text, &contexts()),
        Vec::<String>::new()
    );
    // A block-scalar command and a trailing comment are read too.
    let block = with(
        TEST_STEP,
        &format!(
            "      - name: Test\n        if: {TEST_STEP_CONDITION} # why\n        run: |\n          cargo test --locked --verbose\n"
        ),
    );
    assert_eq!(
        unit_test_condition_violations(&block, &contexts()),
        Vec::<String>::new()
    );
    let unconditioned_block =
        block.replacen(&format!("if: {TEST_STEP_CONDITION} # why\n        "), "", 1);
    assert_eq!(
        unit_test_condition_violations(&unconditioned_block, &contexts()).len(),
        1
    );
}

/// A test command is found by its subcommand, not by `test` directly after
/// `cargo`: global options, their values, and cargo's `t` alias count too.
#[test]
fn every_spelling_of_a_test_command_needs_the_condition() {
    for command in [
        "cargo --locked test",
        "cargo --frozen test",
        "cargo -v test --locked",
        "cargo --color always test --locked",
        "cargo --config net.offline=true test --locked",
        "cargo t --locked",
        "env CARGO_TERM_COLOR=never cargo test --locked",
        "cargo build --locked && cargo test --locked",
    ] {
        let text = with(TEST_STEP, &format!("      - run: {command}\n"));
        let v = unit_test_condition_violations(&text, &contexts());
        assert_eq!(v.len(), 1, "{command}: {v:?}");
    }
    // An option's value is not the subcommand, and nothing after `--` is.
    for command in [
        "cargo --color test build --locked",
        "cargo --config t build --locked",
        "cargo run --locked -- test",
        "cargo clippy --locked --all-targets",
        "cargo testing",
    ] {
        let text = with(TEST_STEP, &format!("      - run: {command}\n"));
        let v = unit_test_condition_violations(&text, &contexts());
        assert_eq!(v, Vec::<String>::new(), "{command}");
    }
}

/// Only the command a step runs counts: a step whose other values read like a
/// test command, which the shell reader would take for one, is not a test
/// step.
#[test]
fn only_the_run_command_makes_a_test_step() {
    let text = with(
        TEST_STEP,
        "      - name: Build\n        env:\n          NEXT: cargo test --locked\n        run: cargo build --locked\n",
    );
    assert_eq!(
        unit_test_condition_violations(&text, &contexts()),
        Vec::<String>::new()
    );
}

/// Only unit-test steps of required jobs are in scope: other commands, and
/// jobs that are not required checks, need no condition.
#[test]
fn other_steps_and_jobs_that_are_not_required_are_out_of_scope() {
    let only_lint = vec!["Lint".to_string()];
    assert_eq!(
        unit_test_condition_violations(&compliant(), &only_lint),
        Vec::<String>::new()
    );
    let build_only = with(TEST_STEP, "      - run: cargo build --locked\n");
    assert_eq!(
        unit_test_condition_violations(&build_only, &contexts()),
        Vec::<String>::new()
    );
}

/// Wiring on the live repository: every required unit-test step carries the
/// condition, and dropping it from the live `ci.yml` is reported.
#[test]
fn live_unit_test_step_loses_its_condition_when_removed() {
    let ci = crate::read(".github/workflows/ci.yml");
    let live_contexts =
        crate::rules::required_check_contexts(&crate::read("scripts/setup-github-protection.sh"));
    assert_eq!(
        unit_test_condition_violations(&ci, &live_contexts),
        Vec::<String>::new()
    );
    let lines: Vec<&str> = ci.lines().collect();
    let test_run = lines
        .iter()
        .position(|l| l.trim() == "run: cargo test --locked --verbose")
        .expect("live unit-test step");
    let step = crate::workflow_jobs::step_properties(&lines, test_run).expect("step");
    let condition = *step
        .iter()
        .find(|&&i| crate::yaml_lines::entry(lines[i]).is_some_and(|e| e.key == "if"))
        .expect("live unit-test step sets `if:`");
    let without: Vec<&str> = lines
        .iter()
        .enumerate()
        .filter(|&(i, _)| i != condition)
        .map(|(_, l)| *l)
        .collect();
    let v = unit_test_condition_violations(&without.join("\n"), &live_contexts);
    assert_eq!(v.len(), 1, "{v:?}");
}
