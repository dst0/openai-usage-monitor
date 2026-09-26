//! Throwaway git repositories for the tests of git-backed rules. Each one
//! lives in its own temporary directory, runs git with the same isolation as
//! `GitRepo`, and is removed on drop.

use crate::git_repo::{git_command, GitRepo};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT_ID: AtomicUsize = AtomicUsize::new(0);

pub struct ScratchGitRepo {
    root: PathBuf,
}

impl ScratchGitRepo {
    pub fn init() -> Self {
        let root = std::env::temp_dir().join(format!(
            "ci-policy-git-{}-{}",
            std::process::id(),
            NEXT_ID.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("create scratch repository");
        let repo = Self { root };
        repo.git(&["init", "-q"]);
        repo
    }

    pub fn repo(&self) -> GitRepo {
        GitRepo::at(&self.root)
    }

    /// Absolute path of `path` inside the repository.
    pub fn path(&self, path: &str) -> PathBuf {
        self.root.join(path)
    }

    pub fn write(&self, path: &str, contents: &str) -> &Self {
        let file = self.root.join(path);
        fs::create_dir_all(file.parent().expect("file has a parent")).expect("create parent");
        fs::write(&file, contents).expect("write scratch file");
        self
    }

    pub fn remove(&self, path: &str) -> &Self {
        fs::remove_file(self.root.join(path)).expect("remove scratch file");
        self
    }

    /// Runs git in the repository and asserts that it succeeded.
    pub fn git(&self, args: &[&str]) -> &Self {
        let output = git_command(&self.root)
            .args(args)
            .output()
            .expect("run git");
        assert!(
            output.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        self
    }

    /// Stages every change, forced past ignore rules when `force`, and
    /// commits it.
    pub fn commit_all(&self, force: bool) -> &Self {
        self.git(if force {
            &["add", "-A", "-f"]
        } else {
            &["add", "-A"]
        });
        self.git(&[
            "-c",
            "user.name=CI policy test",
            "-c",
            "user.email=ci-policy@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            "fixture",
        ])
    }
}

impl Drop for ScratchGitRepo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
