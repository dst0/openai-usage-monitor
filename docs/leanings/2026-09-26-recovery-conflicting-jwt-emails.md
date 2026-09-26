# 2026-09-26 — Recovery rejected conflicting token emails

- **Status:** Resolved
- **Task/context:** Bind a deferred recovery target to the locally configured account during a Desktop account handoff.
- **Unexpected observation or failure:** The ID-token email could match a saved account while the access-token email named a different account. Recovery selected the first email and returned a binding.
- **Evidence:** `conflicting_token_emails_do_not_bind_a_recovery_account` returned a saved account ID before the change; it now returns no binding.
- **Approaches tried:**
  - **Attempt:** Use the general metadata helper that prefers the ID-token email.
    - **Outcome:** Did not work.
    - **Why:** Preference hides an explicit conflict between claims.
  - **Attempt:** Require consistent decoded email claims before matching a saved account.
    - **Outcome:** Worked.
    - **Why:** Ambiguous local identity fails closed.
- **Root cause:** Recovery used a presentation-oriented metadata helper for an account-binding decision.
- **Resolution:** Recovery now uses `consistent_jwt_email`, checks ChatGPT auth mode, and requires a non-default workspace ID.
- **Verification:** The regression passed after the change; the 96-test `recovery::` suite passed.
- **Prevention/follow-up:** The helper checks decoded claims, not JWT signatures. Keep the original checkpoint if identity cannot be established.
- **Reusable learning:** Do not use a preferred display email as proof of account binding when credential claims conflict.
- **References:** `codex-switcher/src/recovery/active_auth_binding_service.rs`, `codex-switcher/src/recovery/manifest_store.test.rs`.
