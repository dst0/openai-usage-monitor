//! Read-only git queries for rules about what the repository commits, which
//! the working tree alone cannot show: a lockfile can exist locally while
//! `.gitignore` keeps it out of every clean checkout. Git runs without user
//! or system configuration and without inherited `GIT_*` variables, so a
//! developer's global excludes, diff drivers, or a hook's `GIT_DIR` cannot
//! change an answer or redirect a command to another repository.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// A git checkout rooted at `root`.
pub struct GitRepo {
    root: PathBuf,
}

/// `git -C root` with configuration and environment isolated.
pub fn git_command(root: &Path) -> Command {
    isolated_git_command(root, std::env::vars_os().map(|(key, _)| key))
}

/// `git -C root` without the `GIT_*` variables among `inherited`, such as the
/// `GIT_DIR` and `GIT_INDEX_FILE` a hook exports, and without user or system
/// configuration.
fn isolated_git_command(root: &Path, inherited: impl IntoIterator<Item = OsString>) -> Command {
    let mut command = Command::new("git");
    for key in inherited {
        if key.to_string_lossy().starts_with("GIT_") {
            command.env_remove(&key);
        }
    }
    command
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .arg("-C")
        .arg(root);
    command
}

impl GitRepo {
    pub fn at(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn run(&self, args: &[&str]) -> Result<Output, String> {
        git_command(&self.root)
            .args(args)
            .output()
            .map_err(|e| format!("cannot run git: {e}"))
    }

    /// Standard output of a git command that must succeed.
    fn stdout(&self, args: &[&str]) -> Result<String, String> {
        let output = self.run(args)?;
        if !output.status.success() {
            return Err(format!(
                "`git {}` failed: {}",
                args.join(" "),
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
        String::from_utf8(output.stdout).map_err(|e| format!("git output is not UTF-8: {e}"))
    }

    /// Whether `HEAD` holds `path` as a regular file.
    pub fn committed_file(&self, path: &str) -> Result<bool, String> {
        let listing = self.stdout(&["ls-tree", "-z", "HEAD", "--", path])?;
        Ok(listing.split('\0').any(|entry| {
            entry
                .split_once('\t')
                .is_some_and(|(meta, name)| name == path && meta.split(' ').nth(1) == Some("blob"))
        }))
    }

    /// Whether a versioned `.gitignore` matches `path`, tracked or not.
    /// Per-clone `.git/info/exclude` and global excludes do not count: they
    /// cannot keep a file out of another clone.
    pub fn ignored_by_gitignore(&self, path: &str) -> Result<bool, String> {
        let listing = self.stdout(&[
            "ls-files",
            "-z",
            "--cached",
            "--others",
            "--ignored",
            "--exclude-per-directory=.gitignore",
            "--",
            path,
        ])?;
        Ok(!listing.is_empty())
    }

    /// Tracked paths matching `pattern`, relative to the repository root.
    pub fn tracked_files(&self, pattern: &str) -> Result<Vec<String>, String> {
        let listing = self.stdout(&["ls-files", "-z", "--", pattern])?;
        Ok(listing
            .split('\0')
            .filter(|p| !p.is_empty())
            .map(str::to_string)
            .collect())
    }

    /// Whether the working-tree `path` has the content committed at `HEAD`.
    /// Only meaningful for a committed path: an untracked file never differs.
    pub fn matches_head(&self, path: &str) -> Result<bool, String> {
        let args = [
            "diff",
            "--quiet",
            "--no-ext-diff",
            "--no-textconv",
            "HEAD",
            "--",
            path,
        ];
        let output = self.run(&args)?;
        match output.status.code() {
            Some(0) => Ok(true),
            Some(1) => Ok(false),
            _ => Err(format!(
                "`git {}` failed: {}",
                args.join(" "),
                String::from_utf8_lossy(&output.stderr).trim()
            )),
        }
    }
}

#[cfg(test)]
#[path = "git_repo.test.rs"]
mod tests;
