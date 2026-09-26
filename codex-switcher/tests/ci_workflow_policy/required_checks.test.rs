use super::{required_check_violations, reused_context_violations};
use crate::fixtures::{compliant, with};

const CLIPPY_STEP: &str = "      - name: Clippy\n        run: cargo clippy\n";
const TEST_STEP: &str = "      - run: cargo test\n";
/// A job that is skipped on pull requests (critic case j01).
const GATE_JOB: &str = "  gate:\n    runs-on: macos-14\n    timeout-minutes: 5\n    if: github.event_name == 'push'\n    steps:\n      - run: echo gate\n";

fn contexts() -> Vec<String> {
    vec!["Build".to_string(), "Lint".to_string()]
}

fn violations(text: &str) -> Vec<String> {
    required_check_violations(text, &contexts(), "main")
}

/// The compliant workflow plus the gate job, with `needs` added to `build`.
fn build_needing(needs: &str) -> String {
    with(CLIPPY_STEP, &format!("{CLIPPY_STEP}{GATE_JOB}")).replacen(
        "    name: Build\n",
        &format!("    name: Build\n{needs}"),
        1,
    )
}

/// Regression (review of PR #13, critic j01): a required job that needs a
/// job skipped on pull requests is skipped too, and GitHub counts a skipped
/// required check as passing.
#[test]
fn required_jobs_need_only_other_required_jobs() {
    for needs in [
        "    needs: gate\n",
        "    needs: [ gate ]\n",
        "    needs: [ lint, gate ]\n",
        "    needs:\n      - gate\n",
        "    needs:\n    - lint\n    - gate\n",
        "    needs: missing\n",
    ] {
        let v = violations(&build_needing(needs));
        assert_eq!(v.len(), 1, "{needs:?}: {v:?}");
        assert!(v[0].contains("may only need required jobs"), "{v:?}");
    }
    let unreadable = violations(&build_needing("    needs: ${{ fromJSON('[]') }}\n"));
    assert_eq!(unreadable.len(), 1, "{unreadable:?}");
    for needs in [
        "    needs: lint\n",
        "    needs: [ lint ]\n",
        "    needs:\n      - lint\n",
    ] {
        assert_eq!(
            violations(&build_needing(needs)),
            Vec::<String>::new(),
            "{needs:?}"
        );
    }
}

/// Regression (critic j02, j03): a step-level `continue-on-error` passes a
/// failing step, and a step-level `if:` can skip the step that tests.
#[test]
fn required_job_steps_cannot_skip_or_mask_their_work() {
    for (step, reason) in [
        ("        continue-on-error: true\n", "`continue-on-error`"),
        ("        continue-on-error: false\n", "`continue-on-error`"),
        (
            "        if: github.event_name == 'push'\n",
            "`if: github.event_name == 'push'`",
        ),
        ("        if: failure()\n", "`if: failure()`"),
        (
            "        if: ${{ !cancelled() }}\n",
            "`if: ${{ !cancelled() }}`",
        ),
    ] {
        let v = violations(&with(TEST_STEP, &format!("{TEST_STEP}{step}")));
        assert_eq!(v.len(), 1, "{step:?}: {v:?}");
        assert!(v[0].contains(reason), "{step:?}: {v:?}");
    }
    // A condition on the step's marker line and a compact steps list.
    let marker = with(
        TEST_STEP,
        "      - if: cancelled()\n        run: cargo test\n",
    );
    let compact = with(
        "    steps:\n      - name: Clippy\n        run: cargo clippy\n",
        "    steps:\n    - name: Clippy\n      if: failure()\n      run: cargo clippy\n",
    );
    for text in [marker, compact] {
        assert_eq!(violations(&text).len(), 1, "{text}");
    }
    // Conditions under which a step still runs whenever the job can pass.
    for step in [
        "        if: always()\n",
        "        if: ${{ success() }}\n",
        "        if: \"always()\"\n",
    ] {
        let text = with(TEST_STEP, &format!("{TEST_STEP}{step}"));
        assert_eq!(violations(&text), Vec::<String>::new(), "{step:?}");
    }
    // Steps of jobs that are not required checks are not restricted.
    let lint_only_optional = with(
        CLIPPY_STEP,
        "      - name: Clippy\n        if: failure()\n        run: cargo clippy\n",
    );
    assert_eq!(
        required_check_violations(&lint_only_optional, &["Build".to_string()], "main"),
        Vec::<String>::new()
    );
}

#[test]
fn unreadable_steps_of_required_jobs_fail_closed() {
    for steps in [
        "    steps: ${{ fromJSON(inputs.steps) }}\n",
        "    steps:\n      -\n        run: cargo test\n",
    ] {
        let text = with(
            "    steps:\n      - name: Checkout\n",
            &format!("{steps}      - name: Checkout\n"),
        );
        let v = violations(&text);
        assert!(v.iter().any(|m| m.contains("steps")), "{steps:?}: {v:?}");
    }
}

/// Regression (critic j09): two jobs with a required context's name both
/// report under it; one of them may be skipped.
#[test]
fn each_required_context_names_exactly_one_job() {
    let twice = with(
        CLIPPY_STEP,
        &format!("{CLIPPY_STEP}  build2:\n    name: Build\n    runs-on: macos-14\n    timeout-minutes: 5\n    if: ${{{{ false }}}}\n    steps:\n      - run: echo skip\n"),
    );
    let v = violations(&twice);
    assert_eq!(v.len(), 1, "{v:?}");
    assert!(v[0].contains("names 2 jobs"), "{v:?}");
    // The same context in another workflow file.
    assert_eq!(
        reused_context_violations(&compliant(), &contexts()).len(),
        2
    );
    let other = "jobs:\n  docs:\n    name: Docs\n    runs-on: macos-14\n";
    assert_eq!(
        reused_context_violations(other, &contexts()),
        Vec::<String>::new()
    );
}

/// The rule reads the protected branch it is given, not a built-in `main`.
#[test]
fn the_protected_branch_comes_from_the_caller() {
    let release = with("branches: [ main ]", "branches: [ release/v2 ]");
    assert_eq!(
        required_check_violations(&release, &contexts(), "release/v2"),
        Vec::<String>::new()
    );
    assert_eq!(violations(&release).len(), 1);
}

/// Regression (PR #18 critic P0-1, P1-1): libyaml reads a job or step
/// `if : false`, a job `continue-on-error : true`, and an `if:` value that
/// continues on a deeper line, none of which this rule's reader sees. The
/// whole-workflow scan rejects each of those lines.
#[test]
fn hidden_skip_conditions_fail_the_workflow_scan() {
    for (from, to, marker) in [
        (
            "    timeout-minutes: 30\n",
            "    timeout-minutes: 30\n    if : false\n",
            "if : false",
        ),
        (
            "    timeout-minutes: 30\n",
            "    timeout-minutes: 30\n    if\t: false\n",
            "if\t: false",
        ),
        (
            "    timeout-minutes: 30\n",
            "    timeout-minutes: 30\n    continue-on-error : true\n",
            "continue-on-error :",
        ),
        (
            TEST_STEP,
            "      - run: cargo test\n        if : false\n",
            "if : false",
        ),
        (
            TEST_STEP,
            "      - run: cargo test\n        if: success()\n          == false\n",
            "== false",
        ),
    ] {
        let text = with(from, to);
        let line = 1 + text.lines().position(|l| l.contains(marker)).unwrap();
        let v = crate::rules::workflow_violations(&text);
        assert!(
            v.iter()
                .any(|m| m.starts_with(&format!("line {line}: ")) && m.contains("cannot read")),
            "{text}\n{v:?}"
        );
    }
}
