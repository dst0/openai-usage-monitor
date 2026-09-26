use super::{clippy_gate_violations, CLIPPY_GATE_COMMAND};
use crate::fixtures::{compliant, with};
use crate::required_checks::required_check_violations;
use crate::rules::workflow_violations;

/// The compliant fixture's step in the required `Lint` job.
const LINT_STEP: &str = "      - name: Clippy\n        run: cargo clippy --locked\n";

fn contexts() -> Vec<String> {
    vec!["Build".to_string(), "Lint".to_string()]
}

fn violations(text: &str) -> Vec<String> {
    clippy_gate_violations(text, &contexts())
}

fn gate_step() -> String {
    format!(
        "      - name: Clippy\n        working-directory: codex-switcher\n        run: {CLIPPY_GATE_COMMAND}\n"
    )
}

/// The compliant workflow with its `Lint` step replaced by the canonical gate.
fn gated() -> String {
    with(LINT_STEP, &gate_step())
}

/// `gated()` after replacing `from` with `to` in the gate step.
fn gate_with(from: &str, to: &str) -> String {
    let step = gate_step();
    assert!(step.contains(from), "gate step lacks {from:?}");
    with(LINT_STEP, &step.replacen(from, to, 1))
}

/// 1-based number of the first line of `text` containing `needle`.
fn line_of(text: &str, needle: &str) -> usize {
    1 + text.lines().position(|l| l.contains(needle)).expect(needle)
}

/// One violation that reports the missing gate and contains `reason`.
fn assert_rejected(text: &str, reason: &str) {
    let v = violations(text);
    assert_eq!(v.len(), 1, "{text}\n{v:?}");
    assert!(
        v[0].contains("no required job runs the Clippy gate"),
        "{v:?}"
    );
    assert!(v[0].contains(reason), "{reason:?}: {v:?}");
}

/// The gate step's near miss is reported with its line and `reason`.
fn assert_near_miss(text: &str, reason: &str) {
    let line = line_of(text, "- name: Clippy");
    assert_rejected(text, &format!("step at line {line}: {reason}"));
}

const RUN_REASON: &str = "`run:` is not set once to exactly";

#[test]
fn canonical_gate_in_a_required_job_complies() {
    let text = gated();
    assert_eq!(violations(&text), Vec::<String>::new());
    // The gated fixture still meets every other rule.
    assert_eq!(workflow_violations(&text), Vec::<String>::new());
    assert_eq!(
        required_check_violations(&text, &contexts(), "main"),
        Vec::<String>::new()
    );
    // The gate may be any step of any required job.
    let in_build = with(
        "      - run: cargo test --locked\n",
        &format!("      - run: cargo test --locked\n{}", gate_step()),
    );
    assert_eq!(violations(&in_build), Vec::<String>::new());
}

/// Regression (PR #18): the Rust job printed `cargo clippy --version` but
/// never ran Clippy, so findings in test targets accumulated unnoticed.
#[test]
fn a_workflow_without_the_gate_is_rejected() {
    let text = compliant();
    assert_near_miss(&text, RUN_REASON);
    let version = with(LINT_STEP, "      - run: cargo clippy --version\n");
    let line = line_of(&version, "cargo clippy --version");
    assert_rejected(&version, &format!("step at line {line}: {RUN_REASON}"));
    // Without any step that mentions Clippy, no step is named.
    let v = violations(&with(LINT_STEP, ""));
    assert_eq!(v.len(), 1, "{v:?}");
    assert!(!v[0].contains("step at line"), "{v:?}");
}

#[test]
fn weakened_or_redirected_commands_are_not_the_gate() {
    for run in [
        // Test targets are not linted.
        "cargo clippy --workspace --locked -- -D warnings",
        // Warnings do not fail the step.
        "cargo clippy --workspace --all-targets --locked",
        "cargo clippy --workspace --all-targets --locked -- -W warnings",
        // Lints are allowed or capped after the deny.
        "cargo clippy --workspace --all-targets --locked -- -D warnings -A clippy::all",
        "cargo clippy --workspace --all-targets --locked -- -D warnings --cap-lints allow",
        // `--locked` is part of the canonical command.
        "cargo clippy --workspace --all-targets -- -D warnings",
        // Another toolchain, or a result that is discarded.
        "cargo +stable clippy --workspace --all-targets --locked -- -D warnings",
        "cargo clippy --workspace --all-targets --locked -- -D warnings; exit 0",
        "echo cargo clippy --workspace --all-targets --locked -- -D warnings",
    ] {
        assert_near_miss(&gate_with(CLIPPY_GATE_COMMAND, run), RUN_REASON);
    }
    // From the repository root, rustup does not apply the pinned toolchain.
    let manifest_path = gate_with(
        "        working-directory: codex-switcher\n        run: cargo clippy",
        "        run: cargo clippy --manifest-path codex-switcher/Cargo.toml",
    );
    assert_near_miss(&manifest_path, RUN_REASON);
}

#[test]
fn the_gate_runs_in_the_pinned_toolchain_directory() {
    for directory in [
        "",
        "        working-directory: .\n",
        "        working-directory: codex-switcher/src\n",
        "        working-directory: ${{ inputs.dir }}\n",
    ] {
        let text = gate_with("        working-directory: codex-switcher\n", directory);
        assert_near_miss(&text, "`working-directory:` is not set once to exactly");
    }
}

