//! Enforces the AGENTS.md CI baseline on the live repository: least-privilege
//! permissions, actions pinned to full commit SHAs, explicit job timeouts and
//! concurrency, an exact Rust toolchain, and required branch-protection checks
//! that always report. Rule logic lives in `ci_workflow_policy/rules.rs`, with
//! the checkout and trigger rules in `checkout.rs` and `triggers.rs`; negative
//! cases live in `fixtures.rs` and in each rule's `.test.rs` file.

use std::fs;
use std::path::PathBuf;

#[path = "ci_workflow_policy/yaml_lines.rs"]
mod yaml_lines;

#[path = "ci_workflow_policy/rules.rs"]
mod rules;

#[path = "ci_workflow_policy/checkout.rs"]
mod checkout;

#[path = "ci_workflow_policy/triggers.rs"]
mod triggers;

#[path = "ci_workflow_policy/fixtures.rs"]
mod fixtures;

fn repo_file(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join(relative)
}

fn read(relative: &str) -> String {
    let path = repo_file(relative);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

#[test]
fn every_workflow_meets_ci_baseline() {
    let dir = repo_file(".github/workflows");
    let contexts = rules::required_check_contexts(&read("scripts/setup-github-protection.sh"));
    let mut checked = 0;
    let mut violations = Vec::new();
    for entry in fs::read_dir(&dir).expect("read workflows dir") {
        let path = entry.expect("dir entry").path();
        if !matches!(
            path.extension().and_then(|e| e.to_str()),
            Some("yml" | "yaml")
        ) {
            continue;
        }
        checked += 1;
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let text = fs::read_to_string(&path).expect("read workflow");
        let mut found = rules::workflow_violations(&text);
        if name == "ci.yml" {
            found.extend(rules::required_check_violations(&text, &contexts));
        }
        violations.extend(found.into_iter().map(|v| format!("{name}: {v}")));
    }
    assert!(checked > 0, "no workflows found in {}", dir.display());
    assert!(
        violations.is_empty(),
        "CI baseline violations:\n{}",
        violations.join("\n")
    );
}

#[test]
fn branch_protection_lists_required_contexts() {
    let contexts = rules::required_check_contexts(&read("scripts/setup-github-protection.sh"));
    assert!(
        !contexts.is_empty() && contexts.iter().all(|c| !c.is_empty()),
        "{contexts:?}"
    );
}

#[test]
fn rust_toolchain_is_pinned_with_lint_components() {
    let violations = rules::toolchain_file_violations(&read("codex-switcher/rust-toolchain.toml"));
    assert!(violations.is_empty(), "{violations:?}");
}

/// Format-independent proof that CI compiled this test with the pinned
/// toolchain: rustup exports `RUSTUP_TOOLCHAIN` to the processes it launches.
/// It catches bypasses the line scan cannot see, such as running cargo with
/// `--manifest-path` from outside `codex-switcher/`. Local runs may
/// deliberately use another toolchain (for example while bumping it).
#[test]
fn ci_runs_on_pinned_toolchain() {
    if std::env::var("GITHUB_ACTIONS").as_deref() != Ok("true") {
        return;
    }
    let toolchain_file = read("codex-switcher/rust-toolchain.toml");
    let channel = rules::toolchain_channel(&toolchain_file);
    let active = std::env::var("RUSTUP_TOOLCHAIN").unwrap_or_default();
    assert!(
        active.starts_with(&format!("{channel}-")),
        "CI ran with RUSTUP_TOOLCHAIN=`{active}`, expected the pinned `{channel}`"
    );
    eprintln!("CI toolchain verified: {active}");
}
