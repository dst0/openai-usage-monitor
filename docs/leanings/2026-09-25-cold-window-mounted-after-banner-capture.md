# 2026-09-25 — Cold task window can appear after banner capture

- **Status:** Partial
- **Task/context:** Ensure an ownerless cold task displays a recovery banner before Desktop IPC resumes it.
- **Unexpected observation or failure:** The first banner capture ran before a cold task deep link mounted its Desktop owner. It could report no window, then the link could create a visible ChatGPT window, allowing IPC recovery without a panel.
- **Evidence:** `recover_threads` created `RecoveryBanner` before `dispatch_if_needed` resolved the owner. `WINDOW_NOT_FOUND` returned a windowless banner. The original dispatch path had no second banner check between owner proof and its durable IPC marker.
- **Approaches tried:**
  - **Attempt:** Treat the first confirmed missing window as final for the whole recovery.
    - **Outcome:** Did not work.
    - **Why:** Desktop mounting can change visibility after that capture.
  - **Attempt:** Recreate the banner after owner proof and require its live helper before IPC.
    - **Outcome:** Partial.
    - **Why:** Unit tests cover the no-window gate, retained checkpoint, helper liveness, and a turn starting during panel startup; real quota-interrupted cold recovery remains unavailable to verify.
- **Root cause:** Banner creation and task mounting were separate stages, but initial no-window state was treated as permanent.
- **Resolution:** Before either queued or interrupted-turn IPC dispatch, recheck a missing panel after owner discovery. Stop and retain the original checkpoint if no live panel appears. Revalidate queue/rollout after panel startup, then recheck the panel and Desktop identity again after potentially slow SQLite reads and revalidate queue/rollout once more. Replay earlier statuses into the late panel, retaining the buffer and blocking dispatch on write failure. Compare the panel's current payload identity to the exact Desktop process.
- **Verification:** Targeted Rust regressions cover no-window deferral, retained checkpoint, helper exit, actual payload replay and failed write retention, a new turn or queued follow-up during the banner gate, a new turn during the final panel check, a changed queued revision with the same count, and helper exit after the SQLite recheck. Both new failure tests were observed red before their fixes and green after. Full installed quota-interrupted cold task recovery remains pending.
- **Prevention/follow-up:** Exercise ownerless-to-owned mounting with a real quota-interrupted task and verify the panel stays over the mounted window through substantive new work. Keep automatic switching disabled and PR draft until then.
- **Reusable learning:** A UI precondition captured before navigation must be checked again immediately before an irreversible dispatch; revalidate state after any slow UI readiness step.
- **References:** `codex-switcher/src/recovery/running_desktop_banner.rs`, `codex-switcher/src/recovery/target_dispatch.rs`, `codex-switcher/src/recovery/target_dispatch.test.rs`, `2026-09-25-deferred-recovery-accessibility-banner.md`.
