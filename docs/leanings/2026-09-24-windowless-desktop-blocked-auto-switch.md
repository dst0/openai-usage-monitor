# 2026-09-24 — Window capture blocked automatic account switching

- **Status:** Partial
- **Task/context:** Diagnose automatic quota switching after a ChatGPT.app update.
- **Unexpected observation or failure:** The daemon selected a replacement account repeatedly but returned before changing credentials.
- **Evidence:** The installed daemon remained active with auto-switch enabled. Its audit log showed 37 consecutive `WINDOW_CAPTURE_FAILED` outcomes for automatic and menu requests, all before auth commit. The older helper reduced every capture error to one generic message, so those records do not prove whether the window was absent or Accessibility failed. A later transient process-exit race produced `got []` separately. The current helper successfully captured the current app window when checked.
- **Approaches tried:**
  - **Attempt:** Treat every capture error as an absent window.
    - **Outcome:** Did not use.
    - **Why:** It could bypass Accessibility, geometry, or process-identity failures and restart the wrong state.
  - **Attempt:** Distinguish an explicit missing standard window from all other helper failures, and carry that state through restart and verification.
    - **Outcome:** Worked in regression tests; live depletion remains unobserved.
    - **Why:** An absent window needs no geometry restore, but Desktop IPC and exact process verification still apply.
- **Root cause:** The transaction made a captured visible window mandatory before credentials could change; the helper also hid its reason for failure. The precise reason for the historical capture failures remains unknown.
- **Resolution:** The helper now emits fixed sanitized reason codes. Only explicit `WINDOW_NOT_FOUND` permits a windowless restart and recovery. Other failures still stop before shutdown or auth commit. A failed shutdown also clears its captured recovery state.
- **Verification:** Focused Rust tests cover windowless switch, access failure, fixed helper errors, and shutdown cleanup. The Swift helper compiled and captured the current ChatGPT window. The full repository gate and live post-install validation are recorded with the delivery result.
- **Prevention/follow-up:** Keep the distinct helper error contract and repeat a live depleted-quota switch when naturally available. Investigate the separate main-process exit race if it recurs.
- **Reusable learning:** Optional UI geometry must not block an account transaction when the exact app process has no eligible window; only an explicit absence result may take that path.
- **References:** `codex-switcher/src/distribution/system_app_lifecycle.rs`, `codex-switcher/src/distribution/distribution.test.rs`, `scripts/codex-window-restore.swift`.
