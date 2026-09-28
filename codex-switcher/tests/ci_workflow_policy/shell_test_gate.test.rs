use super::{
    repository_shell_test_violations, shell_test_gate_violations, shell_tests, SHELL_TEST_CONDITION,
};
use crate::fixtures::{compliant, with};
use crate::required_checks::required_check_violations;
use crate::rules::workflow_violations;
use crate::scratch_git_repo::ScratchGitRepo;

/// The compliant fixture's only step in the required `Build` job.
const BUILD_STEP: &str = "      - run: cargo test --locked\n";
const SCRIPT: &str = "tests/log_permissions_and_uninstall.sh";
const OTHER: &str = "tests/install_bundle_swap.sh";

fn contexts() -> Vec<String> {
    vec!["Build".to_string(), "Lint".to_string()]
}

fn scripts(names: &[&str]) -> Vec<String> {
    names.iter().map(|s| s.to_string()).collect()
}

/// Violations for the single shell test `SCRIPT`.
fn violations(text: &str) -> Vec<String> {
    shell_test_gate_violations(text, &contexts(), &scripts(&[SCRIPT]))
}

fn gate_step_for(script: &str) -> String {
    format!(
        "      - name: Shell Test\n        if: {SHELL_TEST_CONDITION}\n        run: bash {script}\n"
    )
}

