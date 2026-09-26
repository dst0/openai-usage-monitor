//! `Cargo.lock` is a reviewed supply-chain input: it must be committed, and
//! no versioned `.gitignore` rule may match it. A lockfile that exists only in
//! a developer's working tree makes every clean checkout re-resolve the
//! dependency graph, and `--locked` then fails instead of building.

use crate::git_repo::GitRepo;

/// Why `lockfile` is not a committed, unignored file, if it is not.
pub fn committed_lockfile_violations(repo: &GitRepo, lockfile: &str) -> Vec<String> {
    let mut out = Vec::new();
    match repo.committed_file(lockfile) {
        Ok(true) => {}
        Ok(false) => out.push(format!(
            "`{lockfile}` is not committed; commit the resolved lockfile so every checkout \
             builds the reviewed dependency graph"
        )),
        Err(e) => out.push(format!("cannot verify that `{lockfile}` is committed: {e}")),
    }
    match repo.ignored_by_gitignore(lockfile) {
        Ok(false) => {}
        Ok(true) => out.push(format!(
            "`{lockfile}` matches a .gitignore rule; remove the rule so `git add` cannot \
             skip the lockfile once it is untracked"
        )),
        Err(e) => out.push(format!(
            "cannot verify that `{lockfile}` is not ignored: {e}"
        )),
    }
    out
}

#[cfg(test)]
#[path = "lockfile.test.rs"]
mod tests;
