# 2026-09-26 — Relaunched Desktop changed auth before recovery

- **Status:** Resolved
- **Task/context:** Integrate process-bound Desktop account display with shared-auth switching and owner-routed task recovery in distribution and direct `cxi switch`.
- **Unexpected observation or failure:** A relaunched Desktop could replace the planned target's shared `auth.json` with the previous account before recovery, yet either switch path could still call the recovery path.
- **Evidence:** A synthetic lifecycle test changed auth during `launch_app`. Before the fix, the final distribution outcome was unverified but `recovery_calls` was 1. After adding the dispatch gate, a second red assertion showed that the target's process-bound APP marker still remained. The code had checked the latest auth and registry only after the recovery service returned.
- **Approaches tried:**
  - **Attempt:** Rely on the final registry commit's account check.
    - **Outcome:** Did not work
    - **Why:** Recovery had already been requested before that check.
  - **Attempt:** Recheck the target account, unchanged auth document, exact process, and session marker after window restore and immediately before recovery.
    - **Outcome:** Worked
    - **Why:** The same synthetic auth replacement now stops the recovery call while preserving a failed distribution outcome for inspection.
- **Root cause:** Relaunch binding recorded the planned account before verifying the credentials that the new Desktop actually retained, and recovery preceded the final auth commit check.
- **Resolution:** `DistributionDesktopAuthHandoffService::verify_after_launch` reconciles the current auth against the saved target and checks a second read. `DistributionRecoveryAuditService` runs that gate together with process and marker readback after window restore, before invoking Desktop recovery. Direct `cxi switch` performs the same live-auth and exact-process/marker check before its recovery call. A failed gate removes only the exact marker written by this relaunch, so UI cannot keep claiming the planned account as live.
- **Verification:** `distribution::identity_tests::changed_desktop_auth_after_relaunch_blocks_recovery` first failed with `recovery_calls=1`, then passed with `recovery_calls=0` and exact marker invalidation. `switcher::desktop_session_binding_service::account_identity_tests::direct_relaunch_restoring_previous_auth_blocks_recovery_dispatch` similarly failed before the direct-switch gate and passed with no recovery call and no false marker. The integrated baseline passed 530 Rust unit tests and all Swift suites; a later token-alias guard still requires the final Rust rerun. A live cross-account cold-task recovery was unavailable for this run, so runtime behavior remains unverified.
- **Prevention/follow-up:** Keep the pre-dispatch check adjacent to the actual recovery call. A final registry check cannot retroactively prevent a request sent under the wrong credentials. Desktop does not honor the Monitor lock, so an external auth write after the final check remains a fail-closed uncertainty to watch in live traces.
- **Reusable learning:** Verify the *relaunched* Desktop's live auth and process identity immediately before owner-routed recovery; planned account and saved marker alone are insufficient.
- **References:** `codex-switcher/src/distribution/distribution_desktop_auth_handoff_service.rs`, `codex-switcher/src/distribution/distribution_desktop_relaunch_service.rs`, `codex-switcher/src/distribution/distribution_recovery_audit_service.rs`, `codex-switcher/src/distribution/distribution_identity.test.rs`, `codex-switcher/src/switcher/account_switch_service.rs`, `codex-switcher/src/switcher/desktop_session_account_identity.test.rs`.
