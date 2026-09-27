use super::{shell_test_gate_violations, REQUIRED_SHELL_TESTS};
use crate::fixtures::{compliant, with};
use crate::required_checks::required_check_violations;
use crate::rules::workflow_violations;

/// The compliant fixture's only step in the required `Build` job.
const BUILD_STEP: &str = "      - run: cargo test --locked\n";
const SCRIPT: &str = REQUIRED_SHELL_TESTS[0];

fn contexts() -> Vec<String> {
    vec!["Build".to_string(), "Lint".to_string()]
}

fn violations(text: &str) -> Vec<String> {
    shell_test_gate_violations(text, &contexts())
}

fn gate_step() -> String {
    format!("      - name: Uninstall Shell Test\n        run: bash {SCRIPT}\n")
}

/// The compliant workflow with `step` added after the `Build` job's step.
fn with_step(step: &str) -> String {
    with(BUILD_STEP, &format!("{BUILD_STEP}{step}"))
}

/// `with_step(gate_step())` after replacing `from` with `to` in the gate.
fn gate_with(from: &str, to: &str) -> String {
    let step = gate_step();
    assert!(step.contains(from), "gate step lacks {from:?}");
    with_step(&step.replacen(from, to, 1))
}

/// One violation that reports the missing gate and contains `reason`.
fn assert_rejected(text: &str, reason: &str) {
    let v = violations(text);
    assert_eq!(v.len(), 1, "{text}\n{v:?}");
    assert!(
        v[0].contains(&format!("no required job runs `{SCRIPT}`")),
        "{v:?}"
    );
    assert!(v[0].contains(reason), "{reason:?}: {v:?}");
}

const RUN_REASON: &str = "`run:` is not set once to exactly";

#[test]
fn gate_step_in_a_required_job_complies() {
    let text = with_step(&gate_step());
    assert_eq!(violations(&text), Vec::<String>::new());
    // The gated fixture still meets every other rule.
    assert_eq!(workflow_violations(&text), Vec::<String>::new());
    assert_eq!(
        required_check_violations(&text, &contexts(), "main"),
        Vec::<String>::new()
    );
    // Any required job may run it.
    let in_lint = format!("{}{}", compliant(), gate_step());
    assert_eq!(violations(&in_lint), Vec::<String>::new());
}

/// Regression: the uninstall shell test existed but no CI job ran it, so an
/// uninstall regression could pass every required check.
#[test]
fn a_workflow_that_never_runs_the_shell_test_is_rejected() {
    assert_rejected(
        &compliant(),
        "`run: bash tests/log_permissions_and_uninstall.sh`",
    );
}

#[test]
fn the_gate_in_a_job_that_is_not_required_is_rejected() {
    let text = format!(
        "{}  extra:\n    name: Extra\n    runs-on: macos-14\n    timeout-minutes: 5\n    steps:\n{}",
        compliant(),
        gate_step()
    );
    assert_rejected(&text, "job `extra` is not a required check");
}

#[test]
fn a_step_key_that_can_move_change_or_skip_the_command_is_rejected() {
    for key in [
        "        working-directory: codex-switcher\n",
        "        shell: sh\n",
        "        env:\n          HOME: /\n",
        "        if: always()\n",
        "        continue-on-error: true\n",
        "        timeout-minutes: 1\n",
    ] {
        let text = with_step(&format!("{}{key}", gate_step()));
        let name = key.trim().split(':').next().unwrap();
        assert_rejected(&text, &format!("sets `{name}`; the step may set only"));
    }
}

#[test]
fn any_other_command_is_rejected() {
    for command in [
        format!("./{SCRIPT}"),
        format!("sh {SCRIPT}"),
        format!("bash -x {SCRIPT}"),
        format!("bash {SCRIPT} --skip-uninstall"),
        format!("bash {SCRIPT} || exit 0"),
        format!("echo bash {SCRIPT}"),
        "bash tests/install_bundle_swap.sh".to_string(),
    ] {
        let text = gate_with(&format!("run: bash {SCRIPT}"), &format!("run: {command}"));
        let v = violations(&text);
        assert_eq!(v.len(), 1, "{command}: {v:?}");
        if command.contains(SCRIPT) {
            assert!(v[0].contains(RUN_REASON), "{command}: {v:?}");
        }
    }
}

#[test]
fn a_block_scalar_or_repeated_run_is_rejected() {
    let block = gate_with(
        &format!("run: bash {SCRIPT}\n"),
        &format!("run: |\n          bash {SCRIPT}\n"),
    );
    assert_rejected(&block, RUN_REASON);
    let twice = with_step(&format!("{}        run: bash {SCRIPT}\n", gate_step()));
    assert_rejected(&twice, RUN_REASON);
}

#[test]
fn a_quoted_command_is_read_as_its_value() {
    let text = gate_with(
        &format!("run: bash {SCRIPT}"),
        &format!("run: \"bash {SCRIPT}\""),
    );
    assert_eq!(violations(&text), Vec::<String>::new());
}
