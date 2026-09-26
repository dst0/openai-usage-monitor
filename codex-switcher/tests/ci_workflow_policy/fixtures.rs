//! Positive and negative fixtures proving each policy rule rejects the
//! regression it guards against, independent of the live workflow file. The
//! compliant workflow and `with` helper are shared with the per-rule test
//! files next to `checkout.rs` and `triggers.rs`.

use crate::required_checks::required_check_violations;
use crate::rules::*;
use crate::workflow_jobs::{jobs, Job};
use crate::yaml_lines::{entry, Entry};

pub const SHA: &str = "11d5960a326750d5838078e36cf38b85af677262";

const COMPLIANT: &str = r#"name: CI
on:
  pull_request:
    branches: [ main ]
permissions:
  contents: read
concurrency:
  group: ci-${{ github.ref }}
  cancel-in-progress: true
jobs:
  build:
    name: Build
    runs-on: macos-14
    timeout-minutes: 30
    steps:
      - name: Checkout
        uses: actions/checkout@SHA # v4.4.0
        with:
          persist-credentials: false
      - run: cargo test --locked
  lint:
    name: Lint
    runs-on: macos-14
    timeout-minutes: 5
    steps:
      - name: Clippy
        run: cargo clippy --locked
"#;

pub fn compliant() -> String {
    COMPLIANT.replace("@SHA", &format!("@{SHA}"))
}

/// The compliant workflow with the first `from` replaced by `to`.
pub fn with(from: &str, to: &str) -> String {
    let base = compliant();
    assert!(base.contains(from), "fixture lacks {from:?}");
    base.replacen(from, to, 1)
}

fn contexts() -> Vec<String> {
    vec!["Build".to_string(), "Lint".to_string()]
}

const CHECKOUT: &str =
    "        uses: actions/checkout@11d5960a326750d5838078e36cf38b85af677262 # v4.4.0\n";

#[test]
fn compliant_workflow_has_no_violations() {
    assert_eq!(workflow_violations(&compliant()), Vec::<String>::new());
    assert_eq!(
        required_check_violations(&compliant(), &contexts(), "main"),
        Vec::<String>::new()
    );
}

#[test]
fn entry_strips_list_marker_quotes_and_comment() {
    assert_eq!(
        entry("  -   \"uses\": 'a/b@c' # v1.2.3"),
        Some(Entry {
            key: "uses",
            value: "a/b@c",
            comment: "v1.2.3",
            list_item: true
        })
    );
    assert_eq!(entry("echo \"Error: nope\""), None);
    assert_eq!(
        entry("group: ${{ github.workflow }}-${{ github.ref }}")
            .unwrap()
            .value,
        "${{ github.workflow }}-${{ github.ref }}"
    );
}

#[test]
fn unpinned_references_are_rejected() {
    for bad in [
        "actions/checkout@v4",
        "dtolnay/rust-toolchain@stable",
        "actions/checkout@11d5960",
        "actions/checkout",
        &format!("actions/checkout@{}", SHA.to_uppercase()),
    ] {
        let v = action_pin_violations(&with(&format!("actions/checkout@{SHA} # v4.4.0"), bad));
        assert_eq!(v.len(), 1, "{bad}: {v:?}");
        assert!(v[0].contains("not pinned"), "{bad}: {v:?}");
    }
}

#[test]
fn uses_spellings_the_parser_skips_fail_closed() {
    for bad in [
        "        -   uses: actions/checkout@v4\n",
        "        \"uses\": actions/checkout@v4\n",
        "        uses : actions/checkout@v4\n",
        "      - { name: x, uses: actions/checkout@v4 }\n",
    ] {
        let v = action_pin_violations(&with(CHECKOUT, bad));
        assert_eq!(v.len(), 1, "{bad:?}: {v:?}");
    }
}

#[test]
fn local_actions_are_rejected_until_scanned() {
    let v = action_pin_violations(&with(
        "      - run: cargo test --locked\n",
        "      - uses: ./.github/actions/setup-rust\n",
    ));
    assert!(v[0].contains("outside the policy scan"), "{v:?}");
}

#[test]
fn release_comment_must_be_exact_version() {
    for comment in ["", " # latest", " # v4", " # v4.4", " # release v4.4.0"] {
        let text = with(&format!("{SHA} # v4.4.0"), &format!("{SHA}{comment}"));
        assert!(
            action_pin_violations(&text)[0].contains("release comment"),
            "{comment:?}"
        );
    }
}

#[test]
fn docker_actions_require_a_sha256_digest() {
    let digest = "a".repeat(64);
    let ok = with(
        "      - run: cargo test --locked\n",
        &format!("      - uses: docker://alpine@sha256:{digest}\n"),
    );
    assert_eq!(action_pin_violations(&ok), Vec::<String>::new());
    let bad = with(
        "      - run: cargo test --locked\n",
        "      - uses: docker://alpine:3.20\n",
    );
    assert_eq!(action_pin_violations(&bad).len(), 1);
}

