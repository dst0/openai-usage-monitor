use super::committed_lockfile_violations;
use crate::git_repo::GitRepo;
use crate::scratch_git_repo::ScratchGitRepo;

const LOCKFILE: &str = "codex-switcher/Cargo.lock";

fn violations(scratch: &ScratchGitRepo) -> Vec<String> {
    committed_lockfile_violations(&scratch.repo(), LOCKFILE)
}

#[test]
fn a_committed_unignored_lockfile_complies() {
    let scratch = ScratchGitRepo::init();
    scratch
        .write(".gitignore", "target/\n")
        .write(LOCKFILE, "version = 4\n")
        .commit_all(false);
    assert_eq!(violations(&scratch), Vec::<String>::new());
    // Re-including a Cargo lockfile after a broader rule also complies.
    scratch
        .write(".gitignore", "*.lock\n!Cargo.lock\n")
        .commit_all(false);
    assert_eq!(violations(&scratch), Vec::<String>::new());
}

/// Regression: `.gitignore` held `*.lock`, so `codex-switcher/Cargo.lock`
/// existed only where a developer or CI had generated it and every clean
/// checkout re-resolved dependencies.
#[test]
fn an_ignored_uncommitted_lockfile_is_rejected() {
    let scratch = ScratchGitRepo::init();
    scratch
        .write(".gitignore", "*.lock\n")
        .write("codex-switcher/Cargo.toml", "[package]\n")
        .commit_all(false)
        .write(LOCKFILE, "version = 4\n");
    let v = violations(&scratch);
    assert_eq!(v.len(), 2, "{v:?}");
    assert!(v[0].contains("is not committed"), "{v:?}");
    assert!(v[1].contains("matches a .gitignore rule"), "{v:?}");
}

#[test]
fn committed_and_ignored_are_separate_requirements() {
    // Generated locally but never committed.
    let untracked = ScratchGitRepo::init();
    untracked
        .write("README.md", "")
        .commit_all(false)
        .write(LOCKFILE, "version = 4\n");
    let v = violations(&untracked);
    assert_eq!(v.len(), 1, "{v:?}");
    assert!(v[0].contains("is not committed"), "{v:?}");
    // Staged is not committed either.
    untracked.git(&["add", LOCKFILE]);
    assert_eq!(violations(&untracked), v);

    // Force-added past a rule that still matches it.
    let forced = ScratchGitRepo::init();
    forced
        .write("codex-switcher/.gitignore", "Cargo.lock\n")
        .write(LOCKFILE, "version = 4\n")
        .commit_all(true);
    let v = violations(&forced);
    assert_eq!(v.len(), 1, "{v:?}");
    assert!(v[0].contains("matches a .gitignore rule"), "{v:?}");
}

#[test]
fn a_lockfile_removed_from_head_is_rejected() {
    let scratch = ScratchGitRepo::init();
    scratch.write(LOCKFILE, "version = 4\n").commit_all(false);
    scratch.git(&["rm", "-q", LOCKFILE]).commit_all(false);
    let v = violations(&scratch);
    assert_eq!(v.len(), 2, "{v:?}");
    assert!(v[0].contains("is not committed"), "{v:?}");
    assert!(v[1].contains("neither tracked nor present"), "{v:?}");
}

#[test]
fn unanswerable_queries_fail_closed() {
    let v = committed_lockfile_violations(&GitRepo::at("/nonexistent/ci-policy"), LOCKFILE);
    assert_eq!(v.len(), 2, "{v:?}");
    assert!(v.iter().all(|m| m.starts_with("cannot verify")), "{v:?}");
}
