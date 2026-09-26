# 2026-09-26 — Re-login registry snapshot race

- **Status:** Resolved
- **Task/context:** Commit browser re-login and keep daemon quota polling from replacing a newer account registry.
- **Unexpected observation or failure:** A re-login saved a registry snapshot captured before a concurrent settings or token change. The daemon could likewise replay an older quota-poll snapshot after re-login and restore a stale credential copy. Registry auto-heal and first-run auto-import also wrote after a stale read.
- **Evidence:** `active_relogin_registry_commit_preserves_concurrent_other_account_and_settings`, `inactive_relogin_registry_commit_preserves_concurrent_other_account_and_settings`, `quota_save_cannot_overwrite_relogin_between_read_and_commit`, `registry_autoheal_cannot_replay_stale_tokens_after_a_concurrent_commit`, and `registry_autoimport_cannot_replace_a_concurrent_registry_creation` failed before fresh locked transactions. Tests used isolated credential fixtures.
- **Approaches tried:**
  - **Attempt:** Lock only the final whole-registry save.
    - **Outcome:** Did not work
    - **Why:** The stale snapshot was read before the lock and still replaced the newer file.
  - **Attempt:** Read, merge the target fields, and write under one Monitor registry lock.
    - **Outcome:** Worked
    - **Why:** The merge starts from the latest on-disk registry and preserves unrelated changes.
- **Root cause:** The final save was atomic at the file level but was not an atomic read/modify/write transaction.
- **Resolution:** Re-login, daemon quota persistence, active Desktop token sync, duplicate auto-heal, and first-run auto-import now use fresh locked registry transactions. Active re-login also rechecks live auth under that lock. The Desktop does not honor the Monitor lock, so the active auth path separately compares pathname, inode, and full content immediately before replacement; a residual non-cooperating Desktop race remains.
- **Verification:** The named stale-snapshot regressions passed after these fixes. The full `cargo test --quiet --bin codex-mon` suite passed 350 tests. No live credential files were modified in the isolated tests.
- **Prevention/follow-up:** Keep deterministic interleaving regressions for active and inactive re-login, daemon quota save, Desktop token sync, auto-heal, and auto-import. Treat registry mutations as transactions rather than saving earlier snapshots.
- **Reusable learning:** An atomic rename does not protect against a stale read/modify/write; hold one lock across the fresh read, merge, and write.
- **References:** `codex-switcher/src/setup/relogin_registry_commit_service.rs`, `codex-switcher/src/distribution/daemon_tick_service.rs`, `codex-switcher/src/distribution/daemon_account_sync_service.rs`, `codex-switcher/src/storage/accounts_registry_transaction_service.rs`.
