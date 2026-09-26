use super::GitRepo;
use crate::scratch_git_repo::ScratchGitRepo;

#[test]
fn committed_file_requires_a_regular_file_at_head() {
    let scratch = ScratchGitRepo::init();
    scratch
        .write("crate/Cargo.lock", "committed\n")
        .write("crate/Cargo.lock.d/x", "in a directory\n")
        .commit_all(false)
        .write("crate/untracked.lock", "untracked\n")
        .write("staged.lock", "staged\n")
        .git(&["add", "staged.lock"]);
    let repo = scratch.repo();
    assert_eq!(repo.committed_file("crate/Cargo.lock"), Ok(true));
    // Staged is not committed, a directory is not a file (`crate/` lists the
    // files inside it), and a prefix of a committed path is not that path.
    for path in [
        "crate/untracked.lock",
        "staged.lock",
        "crate/Cargo.lock.d",
        "crate/Cargo.lock.d/",
        "crate",
        "crate/",
        "crate/Cargo",
        "missing.lock",
    ] {
        assert_eq!(repo.committed_file(path), Ok(false), "{path}");
    }
}

#[test]
fn committed_file_fails_closed_without_a_commit_or_a_repository() {
    let empty = ScratchGitRepo::init();
    assert!(empty.repo().committed_file("Cargo.lock").is_err());
    let nowhere = GitRepo::at("/nonexistent/ci-policy-repository");
    assert!(nowhere.committed_file("Cargo.lock").is_err());
    assert!(nowhere.ignored_by_gitignore("Cargo.lock").is_err());
    assert!(nowhere.tracked_files("*.sh").is_err());
    assert!(nowhere.matches_head("Cargo.lock").is_err());
}

#[test]
fn only_versioned_gitignore_rules_count_as_ignored() {
    let scratch = ScratchGitRepo::init();
    scratch
        .write("crate/Cargo.lock", "lock\n")
        .commit_all(false);
    let repo = scratch.repo();
    assert_eq!(repo.ignored_by_gitignore("crate/Cargo.lock"), Ok(false));
    // Per-clone excludes cannot keep the file out of another clone.
    scratch.write(".git/info/exclude", "*.lock\n");
    assert_eq!(repo.ignored_by_gitignore("crate/Cargo.lock"), Ok(false));
    // A rule in the root or in the file's own directory does, tracked or not.
    scratch.write(".gitignore", "*.lock\n");
    assert_eq!(repo.ignored_by_gitignore("crate/Cargo.lock"), Ok(true));
    scratch.git(&["rm", "-q", "--cached", "crate/Cargo.lock"]);
    assert_eq!(repo.ignored_by_gitignore("crate/Cargo.lock"), Ok(true));
    scratch
        .write(".gitignore", "")
        .write("crate/.gitignore", "Cargo.lock\n");
    assert_eq!(repo.ignored_by_gitignore("crate/Cargo.lock"), Ok(true));
    // A later negation re-includes the file.
    scratch.write("crate/.gitignore", "*.lock\n!Cargo.lock\n");
    assert_eq!(repo.ignored_by_gitignore("crate/Cargo.lock"), Ok(false));
}

#[test]
fn tracked_files_match_nested_paths() {
    let scratch = ScratchGitRepo::init();
    scratch
        .write("install.sh", "")
        .write("scripts/build.sh", "")
        .write("scripts/notes.md", "")
        .commit_all(false)
        .write("scripts/untracked.sh", "");
    assert_eq!(
        scratch.repo().tracked_files("*.sh"),
        Ok(vec![
            "install.sh".to_string(),
            "scripts/build.sh".to_string()
        ])
    );
}

#[test]
fn matches_head_compares_the_working_tree_with_the_commit() {
    let scratch = ScratchGitRepo::init();
    scratch.write("Cargo.lock", "resolved\n").commit_all(false);
    let repo = scratch.repo();
    assert_eq!(repo.matches_head("Cargo.lock"), Ok(true));
    scratch.write("Cargo.lock", "re-resolved\n");
    assert_eq!(repo.matches_head("Cargo.lock"), Ok(false));
    // Staging the rewrite does not make it the committed content.
    scratch.git(&["add", "Cargo.lock"]);
    assert_eq!(repo.matches_head("Cargo.lock"), Ok(false));
    scratch.write("Cargo.lock", "resolved\n");
    assert_eq!(repo.matches_head("Cargo.lock"), Ok(true));
    scratch.remove("Cargo.lock");
    assert_eq!(repo.matches_head("Cargo.lock"), Ok(false));
}

/// A hook exports `GIT_DIR` and `GIT_INDEX_FILE`; inheriting them would point
/// queries, and the scratch repositories' `git add`, at the caller's
/// repository and index instead of the one given.
#[test]
fn git_commands_drop_inherited_git_variables() {
    let inherited = [
        "GIT_DIR",
        "GIT_INDEX_FILE",
        "GIT_WORK_TREE",
        "HOME",
        "XGIT_DIR",
    ]
    .map(std::ffi::OsString::from);
    let command = super::isolated_git_command(std::path::Path::new("/tmp"), inherited);
    let mut envs: Vec<(String, Option<String>)> = command
        .get_envs()
        .map(|(k, v)| {
            let text = |s: &std::ffi::OsStr| s.to_string_lossy().into_owned();
            (text(k), v.map(text))
        })
        .collect();
    envs.sort();
    let owned = |k: &str, v: Option<&str>| (k.to_string(), v.map(str::to_string));
    assert_eq!(
        envs,
        [
            owned("GIT_CONFIG_GLOBAL", Some("/dev/null")),
            owned("GIT_CONFIG_NOSYSTEM", Some("1")),
            owned("GIT_DIR", None),
            owned("GIT_INDEX_FILE", None),
            owned("GIT_WORK_TREE", None),
        ]
    );
    assert_eq!(command.get_args().collect::<Vec<_>>(), ["-C", "/tmp"]);
}