#[test]
fn missing_or_broad_permissions_are_rejected() {
    let missing = with("permissions:\n  contents: read\n", "");
    assert_eq!(
        permission_violations(&missing),
        vec!["missing top-level `permissions: contents: read`"]
    );
    assert_eq!(
        permission_violations(&with(
            "permissions:\n  contents: read\n",
            "permissions: write-all\n"
        ))
        .len(),
        2
    );
    assert!(permission_violations(&with(
        "permissions:\n  contents: read\n",
        "permissions: read-all\n"
    ))[0]
        .contains("explicitly"));
    assert!(
        permission_violations(&with("  contents: read\n", "  contents: write\n"))
            .iter()
            .any(|v| v.contains("contents: write"))
    );
}

#[test]
fn quoted_and_commented_permissions_are_understood() {
    for ok in ["  contents: 'read'\n", "  contents: read # checkout only\n"] {
        assert_eq!(
            permission_violations(&with("  contents: read\n", ok)),
            Vec::<String>::new(),
            "{ok:?}"
        );
    }
}

#[test]
fn job_level_write_permission_is_rejected() {
    let text = with(
        "    timeout-minutes: 5\n",
        "    timeout-minutes: 5\n    permissions:\n      pull-requests: write\n",
    );
    assert_eq!(
        permission_violations(&text),
        vec!["permission `pull-requests: write` exceeds read-only"]
    );
    assert_eq!(
        permission_violations(&text.replace("pull-requests: write", "id-token: none")),
        Vec::<String>::new()
    );
}

#[test]
fn every_job_needs_a_bounded_timeout() {
    assert_eq!(
        timeout_violations(&with("    timeout-minutes: 5\n", "")),
        vec!["job `lint` has no timeout-minutes"]
    );
    for bad in ["0", "61", "${{ inputs.t }}"] {
        assert_eq!(
            timeout_violations(&with(
                "timeout-minutes: 5",
                &format!("timeout-minutes: {bad}")
            ))
            .len(),
            1,
            "{bad}"
        );
    }
    for ok in ["\"5\"", "5 # minutes"] {
        assert_eq!(
            timeout_violations(&with(
                "timeout-minutes: 5",
                &format!("timeout-minutes: {ok}")
            )),
            Vec::<String>::new(),
            "{ok}"
        );
    }
    assert_eq!(
        timeout_violations("name: x\n"),
        vec!["workflow declares no jobs"]
    );
}

#[test]
fn step_properties_do_not_count_as_job_properties() {
    // Steps written without extra list indentation sit at the job-property indent.
    let text = with(
        "    timeout-minutes: 5\n    steps:\n      - name: Clippy\n        run: cargo clippy --locked\n",
        "    steps:\n    - name: Check\n      timeout-minutes: 5\n      run: cargo clippy --locked\n",
    );
    assert_eq!(
        timeout_violations(&text),
        vec!["job `lint` has no timeout-minutes"]
    );
    assert_eq!(jobs(&text)[1].prop("name"), Some("Lint"));
}

#[test]
fn four_space_indentation_and_reusable_jobs_parse() {
    let four = compliant()
        .lines()
        .map(|l| " ".repeat(l.len() - l.trim_start().len()) + l)
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(timeout_violations(&four), Vec::<String>::new());
    assert_eq!(
        jobs(&four)
            .iter()
            .map(|j| j.id.as_str())
            .collect::<Vec<_>>(),
        ["build", "lint"]
    );
    let reusable = with("    runs-on: macos-14\n    timeout-minutes: 5\n    steps:\n      - name: Clippy\n        run: cargo clippy --locked\n", &format!("    uses: org/repo/.github/workflows/lint.yml@{SHA} # v1.0.0\n"));
    assert_eq!(timeout_violations(&reusable), Vec::<String>::new());
}

#[test]
fn jobs_parse_ids_and_direct_properties() {
    let parsed = jobs(&compliant());
    assert_eq!(
        parsed
            .iter()
            .map(|j| (j.id.as_str(), j.prop("name"), j.prop("timeout-minutes")))
            .collect::<Vec<_>>(),
        [
            ("build", Some("Build"), Some("30")),
            ("lint", Some("Lint"), Some("5"))
        ]
    );
    assert!(parsed.iter().all(|j: &Job| j.prop("steps") == Some("")));
    assert_eq!(
        jobs("jobs: # comment\n  a:\n    timeout-minutes: 1\n").len(),
        1
    );
}

#[test]
fn concurrency_requires_group_and_cancel() {
    assert_eq!(
        concurrency_violations(&with(
            "concurrency:\n  group: ci-${{ github.ref }}\n  cancel-in-progress: true\n",
            ""
        )),
        vec!["missing top-level `concurrency:`"]
    );
    assert_eq!(
        concurrency_violations(&with(
            "cancel-in-progress: true",
            "cancel-in-progress: false"
        ))
        .len(),
        1
    );
    assert_eq!(
        concurrency_violations(&with("  group: ci-${{ github.ref }}\n", "")).len(),
        1
    );
    let pr_only = with(
        "cancel-in-progress: true",
        "cancel-in-progress: ${{ github.event_name == 'pull_request' }}",
    );
    assert_eq!(concurrency_violations(&pr_only), Vec::<String>::new());
}

