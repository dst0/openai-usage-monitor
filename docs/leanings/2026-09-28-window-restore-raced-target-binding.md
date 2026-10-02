# 2026-09-28 — Window restore raced target binding

- **Status:** Resolved
- **Task/context:** Adversarial review of target account checks before Desktop recovery IPC in PR #45.
- **Unexpected observation or failure:** Moving target verification before geometry restoration left no check after restoration, so an external Desktop credential writer could change the binding during that work.
- **Evidence:** The reviewed distribution and direct-switch paths verified before restore and then proceeded to recovery without a second binding check.
- **Approaches tried:**
  - **Attempt:** Check the target once before touching window geometry.
    - **Outcome:** Did not work.
    - **Why:** Desktop is an uncooperative writer and geometry work takes time.
- **Root cause:** The restore refactor changed the ordering of a security-sensitive check.
- **Resolution:** Verify target auth and exact Desktop session before and after restoration, aborting before IPC if the second check fails.
- **Verification:** Mock distribution and direct-switch helper tests change the target during restore and assert recovery does not start; full CI and installed behavior remain to be checked.
- **Prevention/follow-up:** Preserve both sides of the restore boundary in future refactors and keep the checkpoint on identity failure.
- **Reusable learning:** Recheck mutable security state immediately before dispatch after any intervening UI operation.
- **References:** `codex-switcher/src/distribution/distribution_recovery_audit_service.rs`, `codex-switcher/src/switcher/desktop_session_binding_service.rs`, PR #45.