/// Any other step key can change what the command does or whether its
/// failure counts; the job's `timeout-minutes` already bounds the step.
#[test]
fn the_gate_step_sets_nothing_else() {
    let after = format!("        run: {CLIPPY_GATE_COMMAND}\n");
    for (extra, reason) in [
        (
            "        env:\n          RUSTFLAGS: --cap-lints=allow\n",
            "sets `env`",
        ),
        ("        shell: bash -c 'exit 0' {0}\n", "sets `shell`"),
        (
            "        continue-on-error: true\n",
            "sets `continue-on-error`",
        ),
        ("        if: failure()\n", "sets `if`"),
        ("        timeout-minutes: 15\n", "sets `timeout-minutes`"),
        // A second `run:` or `working-directory:` makes the step ambiguous.
        ("        run: echo skipped\n", RUN_REASON),
        ("        working-directory: .\n", "`working-directory:`"),
        // libyaml reads these keys; `entry` does not (PR #18 critic P1-4).
        (
            "        continue-on-error : true\n",
            "has a key the policy cannot read",
        ),
        (
            "        if : failure()\n",
            "has a key the policy cannot read",
        ),
    ] {
        assert_near_miss(&gate_with(&after, &format!("{after}{extra}")), reason);
    }
    let marker_condition = gate_with("      - name: Clippy\n", "      - if: failure()\n");
    let line = line_of(&marker_condition, "- if: failure()");
    assert_rejected(
        &marker_condition,
        &format!("step at line {line}: sets `if`"),
    );
}

/// A block scalar can hold several commands, and gate text outside a step
/// list item is not a step, so neither is read as the gate.
#[test]
fn unreadable_gate_shapes_fail_closed() {
    let block = gate_with(
        &format!("run: {CLIPPY_GATE_COMMAND}\n"),
        &format!("run: |\n          {CLIPPY_GATE_COMMAND}\n"),
    );
    assert_near_miss(&block, RUN_REASON);
    let job_level = with(
        "    timeout-minutes: 5\n",
        &format!("    timeout-minutes: 5\n    working-directory: codex-switcher\n    run: {CLIPPY_GATE_COMMAND}\n"),
    );
    assert_rejected(&job_level, "");
}

#[test]
fn quoting_comments_and_property_order_do_not_matter() {
    for (from, to) in [
        (
            CLIPPY_GATE_COMMAND.to_string(),
            format!("\"{CLIPPY_GATE_COMMAND}\""),
        ),
        (
            CLIPPY_GATE_COMMAND.to_string(),
            format!("'{CLIPPY_GATE_COMMAND}'"),
        ),
        (
            CLIPPY_GATE_COMMAND.to_string(),
            format!("{CLIPPY_GATE_COMMAND} # lint every target"),
        ),
        (
            "working-directory: codex-switcher".to_string(),
            "working-directory: 'codex-switcher'".to_string(),
        ),
        (
            "      - name: Clippy\n        working-directory: codex-switcher\n".to_string(),
            "      - working-directory: codex-switcher\n".to_string(),
        ),
    ] {
        let text = gate_with(&from, &to);
        assert_eq!(violations(&text), Vec::<String>::new(), "{text}");
    }
}

/// A gate in a job that is not a required check can be skipped or fail
/// without blocking a merge.
#[test]
fn the_gate_must_run_in_a_required_job() {
    let text = gated();
    let v = clippy_gate_violations(&text, &["Build".to_string()]);
    assert_eq!(v.len(), 1, "{v:?}");
    let line = line_of(&text, "- name: Clippy");
    assert!(
        v[0].contains(&format!(
            "step at line {line}: job `lint` is not a required check"
        )),
        "{v:?}"
    );
}

/// This rule reads only the step; the live test also runs the required-job
/// rule, which rejects a gate job that a condition can skip.
#[test]
fn a_skippable_gate_job_is_rejected_by_the_required_job_rule() {
    let text = gated().replacen(
        "    timeout-minutes: 5\n",
        "    timeout-minutes: 5\n    if: github.event_name == 'push'\n",
        1,
    );
    assert_eq!(violations(&text), Vec::<String>::new());
    let v = required_check_violations(&text, &contexts(), "main");
    assert_eq!(v.len(), 1, "{v:?}");
    assert!(v[0].contains("must not set `if`"), "{v:?}");
}

/// Regression (PR #18 critic P0-1, P0-2): libyaml runs `-A clippy::all` from
/// a deeper line that continues the plain `run:` value, and passes
/// `warnings\u{a0}` where `str::trim` shows the reader `warnings`. The
/// whole-workflow scan rejects both lines.
#[test]
fn hidden_command_text_fails_the_workflow_scan() {
    let run = format!("        run: {CLIPPY_GATE_COMMAND}\n");
    for (to, marker) in [
        (format!("{run}          -A clippy::all\n"), "-A clippy::all"),
        (
            run.replace("warnings\n", "warnings\u{a0}\n"),
            "warnings\u{a0}",
        ),
    ] {
        let text = gate_with(&run, &to);
        let line = line_of(&text, marker);
        let v = workflow_violations(&text);
        assert!(
            v.iter()
                .any(|m| m.starts_with(&format!("line {line}: ")) && m.contains("cannot read")),
            "{text}\n{v:?}"
        );
    }
}