#[test]
fn floating_rust_toolchains_are_rejected() {
    for bad in [
        "        with:\n          toolchain: stable\n",
        "        with:\n          toolchain: \"stable\" # latest\n",
        "      - run: rustup toolchain install stable-aarch64-apple-darwin\n",
        "      - run: rustup default nightly\n",
        "      - run: rustup override set 1.90.0\n",
        "      - run: rustup run 1.90.0 cargo test\n",
        "      - run: cargo +beta test\n",
        "    env:\n      RUSTUP_TOOLCHAIN: 1.90.0\n",
    ] {
        let text = with("      - run: cargo test --locked\n", bad);
        assert_eq!(floating_toolchain_violations(&text).len(), 1, "{bad:?}");
    }
    let comment = with(
        "      - run: cargo test --locked\n",
        "      # toolchain: stable was replaced\n      - run: cargo test # not stable\n",
    );
    assert_eq!(
        floating_toolchain_violations(&comment),
        Vec::<String>::new()
    );
}

#[test]
fn masked_failures_are_rejected() {
    for bad in [
        "cargo test || true",
        "cargo test || :",
        "set +e; cargo test",
    ] {
        assert_eq!(
            masked_failure_violations(&with("run: cargo test --locked", &format!("run: {bad}")))
                .len(),
            1,
            "{bad}"
        );
    }
}

#[test]
fn required_checks_must_always_report() {
    let cases = [
        (
            with("    name: Build\n", "    name: Build (renamed)\n"),
            "has no job",
        ),
        (
            with(
                "    timeout-minutes: 30\n",
                "    timeout-minutes: 30\n    if: github.event_name == 'push'\n",
            ),
            "`if`",
        ),
        (
            with(
                "    timeout-minutes: 30\n",
                "    timeout-minutes: 30\n    strategy:\n      matrix:\n        os: [macos-14]\n",
            ),
            "`strategy`",
        ),
        (
            with(
                "    timeout-minutes: 30\n",
                "    timeout-minutes: 30\n    continue-on-error: true\n",
            ),
            "`continue-on-error`",
        ),
        (
            with(
                "    branches: [ main ]\n",
                "    branches: [ main ]\n    paths: ['Sources/**']\n",
            ),
            "`paths`",
        ),
        (
            with(
                "    branches: [ main ]\n",
                "    paths-ignore: ['docs/**']\n",
            ),
            "`paths-ignore`",
        ),
        (
            with(
                "on:\n  pull_request:\n    branches: [ main ]\n",
                "on: push\n",
            ),
            "unconditional `pull_request`",
        ),
    ];
    for (text, expected) in cases {
        let v = required_check_violations(&text, &contexts(), "main");
        assert_eq!(v.len(), 1, "{expected}: {v:?}");
        assert!(v[0].contains(expected), "{expected}: {v:?}");
    }
}

#[test]
fn toolchain_file_must_be_exact_release_with_lint_components() {
    let ok = "[toolchain]\nchannel = \"1.98.1\"\ncomponents = [\"clippy\", \"rustfmt\"]\n";
    assert_eq!(toolchain_file_violations(ok), Vec::<String>::new());
    assert_eq!(toolchain_channel(ok), "1.98.1");
    for channel in ["stable", "1.98", "1.98.x", "nightly-2026-09-01", ""] {
        assert_eq!(
            toolchain_file_violations(&ok.replace("1.98.1", channel)).len(),
            1,
            "{channel}"
        );
    }
    assert_eq!(
        toolchain_file_violations(&ok.replace("\"clippy\", ", "")).len(),
        1
    );
    assert_eq!(
        toolchain_file_violations(&ok.replace(", \"rustfmt\"", "")).len(),
        1
    );
    assert_eq!(toolchain_file_violations("[toolchain]\n").len(), 3);
}

#[test]
fn protected_branch_is_parsed_from_the_protection_api_path() {
    let put = |branch: &str| {
        format!("gh api -X PUT \"repos/${{REPO}}/branches/{branch}/protection\" \\\n")
    };
    assert_eq!(protected_branch(&put("main")), Some("main"));
    assert_eq!(protected_branch(&put("release/v2")), Some("release/v2"));
    for script in [
        String::new(),
        put("${BRANCH}"),
        put(""),
        format!("{}{}", put("main"), put("release")),
        "gh api repos/o/r/branches/main\n".to_string(),
    ] {
        assert_eq!(protected_branch(&script), None, "{script:?}");
    }
}

#[test]
fn required_contexts_are_parsed_from_protection_payload() {
    let script = "  \"required_status_checks\": {\n    \"contexts\": [\n      \"A b\",\n      \"C\"\n    ]\n  },\n";
    assert_eq!(required_check_contexts(script), vec!["A b", "C"]);
}
