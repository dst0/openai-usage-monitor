# 2026-09-28 — Ownerless mount had no pending banner

- **Status:** Partial
- **Task/context:** Deferred recovery of an ownerless task after Desktop account distribution.
- **Unexpected observation or failure:** An accepted task link could begin mounting while no recovery panel was visible.
- **Evidence:** The deferred worker navigated in `select_scanned_ready_targets`, but created a `RecoveryBanner` only after owner discovery passed. A switch with no currently running recovery targets created `RecoveryBanner::without_window` and dropped it before deferred work.
- **Approaches tried:**
  - **Attempt:** Add ownerless IDs to the switch's running target list.
    - **Outcome:** Rejected.
    - **Why:** An ownerless checkpoint can belong to another account and cannot authorize recovery under the newly selected account.
  - **Attempt:** Start a bounded banner session only for a same-account, currently eligible deferred navigation attempt.
    - **Outcome:** Local focused tests pass; installed-app behavior remains unverified.
    - **Why:** It ties the visible panel to the exact account and process and retains one panel through owner proof and IPC recovery.
- **Root cause:** Banner creation was downstream of owner discovery, while task mounting happens before owner discovery.
- **Resolution:** The deferred worker now verifies one same-account eligible target, holds a `Pending` panel during a bounded mount attempt when a window is visible, and passes the same panel into recovery. A missing window after owner proof, timeout, or identity/helper failure cannot dispatch IPC.
- **Verification:** Focused Rust tests cover candidate selection, panel lifetime, timeout, identity change, and missing window. Installed Desktop behavior is pending.
- **Prevention/follow-up:** Verify the panel during a real cold mount in the installed app before claiming end-to-end unattended recovery.
- **Reusable learning:** URL acceptance and a retained checkpoint do not prove recovery; show a pending panel only for a verified same-account attempt, and require owner plus a live panel before dispatch.
- **References:** `codex-switcher/src/recovery/deferred_mount_banner_service.rs`, `codex-switcher/src/recovery/deferred_mount_banner_service.test.rs`, `CODEX.md`.
