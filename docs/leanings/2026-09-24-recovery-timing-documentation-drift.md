# 2026-09-24 — Recovery timing documentation drift

- **Status:** Resolved
- **Task/context:** Verify that the installed auto-switch fix and repository documentation match the current Rust recovery implementation.
- **Unexpected observation or failure:** The updated README timing table described the current values, but a later narrative paragraph still claimed a 90-second soak. AGENTS.md retained older startup, owner-discovery, dispatch, and stability limits and implied a visible window was always required.
- **Evidence:** `recovery_service.rs` sets IPC startup to 90 seconds; `desktop_ipc.rs` sets owner discovery to 90 seconds; `recovery_target.rs` sets dispatch, execution, and evidence soak to 90, 600, and 10 seconds; `window_restore.rs` sets Desktop stabilization to 3 seconds and only checks visibility when `require_window` is true.
- **Approaches tried:**
  - **Attempt:** Rely on the newly corrected README table alone.
    - **Outcome:** Did not work.
    - **Why:** Readers would still find contradictory claims in the README recovery steps and AGENTS.md.
  - **Attempt:** Compare each documented timing and visibility condition with the owning Rust constant and update both canonical documents.
    - **Outcome:** Worked.
    - **Why:** Each documented value now maps directly to the current code path.
- **Root cause:** The earlier fix updated the nearby summary table without auditing all repeated timing descriptions.
- **Resolution:** Corrected README.md and AGENTS.md to state the current timeouts and conditional window visibility requirement.
- **Verification:** Source-to-document comparison, focused text search for the obsolete values, and `git diff --check`.
- **Prevention/follow-up:** When a recovery constant or window requirement changes, search all canonical documents for both tables and narrative descriptions before delivery.
- **Reusable learning:** A timing table can be correct while nearby prose remains stale; verify all repeated claims against their owning constants.
- **References:** `codex-switcher/src/recovery/recovery_service.rs`, `desktop_ipc.rs`, `recovery_target.rs`, `window_restore.rs`, `README.md`, `AGENTS.md`.
