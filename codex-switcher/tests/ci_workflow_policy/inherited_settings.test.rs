use super::inherited_setting_violations;
use crate::fixtures::{compliant, with};

const BUILD_TIMEOUT: &str = "    timeout-minutes: 30\n";
const CONCURRENCY: &str = "concurrency:\n";

fn contexts() -> Vec<String> {
    vec!["Build".to_string(), "Lint".to_string()]
}

fn violations(text: &str) -> Vec<String> {
    inherited_setting_violations(text, &contexts())
}

fn assert_rejected(text: &str, reason: &str) {
    let v = violations(text);
    assert_eq!(v.len(), 1, "{text}\n{v:?}");
    assert!(v[0].contains(reason), "{reason}: {v:?}");
}

#[test]
fn allowed_variables_and_defaults_without_a_shell_comply() {
    assert_eq!(violations(&compliant()), Vec::<String>::new());
    let job_env = with(
        BUILD_TIMEOUT,
        "    timeout-minutes: 30\n    env:\n      CARGO_INCREMENTAL: 0\n      CARGO_TERM_COLOR: always # logs\n",
    );
    assert_eq!(violations(&job_env), Vec::<String>::new());
    let working_directory = with(
        CONCURRENCY,
        "defaults:\n  run:\n    working-directory: codex-switcher\nconcurrency:\n",
    );
    assert_eq!(violations(&working_directory), Vec::<String>::new());
}

/// Regression (PR #18 critic P1-3): a variable exported by the workflow or a
/// required job reaches the Clippy gate even though the gate step sets no
/// `env:` itself.
#[test]
fn inherited_variables_outside_the_allowlist_are_rejected() {
    for variable in [
        "RUSTFLAGS: -Aclippy::all",
        "CARGO_ENCODED_RUSTFLAGS: --cap-lints=allow",
        "CARGO_BUILD_RUSTFLAGS: --cap-lints=allow",
        "CARGO_TARGET_AARCH64_APPLE_DARWIN_RUSTFLAGS: -Awarnings",
        "RUSTC_WRAPPER: /usr/bin/true",
        "CLIPPY_CONF_DIR: /tmp",
    ] {
        let job = with(
            BUILD_TIMEOUT,
            &format!("    timeout-minutes: 30\n    env:\n      {variable}\n"),
        );
        assert_rejected(&job, "required job `build` exports");
        let workflow = with(CONCURRENCY, &format!("env:\n  {variable}\nconcurrency:\n"));
        assert_rejected(&workflow, "the workflow exports");
    }
    for env in [
        "    env: { RUSTFLAGS: -Awarnings }\n",
        "    env: ${{ fromJSON(inputs.env) }}\n",
    ] {
        let text = with(BUILD_TIMEOUT, &format!("{BUILD_TIMEOUT}{env}"));
        assert_rejected(&text, "block mapping");
    }
    let listed = with(
        BUILD_TIMEOUT,
        "    timeout-minutes: 30\n    env:\n      - RUSTFLAGS\n",
    );
    assert_rejected(&listed, "cannot read");
}

/// Regression (PR #18 critic P1-2): `defaults.run.shell` decides how every
/// `run:` of the workflow or job executes, so `true {0}` passes the gate
/// without running Clippy.
#[test]
fn a_default_shell_is_rejected() {
    let workflow = with(
        CONCURRENCY,
        "defaults:\n  run:\n    shell: true {0}\nconcurrency:\n",
    );
    assert_rejected(&workflow, "`defaults` of the workflow sets `shell`");
    let job = with(
        BUILD_TIMEOUT,
        "    timeout-minutes: 30\n    defaults:\n      run:\n        shell: sh -c : {0}\n",
    );
    assert_rejected(&job, "`defaults` of required job `build` sets `shell`");
    let flow = with(
        CONCURRENCY,
        "defaults: { run: { shell: bash } }\nconcurrency:\n",
    );
    assert_rejected(&flow, "block mapping");
}

/// Regression (PR #18 critic P1-3): appending to `$GITHUB_ENV` or
/// `$GITHUB_PATH` sets the environment or `cargo` of every later step.
#[test]
fn runner_environment_files_are_rejected() {
    for run in [
        "      - run: echo RUSTFLAGS=-Awarnings >> \"$GITHUB_ENV\"\n",
        "      - run: echo /tmp/fake-cargo >> $GITHUB_PATH\n",
        "      - run: |\n          echo X=1 >> ${GITHUB_ENV}\n",
    ] {
        let text = with(
            "      - run: cargo test\n",
            &format!("{run}      - run: cargo test\n"),
        );
        assert_rejected(&text, "changes the environment of later required steps");
    }
}

/// Only the required jobs' steps are protected; another job's settings do
/// not reach them.
#[test]
fn settings_of_jobs_that_are_not_required_are_not_restricted() {
    let text = with(
        "    timeout-minutes: 5\n",
        "    timeout-minutes: 5\n    env:\n      RUSTFLAGS: -Awarnings\n    defaults:\n      run:\n        shell: true {0}\n",
    );
    assert_eq!(violations(&text).len(), 2);
    assert_eq!(
        inherited_setting_violations(&text, &["Build".to_string()]),
        Vec::<String>::new()
    );
}
