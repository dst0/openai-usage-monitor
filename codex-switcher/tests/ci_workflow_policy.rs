//! Enforces the AGENTS.md CI baseline on the live repository: least-privilege
//! permissions, actions pinned to full commit SHAs, explicit job timeouts and
//! concurrency, an exact Rust toolchain, a committed `Cargo.lock` that every
//! workflow and repository script builds with `--locked`, cache keys that hash
//! committed files, and required branch-protection checks that always report.
//! Rule logic lives in `ci_workflow_policy/rules.rs`, with the checkout,
//! trigger, required-job, locked-cargo, lockfile, and cache-key rules in
//! `checkout.rs`, `triggers.rs`, `required_checks.rs`, `locked_cargo.rs`,
//! `lockfile.rs`, and `cache_keys.rs`, and `yaml_limits.rs` rejecting YAML the
//! line reader (`yaml_lines.rs`, `yaml_values.rs`, `workflow_jobs.rs`) cannot
//! read. `shell_lines.rs` reads shell command lines for the locked-cargo rule,
//! and `git_repo.rs` answers what the repository commits; negative cases live
//! in `fixtures.rs` and in each module's `.test.rs` file.

use std::fs;
use std::path::PathBuf;

use git_repo::GitRepo;

#[path = "ci_workflow_policy/yaml_lines.rs"]
mod yaml_lines;

#[path = "ci_workflow_policy/yaml_limits.rs"]
mod yaml_limits;

#[path = "ci_workflow_policy/yaml_values.rs"]
mod yaml_values;

#[path = "ci_workflow_policy/workflow_jobs.rs"]
mod workflow_jobs;

#[path = "ci_workflow_policy/rules.rs"]
mod rules;

#[path = "ci_workflow_policy/required_checks.rs"]
mod required_checks;

#[path = "ci_workflow_policy/checkout.rs"]
mod checkout;

#[path = "ci_workflow_policy/triggers.rs"]
mod triggers;

#[path = "ci_workflow_policy/shell_lines.rs"]
mod shell_lines;

#[path = "ci_workflow_policy/locked_cargo.rs"]
mod locked_cargo;

#[path = "ci_workflow_policy/git_repo.rs"]
mod git_repo;

#[path = "ci_workflow_policy/lockfile.rs"]
mod lockfile;

#[path = "ci_workflow_policy/cache_keys.rs"]
mod cache_keys;

#[path = "ci_workflow_policy/scratch_git_repo.rs"]
mod scratch_git_repo;

#[path = "ci_workflow_policy/fixtures.rs"]
mod fixtures;

/// The workflow whose jobs report the branch-protection contexts.
const REQUIRED_CHECK_WORKFLOW: &str = "ci.yml";
/// The lockfile of the `codex-mon` binary crate, relative to the repository.
const LOCKFILE: &str = "codex-switcher/Cargo.lock";

fn repo_file(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join(relative)
}

