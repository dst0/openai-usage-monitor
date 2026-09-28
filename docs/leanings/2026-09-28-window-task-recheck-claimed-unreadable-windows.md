# 2026-09-28 — Window task recheck claimed unreadable windows

- **Status:** Resolved
- **Task/context:** Review of explicit multiwindow task restoration after a Desktop restart.
- **Unexpected observation or failure:** The post-recovery recheck could return `verified: true` for a planned window whose frame had no unique match or whose Copy deeplink could not be read. The restart could then print `WINDOW_TASKS_RESTORED` without confirming that window's task.
- **Evidence:** `WindowTaskSession.recheck` initialized all verification flags to `true`, skipped unmatched frames, and used `try?` for link reads without clearing the flag. A focused regression test against the original implementation stopped on the expected incomplete result.
- **Approaches tried:**
  - **Attempt:** Keep `true` for windows that recovery did not visibly change.
    - **Outcome:** Did not work.
    - **Why:** A missing frame or unreadable link gives no evidence that recovery left the task intact.
  - **Attempt:** Add a separate three-state result to the helper protocol.
    - **Outcome:** Rejected.
    - **Why:** The existing Boolean means verified or unverified; `false` already carries the honest result without changing the Rust/Swift wire format.
- **Root cause:** The recheck treated the absence of evidence of a recovery change as evidence of restoration.
- **Resolution:** Initialize recheck flags to `false`, mark an entry true only after its unique matching window copies the planned task or a recovery task was moved back and verified. Leave unmatched and unreadable windows untouched and unverified.
- **Verification:** `CodexWindowTaskSessionCoreTests` covers unreadable links, missing planned frames, user-changed tasks, and successful moves back. These are scripted Desktop tests; installed-app behavior remains unverified.
- **Prevention/follow-up:** Keep the `verified` flag tied to an actual readable task link. Exercise the explicit path on the installed Desktop before claiming live restoration.
- **Reusable learning:** A failed observation must not be recorded as successful restoration, even when the recovery action is deliberately conservative.
- **References:** `scripts/CodexWindowTaskSessionCore.swift`, `tests/CodexWindowTaskSessionCoreTests.swift`, `CODEX.md`.
