# 2026-09-28 — Last window link attempt changed an earlier task

- **Status:** Resolved
- **Task/context:** Review of PR #39's post-recovery selected-window recheck.
- **Unexpected observation or failure:** A delayed last link could change a window already marked verified while the intended target still failed to load. The recheck could return a true flag for the changed earlier window.
- **Evidence:** The final pass ran only when `show` returned true for a moved-back target. A fake Desktop sent the first link to an earlier window after retry two had checked it; the target never verified. The new focused test failed on the old code with a true flag for the earlier window.
- **Approaches tried:**
  - **Attempt:** Rely on the pre-retry check of previously verified windows.
    - **Outcome:** Did not work.
    - **Why:** A link can land after the last pre-retry check.
  - **Attempt:** Recheck all previously verified windows after any navigation attempt.
    - **Outcome:** Worked.
    - **Why:** Verification then reflects the window's task after the attempted link, even if its intended target never loaded.
- **Root cause:** The final validation condition tracked a successful target restore instead of whether a link might have changed another window.
- **Resolution:** Track attempted navigation and run the final previously verified window pass whenever a link was attempted.
- **Verification:** `recheckCatchesLastAttemptMisroute` reproduces the false flag before the change and passes afterward; the focused Swift window-task suite passes. Installed Desktop behavior remains to be tested.
- **Prevention/follow-up:** A failed navigation can still have side effects in another window. Run the full Swift gate and installed-app rehearsal after the combined patch is released.
- **Reusable learning:** Revalidate earlier effects after every attempted asynchronous navigation, not only after a successful target observation.
- **References:** `scripts/CodexWindowTaskSessionCore.swift`, `tests/CodexWindowTaskSessionCoreTests.swift`, PR #39.
