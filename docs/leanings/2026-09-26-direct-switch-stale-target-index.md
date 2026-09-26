# 2026-09-26 — Direct switch target index became stale during registry sync

- **Status:** Resolved
- **Task/context:** Review direct `cxi switch` while hardening shared Desktop authentication.
- **Unexpected observation or failure:** A saved target could be replaced by another account at the same vector index before credential selection.
- **Evidence:** A deterministic synthetic test selected the first account, replaced the registry with the same two accounts in reverse order during the sync callback, and observed the old code choose the other account. A removed target or duplicate ID also needed a fail-closed outcome. No live account was changed.
- **Approaches tried:**
  - **Attempt:** Retain the pre-sync vector index.
    - **Outcome:** Did not work.
    - **Why:** `ActiveAuthRegistrySyncService::sync_from_disk` reloads the registry under a lock and replaces the caller's account vector.
  - **Attempt:** Retain the canonical target ID, then resolve it uniquely after sync.
    - **Outcome:** Worked in focused tests.
    - **Why:** The selection survives vector reordering and refuses removal or duplicate identity.
- **Root cause:** `switch_to_account` resolved an index before `sync_from_disk` and dereferenced that index after the vector could be replaced.
- **Resolution:** Resolve the user query once, retain its canonical ID, synchronize active authentication, and select credentials only from one exact refreshed ID match.
- **Verification:** The reorder regression failed with the other account selected before the fix and passed after it. Focused tests cover missing and duplicate IDs; two additional isolated `CODEX_HOME` tests use the real atomic registry update and active-auth synchronization to verify fresh credentials after reorder and unchanged auth after removal. No live switch or installed-app proof was attempted for this narrow change.
- **Prevention/follow-up:** Treat vector positions as snapshot-local whenever a locked update may replace a registry. Keep direct-switch target resolution inside the same tested helper.
- **Reusable learning:** Carry stable account identity, never an index, across a registry refresh boundary.
- **References:** `codex-switcher/src/switcher/account_switch_service.rs`, `codex-switcher/src/switcher/account_target_resolver.rs`, `codex-switcher/src/switcher/account_target_resolver.test.rs`, `CODEX.md`.
