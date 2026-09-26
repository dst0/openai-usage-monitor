# 2026-09-26 — Configuration and registration replayed stale account state

- **Status:** Resolved
- **Task/context:** Review account and credential safety after a failed Desktop account handoff.
- **Unexpected observation or failure:** Changing an unrelated setting or removing an account could restore an older auto-switch setting, token, or account list saved before a concurrent update.
- **Evidence:** Deterministic interleaving tests made a second writer disable auto-switch and update synthetic credentials or add a synthetic account between the first writer's registry read and save. Before the fix, the setting test failed because auto-switch was re-enabled; the removal test lost the concurrently added account.
- **Approaches tried:**
  - **Attempt:** Keep the existing `load_accounts()` then `save_accounts()` sequence, relying on `codex.lock` around each operation.
    - **Outcome:** Did not work.
    - **Why:** The lock serialized individual reads and writes, but released between the read and the later whole-file replacement.
  - **Attempt:** Perform each field change through `update_accounts_atomically()` on the latest registry; initialize a missing registry without replacing one concurrently created, and merge quota fields after network inspection.
    - **Outcome:** Worked in focused tests.
    - **Why:** Each mutation now reads and writes under one registry lock. Quota HTTP work stays outside that lock, and its result cannot replay old tokens or settings.
- **Root cause:** Configuration and registration paths saved stale whole-file snapshots. One registration path also substituted an empty default registry when loading a damaged registry failed.
- **Resolution:** Configuration setters, nickname and multiplier updates, account addition, and account removal now update only their intended fields in a fresh locked registry. `save_current_as` propagates registry load failures. Account addition uses read-only quota inspection and merges only quota fields when the account's tokens still match.
- **Verification:** `cargo test --quiet setup::` passed 42 tests; focused settings and removal regressions were observed failing before the fix and passing afterward. `cargo check --quiet` and `git diff --check` passed. No live account or Desktop switch was exercised by this change.
- **Prevention/follow-up:** Avoid `load_accounts()` plus whole-file `save_accounts()` for read-modify-write operations. Keep network calls outside the registry transaction and check identity before merging their results.
- **Reusable learning:** A lock around the final file write does not make a read-modify-write operation atomic.
- **References:** `codex-switcher/src/setup/account_configuration.test.rs`, `codex-switcher/src/setup/account_registration.test.rs`, `codex-switcher/src/storage/accounts_registry_transaction_service.rs`.
