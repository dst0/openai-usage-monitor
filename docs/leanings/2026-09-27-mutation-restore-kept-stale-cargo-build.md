# 2026-09-27 — Restoring a mutated file from a backup kept a stale cargo build

- **Status:** Resolved
- **Task/context:** Hand-applied mutation checks of the new `ci_workflow_policy` rules while committing `codex-switcher/Cargo.lock` ([2026-09-27-ignored-cargo-lock-left-builds-unpinned.md](2026-09-27-ignored-cargo-lock-left-builds-unpinned.md)). A script copied each source file to a backup, wrote the mutation, ran `cargo test --locked --test ci_workflow_policy`, and moved the backup back.
- **Unexpected observation or failure:** After the last mutation was restored, the unchanged suite failed: `committed_file_requires_a_regular_file_at_head` reported `Ok(true)` for a path the restored code rejects. It looked like a real bug in `git_repo.rs`.
- **Evidence:** The restored file contained the original code (`grep` found the removed condition). Running `git ls-tree` by hand gave exactly the output the restored code handles correctly. `touch` on the restored sources made the next run rebuild and pass all 82 tests.
- **Approaches tried:**
  - **Attempt:** Debug the reported `ls-tree` behavior.
    - **Outcome:** Did not work
    - **Why:** The code under test was not the code on disk.
  - **Attempt:** Set the restored file's modification time to now after moving the backup back.
    - **Outcome:** Worked
    - **Why:** cargo decides freshness by comparing source modification times with the previous build's dep-info. The backup kept the time it was copied, which was older than the mutated build, so cargo reused the mutated test binary. Earlier runs in the batch were unaffected because each wrote a newer mutation.
- **Root cause:** Restoring content with an older modification time does not invalidate cargo's fingerprint.
- **Resolution:** The mutation script now calls `os.utime` on each restored file; every mutation was rerun and each still failed at least one test.
- **Verification:** A clean `cargo test --locked --test ci_workflow_policy` after the restore passed 82 tests; rerunning the mutation that had exposed the stale build failed its test as expected.
- **Prevention/follow-up:** None in the repository; the mutation script was a temporary tool.
- **Reusable learning:** When a mutation or experiment restores source files, give them a fresh modification time (or `touch` them) before the next cargo build, or the build may silently reuse artifacts compiled from the mutated code.
- **References:** `codex-switcher/tests/ci_workflow_policy/git_repo.rs`, `git_repo.test.rs`.
