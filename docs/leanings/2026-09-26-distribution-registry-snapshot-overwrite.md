# 2026-09-26 — Distribution registry snapshot overwrite

- **Status:** Resolved
- **Task/context:** Review account registry writes during ChatGPT Desktop shutdown token handoff and after relaunch.
- **Unexpected observation or failure:** A concurrent account setting change could be silently lost while the switcher reported a successful handoff.
- **Evidence:** A deterministic test changed `auto_switch_enabled` and another account's priority after the distribution path loaded its registry snapshot. The old post-relaunch commit then saved the stale snapshot and restored the previous setting. The same load-then-save pattern existed in shutdown token handoff. A second test showed that changing an unknown top-level auth field was ignored by the previous readback comparison.
- **Approaches tried:**
  - **Attempt:** Keep separate registry load and save calls.
    - **Outcome:** Did not work
    - **Why:** Each call locked only its own operation, leaving a gap where another writer could update the registry.
  - **Attempt:** Reconcile fresh auth into the registry inside `update_accounts_atomically` and compare complete `AuthJson` values.
    - **Outcome:** Worked
    - **Why:** The registry read, merge, and save now share one Monitor lock; full document comparison detects unknown-field changes before success or rollback.
- **Root cause:** The handoff and commit paths wrote a registry snapshot captured before concurrent changes; the auth comparator ignored extension fields.
- **Resolution:** Both registry paths now reconcile freshly read live auth inside the atomic registry transaction. Offline and Desktop readback and rollback compare the complete auth document.
- **Verification:** Both stale-snapshot and unknown-field regressions failed against the previous behavior. Focused distribution tests passed after the fix. No live Desktop restart was performed.
- **Prevention/follow-up:** Keep concurrent registry mutation and unknown-field readback tests in the distribution suite; install and live recovery verification remain separate release gates.
- **Reusable learning:** A lock around each file operation does not make a read-modify-write sequence atomic; reconcile under one lock and verify the complete credential document.
- **References:** `codex-switcher/src/distribution/distribution_desktop_auth_handoff_service.test.rs`, `codex-switcher/src/distribution/distribution_account_commit_service.test.rs`.
