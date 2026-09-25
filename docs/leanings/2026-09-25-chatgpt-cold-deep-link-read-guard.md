# 2026-09-25 — ChatGPT cold deep link requires a successful thread read

- **Status:** Partial
- **Task/context:** Investigate why account rotation resumes owned tasks but some cold tasks remain `no-client-found` after a successful macOS `open` call.
- **Unexpected observation or failure:** Repeated `codex://threads/<id>` launches did not mount two affected tasks, while navigation through ChatGPT's own task action mounted one of them.
- **Evidence:** The installed ChatGPT application's deep-link handler for a local conversation returns without navigation when its `readThread(id, includeTurns: false)` call returns null or throws. The monitor's `open` exit code proves only that macOS delivered a URL, not that Desktop mounted the task. Subsequent owner discovery returned `no-client-found` in the recorded distribution.
- **Approaches tried:**
  - **Attempt:** Retry the accepted deep link during owner discovery.
    - **Outcome:** Did not work for the affected cold tasks.
    - **Why:** Retrying the same guarded route does not establish a Desktop owner when the thread read fails.
  - **Attempt:** Use ChatGPT's own task navigation, then owner-routed recovery.
    - **Outcome:** Worked for one affected task manually.
    - **Why:** The app navigation mounted the task before the monitor dispatched its recovery turn.
- **Root cause:** The installed Desktop build's local deep-link route depends on a successful pre-navigation thread read. Why that read failed for the affected tasks after account rotation remains unconfirmed.
- **Resolution:** Preserve the fail-closed `RECOVERY_INCOMPLETE` outcome when Desktop reports no owner. No unattended cold-task navigation route has been verified in this build.
- **Verification:** Installed app handler inspection and the earlier live owner-discovery evidence establish the guard and symptom. An automated end-to-end cold-task recovery test remains pending.
- **Prevention/follow-up:** Find and verify a supported Desktop-owned navigation method callable by the daemon, or prove that mounting targets before the restart survives relaunch. Require owner discovery before sending any turn.
- **Reusable learning:** A successful OS deep-link launch is not evidence that a task is mounted; always verify the Desktop owner before dispatch.
- **References:** `codex-switcher/src/recovery/desktop_ipc.rs`, `scripts/probe-desktop-ipc.js`, `2026-09-24-cold-desktop-deep-link-did-not-mount-task.md`.

**2026-09-25 follow-up:** A completed, unmounted local task returned `no-client-found` before account rotation. macOS accepted its deep link, but owner discovery still returned `no-client-found`. ChatGPT's built-in task navigation immediately mounted the same task, and the same read-only IPC probe then confirmed an owner. The issue therefore also reproduces without a credential transition; automatic recovery still needs a daemon-accessible Desktop navigation route.