fn read(relative: &str) -> String {
    let path = repo_file(relative);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

fn repo() -> GitRepo {
    GitRepo::at(repo_file(""))
}

/// `(file name, text)` of every workflow in `.github/workflows`.
fn workflows() -> Vec<(String, String)> {
    let dir = repo_file(".github/workflows");
    let mut out = Vec::new();
    for entry in fs::read_dir(&dir).expect("read workflows dir") {
        let path = entry.expect("dir entry").path();
        if matches!(
            path.extension().and_then(|e| e.to_str()),
            Some("yml" | "yaml")
        ) {
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            out.push((name, fs::read_to_string(&path).expect("read workflow")));
        }
    }
    assert!(!out.is_empty(), "no workflows found in {}", dir.display());
    out
}

#[test]
fn every_workflow_meets_ci_baseline() {
    let script = read("scripts/setup-github-protection.sh");
    let contexts = rules::required_check_contexts(&script);
    let branch = rules::protected_branch(&script).expect("protection script names one branch");
    let mut required_workflow_checked = false;
    let mut violations = Vec::new();
    for (name, text) in workflows() {
        let mut found = rules::workflow_violations(&text);
        if name == REQUIRED_CHECK_WORKFLOW {
            required_workflow_checked = true;
            found.extend(required_checks::required_check_violations(
                &text, &contexts, branch,
            ));
        } else {
            found.extend(required_checks::reused_context_violations(&text, &contexts));
        }
        violations.extend(found.into_iter().map(|v| format!("{name}: {v}")));
    }
    assert!(
        required_workflow_checked,
        "{REQUIRED_CHECK_WORKFLOW} (the workflow reporting the required checks) not found"
    );
    assert!(
        violations.is_empty(),
        "CI baseline violations:\n{}",
        violations.join("\n")
    );
}

#[test]
fn branch_protection_lists_required_contexts() {
    let script = read("scripts/setup-github-protection.sh");
    let contexts = rules::required_check_contexts(&script);
    assert!(
        !contexts.is_empty() && contexts.iter().all(|c| !c.is_empty()),
        "{contexts:?}"
    );
    assert_eq!(rules::protected_branch(&script), Some("main"));
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

/// The `codex-mon` binary builds from a reviewed dependency graph: its
/// lockfile is committed and no `.gitignore` rule can drop it again.
#[test]
fn cargo_lockfile_is_committed() {
    let violations = lockfile::committed_lockfile_violations(&repo(), LOCKFILE);
    assert!(violations.is_empty(), "{violations:?}");
}

/// The negative rules pass on a workflow or installer that never builds;
/// the builds that matter must also exist with `--locked`.
#[test]
fn installer_and_required_rust_job_build_locked() {
    assert!(
        locked_cargo::runs_locked(&read("scripts/install.sh"), "build"),
        "scripts/install.sh must run `cargo build --locked`"
    );
    let ci = read(&format!(".github/workflows/{REQUIRED_CHECK_WORKFLOW}"));
    let contexts = rules::required_check_contexts(&read("scripts/setup-github-protection.sh"));
    let required_jobs_testing_locked: Vec<String> = workflow_jobs::jobs(&ci)
        .into_iter()
        .filter(|job| {
            job.prop("name")
                .is_some_and(|n| contexts.iter().any(|c| c == n))
        })
        .filter(|job| {
            workflow_jobs::job_text(&ci, &job.id)
                .is_some_and(|t| locked_cargo::runs_locked(&t, "test"))
        })
        .map(|job| job.id)
        .collect();
    assert!(
        !required_jobs_testing_locked.is_empty(),
        "no required-check job in {REQUIRED_CHECK_WORKFLOW} runs `cargo test --locked`"
    );
}

/// Workflows are covered by `workflow_violations`; the installer and the
/// shell tests build `codex-mon` too, and must fail on a missing or stale
/// lockfile instead of shipping a freshly resolved dependency graph.
#[test]
fn repository_scripts_run_cargo_locked() {
    let scripts = repo().tracked_files("*.sh").expect("list tracked scripts");
    assert!(
        scripts.iter().any(|s| s == "scripts/install.sh"),
        "scripts/install.sh is not among the tracked scripts: {scripts:?}"
    );
    let violations: Vec<String> = scripts
        .iter()
        .flat_map(|script| {
            locked_cargo::unlocked_cargo_violations(&read(script))
                .into_iter()
                .map(move |v| format!("{script}: {v}"))
        })
        .collect();
    assert!(
        violations.is_empty(),
        "unlocked cargo invocations:\n{}",
        violations.join("\n")
    );
}

#[test]
fn workflow_cache_keys_hash_committed_files() {
    let repo = repo();
    let violations: Vec<String> = workflows()
        .into_iter()
        .flat_map(|(name, text)| {
            cache_keys::cache_key_input_violations(&text, &repo)
                .into_iter()
                .map(move |v| format!("{name}: {v}"))
        })
        .collect();
    assert!(violations.is_empty(), "{violations:?}");
}

/// Format-independent proof that CI built and tested the committed lockfile:
/// any step that re-resolved dependencies without `--locked`, however it
/// invoked cargo, leaves `Cargo.lock` different from `HEAD`. Local runs may
/// carry an uncommitted dependency change.
#[test]
fn ci_tests_ran_against_committed_lockfile() {
    if std::env::var("GITHUB_ACTIONS").as_deref() != Ok("true") {
        return;
    }
    assert_eq!(
        repo().matches_head(LOCKFILE),
        Ok(true),
        "{LOCKFILE} changed during the CI run; build with --locked"
    );
    eprintln!("CI lockfile verified: {LOCKFILE} matches HEAD");
}
