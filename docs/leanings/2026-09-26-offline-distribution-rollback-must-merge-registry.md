# 2026-09-26 — Offline distribution rollback must preserve concurrent registry edits

- **Status:** Resolved
- **Task/context:** Reviewing offline APP/CLI distribution when saving the Desktop session marker fails.
- **Unexpected observation or failure:** The rollback wrote a saved whole-file `accounts.json` snapshot, erasing settings and account edits made by another Monitor operation after that snapshot.
- **Evidence:** `marker_failure_rollback_preserves_concurrent_registry_changes` failed before the fix because a concurrent `auto_switch_enabled=false` change was reset. The forward path also used a whole-file snapshot save.
- **Approaches tried:**
  - **Attempt:** Restore the complete pre-request registry after rolling back authentication.
    - **Outcome:** Did not work.
    - **Why:** The snapshot was stale by the time the marker save failed.
  - **Attempt:** Commit or restore only `active_account_id` inside a fresh locked registry transaction, verifying target identity and credentials.
    - **Outcome:** Worked for the reproduced race.
    - **Why:** Concurrent settings and unrelated account fields survive, while a changed target or active selection blocks the commit.
- **Root cause:** The account registry was treated as an operation-local snapshot instead of shared mutable state.
- **Resolution:** Offline distribution now uses a named registry service for field-only forward commit and rollback. Uncertain failures retain the distribution journal.
- **Verification:** `cargo test --quiet --bin codex-mon distribution::distribution_offline -- --test-threads=1` passed three focused tests, including settings preservation and target reauthentication refusal.
- **Prevention/follow-up:** Keep cross-file auth/registry failures journaled for reconciliation; add a synthetic interleaving test whenever another registry field is changed by distribution.
- **Reusable learning:** Roll back only the field owned by a transaction and verify its expected value under the fresh registry lock.
- **References:** `codex-switcher/src/distribution/distribution_offline_registry_service.rs`; `codex-switcher/src/distribution/distribution_offline_commit_service.test.rs`.
