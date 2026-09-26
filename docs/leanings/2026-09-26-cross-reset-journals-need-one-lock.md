# 2026-09-26 — Cross-reset journals need one operation lock

- **Status:** Resolved
- **Task/context:** Audit manual and automatic weekly reset-credit requests after making manual attempts durable.
- **Unexpected observation or failure:** Each path checked only its own journal. A manual request could use a new key while an automatic attempt was pending, and the reverse path could do the same. A restored quota snapshot also cleared a pending or unknown automatic attempt; merely preserving it still left the non-exhausted branch reporting `ready` and allowing account rotation. A changed local account ID or cached weekly timestamp bypassed the first cross-journal guards. The single automatic journal could also be replaced by another account's attempt.
- **Evidence:** Synthetic tests for manual-after-auto and auto-after-manual failed before the guards; a restored-quota test changed `pending` to `ready` before the cleanup fix. A parseable but incomplete pending journal failed to block a new manual request before validation. Further red tests showed an alias ID escaped an unresolved manual attempt and a changed weekly marker hid an unknown automatic attempt. No live credit request was made.
- **Approaches tried:**
  - **Attempt:** Let each journal enforce only its own path.
    - **Outcome:** Did not work.
    - **Why:** The other path could mint an unrelated idempotency key.
  - **Attempt:** Check the other journal while holding the shared recovery operation lock; preserve uncertain automatic records during restored-quota cleanup.
    - **Outcome:** Worked in synthetic tests.
    - **Why:** Journal preparation, cross-check, and cleanup now serialize across both paths.
- **Root cause:** Independent journals and unlocked cleanup did not preserve a single outstanding reset attempt across the two entry points.
- **Resolution:** Manual reset rejects a pending/unknown automatic attempt for the same account route regardless of cached weekly marker. Any unresolved manual attempt blocks automatic reset across local IDs. Automatic reset refuses to replace any pending/unknown journal, even for another account, until it is reconciled. Malformed or incomplete uncertain journals fail closed, and restored quota keeps pending/unknown state and account-switch suppression.
- **Verification:** Red-to-green cross-path, alias-ID, changed-marker, other-account journal, and cleanup regressions; 11 automatic reset tests and 12 manual reset tests pass. Tests use isolated synthetic homes.
- **Prevention/follow-up:** Keep both entry points and automatic journal cleanup under the same operation lock. Reconcile uncertain attempts with official account evidence or a verified later weekly boundary before removing only the exact private journal; the README records the procedure.
- **Reusable learning:** Separate idempotency journals for one consumable resource must be mutually checked under a shared lock. A local account ID or cached weekly timestamp is not authoritative evidence that an unknown attempt is settled.
- **References:** `codex-switcher/src/auto_reset.test.rs`, `codex-switcher/src/setup/account_reset_service.test.rs`, `codex-switcher/src/auto_reset/weekly_reset_status_service.rs`, `README.md`.
