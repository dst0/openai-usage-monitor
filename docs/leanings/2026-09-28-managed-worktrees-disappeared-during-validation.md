# 2026-09-28 — Managed worktrees disappeared during validation

- **Status:** Partial
- **Task/context:** PR #45 validation and parallel PR #46 work in managed checkouts.
- **Unexpected observation or failure:** Two checkout directories disappeared while local Rust builds were running. One test process completed 813 unit tests, then could not execute its integration-test binary because its checkout was gone; uncommitted work in the other checkout was unavailable.
- **Evidence:** `git worktree list --porcelain` still listed both paths as prunable, while the directories no longer existed. The remote PR #45 branch remained at its pushed head. The cause of directory removal is not established.
- **Approaches tried:**
  - **Attempt:** Continue the build in the original checkout.
    - **Outcome:** Did not work.
    - **Why:** The path and generated test executable no longer existed.
  - **Attempt:** Reuse a clean, idle managed checkout from the same chat and branch from the exact remote PR head.
    - **Outcome:** Worked for PR #45.
    - **Why:** The committed source and remote branch were intact; the small uncommitted test edit was reconstructed and checked.
- **Root cause:** Unknown. The checkout removal was external to the observed Git, test, and tool commands.
- **Resolution:** Reconstruct the missing edit in an existing free worktree and push it to the original PR branch. Treat hosted CI as the complete gate for the new exact head.
- **Verification:** `cargo fmt --all --check`, `git diff --check`, and a focused test passed before the path vanished; the remote branch accepted the reconstructed commit. Full exact-head CI remains pending.
- **Prevention/follow-up:** Commit small verified changes promptly; if a checkout vanishes, verify remote state and recover separately without pruning shared metadata. Investigate the external remover before relying on long local builds.
- **Reusable learning:** A vanished test executable is an infrastructure interruption, not a code-test failure or a pass.
- **References:** PR #45, PR #46, `git worktree list --porcelain`.
