# 2026-09-26 — Persist manual reset attempts before sending credits

- **Status:** Resolved
- **Task/context:** Review manual `cxi reset-account` after adding a fresh-registry credit merge.
- **Unexpected observation or failure:** A second invocation after an unknown service response or an applied reset with local cache conflict generated a new idempotency key and could request another credit.
- **Evidence:** Two synthetic repeat-invocation tests failed before the journal and passed after it blocked the second callback; no live reset was sent.
- **Approaches tried:**
  - **Attempt:** Report uncertainty only in the first command's error.
    - **Outcome:** Did not work.
    - **Why:** A later process had no durable knowledge of that request.
  - **Attempt:** Save a private pending attempt before dispatch and retain unresolved outcomes.
    - **Outcome:** Worked.
    - **Why:** Every new invocation loads the attempt before selecting or consuming a credit.
- **Root cause:** Manual reset requests had ephemeral idempotency state despite an externally consumable action.
- **Resolution:** Added a 0600, no-follow, atomically replaced and directory-synced manual reset journal. Unknown, crash, and applied cache conflict retain a blocking record; known non-consumption and committed application resolve it.
- **Verification:** Eight reset-flow tests and two journal-validation tests pass, including red-to-green repeat calls, simulated crash persistence, mode and payload rejection, symlink refusal, and a later explicit request after resolution. The synthetic uninstall test removes only the Monitor-owned journal.
- **Prevention/follow-up:** The README gives an exact-path operator reconciliation procedure after official same-account quota evidence; an unchanged cache alone cannot settle an unknown request.
- **Reusable learning:** Persist an idempotency key before a consumable request, and never create a new key while the prior outcome is unresolved.
- **References:** `codex-switcher/src/setup/manual_reset_attempt.rs`, `codex-switcher/src/setup/manual_reset_attempt_store.rs`, `codex-switcher/src/setup/account_reset_service.test.rs`, `README.md`.