fn gate_step() -> String {
    gate_step_for(SCRIPT)
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
const IF_REASON: &str = "`if:` is not set once to exactly `${{ !cancelled() }}`";
/// Advice for a script that no step mentions, which may be a helper.
const HELPER_HINT: &str = "is a helper or fixture rather than a test, move it out of `tests/`";

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

/// Regression: six shell tests, among them the installer's bundle-swap and
/// restart-worker checks, ran only locally while one other shell test had a
/// gate. Each shell test needs a gate of its own.
#[test]
fn each_shell_test_needs_its_own_gate() {
    let both = scripts(&[SCRIPT, OTHER]);
    let one_gate = with_step(&gate_step());
    let v = shell_test_gate_violations(&one_gate, &contexts(), &both);
    assert_eq!(v.len(), 1, "{v:?}");
    assert!(
        v[0].contains(&format!("no required job runs `{OTHER}`")),
        "{v:?}"
    );

    let both_gates = with_step(&format!("{}{}", gate_step(), gate_step_for(OTHER)));
    assert_eq!(
        shell_test_gate_violations(&both_gates, &contexts(), &both),
        Vec::<String>::new()
    );
    // Gates in different required jobs count as well.
    let split = format!("{}{}", with_step(&gate_step()), gate_step_for(OTHER));
    assert_eq!(
        shell_test_gate_violations(&split, &contexts(), &both),
        Vec::<String>::new()
    );

    let none = shell_test_gate_violations(&compliant(), &contexts(), &both);
    assert_eq!(none.len(), 2, "{none:?}");
    assert!(
        none[0].contains(SCRIPT) && none[1].contains(OTHER),
        "{none:?}"
    );
}

#[test]
fn a_gate_for_a_similar_name_does_not_count() {
    for similar in [
        "tests/log_permissions_and_uninstall.sh.bak",
        "tests/old/log_permissions_and_uninstall.sh",
        "./tests/log_permissions_and_uninstall.sh",
        "tests/log_permissions_and_uninstall",
    ] {
        let text = with_step(&gate_step_for(similar));
        assert_eq!(violations(&text).len(), 1, "{similar}");
    }
}

#[test]
fn a_shell_test_name_that_is_not_one_plain_word_is_rejected() {
    for name in [
        "tests/with space.sh",
        "tests/quote'd.sh",
        "tests/hash#.sh",
        "tests/$HOME.sh",
        "tests/glob*.sh",
        "tests/caf\u{e9}.sh",
    ] {
        // Not even a matching gate can run it as written.
        let text = with_step(&gate_step_for(name));
        let v = shell_test_gate_violations(&text, &contexts(), &scripts(&[name]));
        assert_eq!(v.len(), 1, "{name}: {v:?}");
        assert!(
            v[0].contains("cannot run as `bash <script>`"),
            "{name}: {v:?}"
        );
    }
}

#[test]
fn a_plain_name_with_digits_dashes_and_directories_is_accepted() {
    let name = "tests/sub-dir/v2_check-3.sh";
    let text = with_step(&gate_step_for(name));
    assert_eq!(
        shell_test_gate_violations(&text, &contexts(), &scripts(&[name])),
        Vec::<String>::new()
    );
}

#[test]
fn no_shell_tests_need_no_gate() {
    assert_eq!(
        shell_test_gate_violations(&compliant(), &contexts(), &[]),
        Vec::<String>::new()
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
        "        continue-on-error: true\n",
        "        timeout-minutes: 1\n",
    ] {
        let text = with_step(&format!("{}{key}", gate_step()));
        let name = key.trim().split(':').next().unwrap();
        assert_rejected(&text, &format!("sets `{name}`; the step may set only"));
    }
}

/// Regression: gates ran under the default `success()`, so the first failing
/// shell test skipped every later one in its job and a push reported only
/// that first failure. Each gate must run after an earlier failure too.
#[test]
fn a_gate_that_an_earlier_failure_would_skip_is_rejected() {
    let condition = format!("        if: {SHELL_TEST_CONDITION}\n");
    let without = gate_with(&condition, "");
    assert_rejected(&without, IF_REASON);
    for other in [
        "success()",
        "${{ success() }}",
        // Runs on after a cancellation, until the job times out.
        "always()",
        "${{ always() }}",
        // The same test, but only the documented spelling is a gate.
        "'!cancelled()'",
        "cancelled()",
        "failure()",
        "${{ !cancelled() && github.event_name == 'push' }}",
    ] {
        let text = gate_with(&condition, &format!("        if: {other}\n"));
        assert_rejected(&text, IF_REASON);
    }
    let twice = with_step(&format!("{}{condition}", gate_step()));
    assert_rejected(&twice, IF_REASON);
}

/// The condition may sit on the step's marker line or be quoted, and the keys
/// may come in any order.
#[test]
fn the_gate_condition_is_read_wherever_the_step_sets_it() {
    for step in [
        format!("      - if: {SHELL_TEST_CONDITION}\n        run: bash {SCRIPT}\n"),
        format!(
            "      - run: bash {SCRIPT}\n        name: X\n        if: \"{SHELL_TEST_CONDITION}\"\n"
        ),
    ] {
        let text = with_step(&step);
        assert_eq!(violations(&text), Vec::<String>::new(), "{step}");
        assert_eq!(
            required_check_violations(&text, &contexts(), "main"),
            Vec::<String>::new(),
            "{step}"
        );
    }
}

/// A script that no step mentions may be a helper that a test sources. The
/// violation says to move it rather than gate it, which would run a helper
/// on its own as if it were a test. A near miss is a test with a broken gate,
/// so its message leaves the advice out.
#[test]
fn only_a_script_no_step_mentions_is_told_it_may_be_a_misplaced_helper() {
    let absent = violations(&compliant());
    assert_eq!(absent.len(), 1, "{absent:?}");
    assert!(absent[0].contains(HELPER_HINT), "{absent:?}");
    assert!(
        absent[0].contains("keep its `.sh` suffix so the locked-cargo scan still reads it"),
        "{absent:?}"
    );
    let near_miss = violations(&gate_with(
        &format!("run: bash {SCRIPT}"),
        &format!("run: sh {SCRIPT}"),
    ));
    assert_eq!(near_miss.len(), 1, "{near_miss:?}");
    assert!(near_miss[0].contains(RUN_REASON), "{near_miss:?}");
    assert!(!near_miss[0].contains(HELPER_HINT), "{near_miss:?}");
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
        format!("bash {OTHER}"),
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

/// A new shell test is required as soon as git tracks it, in `tests/` or a
/// subdirectory; scripts elsewhere, other suffixes, and untracked files are
/// not shell tests.
#[test]
fn shell_tests_are_the_tracked_sh_files_under_tests() {
    let scratch = ScratchGitRepo::init();
    scratch
        .write("tests/a.sh", "")
        .write("tests/nested/b.sh", "")
        .write("tests/c.bash", "")
        .write("tests/d.sh.orig", "")
        .write("tests/E.SH", "")
        .write("tests/dir.sh/f.txt", "")
        .write("tests/fixtures/g.json", "")
        .write("scripts/h.sh", "")
        .write("i.sh", "")
        .commit_all(false)
        .write("tests/untracked.sh", "")
        .write("tests/staged.sh", "")
        .git(&["add", "tests/staged.sh"]);
    assert_eq!(
        shell_tests(&scratch.repo()),
        Ok(scripts(&[
            "tests/a.sh",
            "tests/nested/b.sh",
            "tests/staged.sh"
        ]))
    );
}

#[test]
fn shell_test_discovery_fails_closed_outside_a_repository() {
    let nowhere = crate::git_repo::GitRepo::at("/nonexistent/ci-policy-repository");
    assert!(shell_tests(&nowhere).is_err());
}

/// The live rule gates exactly what git lists: every tracked shell test with a
/// gate complies, and dropping any one gate is reported for that script.
#[test]
fn repository_rule_gates_each_listed_shell_test() {
    let scratch = ScratchGitRepo::init();
    scratch
        .write("tests/a.sh", "")
        .write("tests/nested/b.sh", "")
        .commit_all(false);
    let both = with_step(&format!(
        "{}{}",
        gate_step_for("tests/a.sh"),
        gate_step_for("tests/nested/b.sh")
    ));
    assert_eq!(
        repository_shell_test_violations(&scratch.repo(), &both, &contexts()),
        Vec::<String>::new()
    );
    let one = with_step(&gate_step_for("tests/a.sh"));
    let v = repository_shell_test_violations(&scratch.repo(), &one, &contexts());
    assert_eq!(v.len(), 1, "{v:?}");
    assert!(
        v[0].contains("no required job runs `tests/nested/b.sh`"),
        "{v:?}"
    );
}

/// A listing that fails or finds nothing would require no gate at all.
#[test]
fn repository_rule_fails_closed_on_an_empty_or_failed_listing() {
    let empty = ScratchGitRepo::init();
    empty.write("scripts/a.sh", "").commit_all(false);
    let v = repository_shell_test_violations(&empty.repo(), &compliant(), &contexts());
    assert_eq!(v.len(), 1, "{v:?}");
    assert!(v[0].contains("git lists no shell tests"), "{v:?}");

    let nowhere = crate::git_repo::GitRepo::at("/nonexistent/ci-policy-repository");
    let v = repository_shell_test_violations(&nowhere, &compliant(), &contexts());
    assert_eq!(v.len(), 1, "{v:?}");
    assert!(v[0].contains("cannot list the shell tests"), "{v:?}");
}

/// Wiring on the live repository: the uninstall test, whose gate this rule
/// was written for, is listed, and removing the gate of any listed shell test
/// from the live `ci.yml` is reported for exactly that script.
#[test]
fn live_workflow_loses_a_gate_when_its_step_is_removed() {
    let ci = crate::read(".github/workflows/ci.yml");
    let live_contexts =
        crate::rules::required_check_contexts(&crate::read("scripts/setup-github-protection.sh"));
    let repo = crate::repo();
    let listed = shell_tests(&repo).expect("list shell tests");
    assert!(listed.iter().any(|t| t == SCRIPT), "{listed:?}");
    assert_eq!(
        repository_shell_test_violations(&repo, &ci, &live_contexts),
        Vec::<String>::new()
    );
    for script in &listed {
        let run = format!("run: bash {script}\n");
        assert_eq!(ci.matches(&run).count(), 1, "{script}");
        let without = ci.replacen(&run, "run: echo skipped\n", 1);
        let v = repository_shell_test_violations(&repo, &without, &live_contexts);
        assert_eq!(v.len(), 1, "{script}: {v:?}");
        assert!(
            v[0].contains(&format!("no required job runs `{script}`")),
            "{v:?}"
        );
        // Each live gate sets its condition on the line before `run:`, and
        // dropping it is reported for that script alone.
        let guarded = format!("if: {SHELL_TEST_CONDITION}\n        {run}");
        assert_eq!(ci.matches(&guarded).count(), 1, "{script}");
        let unguarded = ci.replacen(&guarded, &run, 1);
        let v = repository_shell_test_violations(&repo, &unguarded, &live_contexts);
        assert_eq!(v.len(), 1, "{script}: {v:?}");
        assert!(
            v[0].contains(&format!("no required job runs `{script}`")) && v[0].contains(IF_REASON),
            "{v:?}"
        );
    }
}
