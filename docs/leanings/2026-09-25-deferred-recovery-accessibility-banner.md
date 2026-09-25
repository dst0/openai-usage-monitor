# 2026-09-25 — Deferred recovery entered an Accessibility banner path

- **Status:** Partial
- **Task/context:** Audit cold-task recovery after an account switch and restore the banner over ChatGPT during deferred recovery.
- **Unexpected observation or failure:** The ownerless retry daemon called `recover_threads`, which created a banner by capturing an Accessibility window before any Desktop IPC dispatch. A launchd Accessibility denial could stop recovery even after a cold task gained a real owner.
- **Evidence:** `DeferredRecoveryService::run_once` calls `recover_threads`; that entry point called `RecoveryBanner::start`, whose window capture uses the Accessibility `capture-window` command. The separate account-distribution path already used read-only `capture-banner-window` when window preservation was disabled. The installed helper previously displayed a layer-25 panel within ChatGPT using that read-only geometry.
- **Approaches tried:**
  - **Attempt:** Keep the same Accessibility capture for deferred recovery.
    - **Outcome:** Did not work.
    - **Why:** A background daemon can lack Accessibility permission even when Desktop IPC and WindowServer geometry work.
  - **Attempt:** Use the existing read-only WindowServer banner capture with exact PID and birth checks for recovery in an already running Desktop.
    - **Outcome:** Partial.
    - **Why:** Focused tests verify optional missing-window fallback and malformed-output rejection; installed end-to-end cold quota recovery remains unverified.
- **Root cause:** The recovery-only entry point reused the window-preserving account-restart banner constructor, coupling a cosmetic banner to Accessibility before task dispatch.
- **Resolution:** Recovery in an already running Desktop now uses WindowServer banner placement without geometry restoration. Only confirmed `WINDOW_NOT_FOUND` permits IPC without a panel, after rechecking process identity. Window access/geometry failures, panel timeout, changed identity, missing helper, payload/lease errors, and invalid helper output block dispatch.
- **Verification:** The focused backend regression invokes only `inspect-process` and `capture-banner-window`, continues after `WINDOW_NOT_FOUND`, and rejects malformed output, a changed process birth ID after `WINDOW_NOT_FOUND`, access/geometry failures, and a panel visibility timeout after successful capture. Full installed cold quota recovery remains pending.
- **Prevention/follow-up:** Test the full launchd ownerless-to-owned transition with an actual quota-interrupted task, including on-screen banner and substantive post-checkpoint work; keep automatic switching disabled until then.
- **Reusable learning:** Entry points that share a recovery engine must choose banner capture based on whether a Desktop restart and window restoration are actually occurring.
- **References:** `codex-switcher/src/recovery/deferred_recovery_service.rs`, `codex-switcher/src/recovery/running_desktop_banner.rs`, `codex-switcher/src/distribution/system_window_restore_backend.test.rs`, `2026-09-25-windowserver-banner-after-accessibility-bypass.md`.
