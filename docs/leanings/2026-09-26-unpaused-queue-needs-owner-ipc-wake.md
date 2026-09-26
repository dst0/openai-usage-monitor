# 2026-09-26 — An unpaused queue still needs an owner IPC wake

- **Status:** Resolved
- **Task/context:** Audit cold-task queued follow-up recovery after the Desktop owner has mounted the task.
- **Unexpected observation or failure:** The already-unpaused branch consumed the durable recovery checkpoint and set `dispatched`, but sent no IPC request. Owner discovery alone only found the window; it did not update or wake its queue coordinator.
- **Evidence:** The `pending > 0` branch called `mark_target_dispatch` before an `if unpaused` guard around `resume_existing_queue`; the `else` branch only logged `RECOVERY_QUEUE_MOUNTED`. A synthetic SQLite queue and UnixStream router test now asserts that an unpaused item receives one owner-targeted state update.
- **Approaches tried:**
  - **Attempt:** Treat owner mounting as a queue wake.
    - **Outcome:** Did not work.
    - **Why:** The owner-discovery call has no queue-state side effect in the Monitor protocol.
  - **Attempt:** Send the same owner-routed queue-state IPC update for both paused and already-unpaused snapshots.
    - **Outcome:** The synthetic owner router received one queue-state request, and the focused recovery tests passed.
    - **Why:** The call now occurs after the durable dispatch marker in both branches, preserving the one-send rule.
- **Root cause:** The branch mistook queue pause removal for the condition to send IPC.
- **Resolution:** Always send `thread-follower-set-queued-follow-ups-state` to the discovered owner after queue and rollout revalidation; retain the snapshot's messages if no restart pause exists.
- **Verification:** `CARGO_INCREMENTAL=0 cargo test --quiet recovery::target_dispatch_tests` passed 17/17 tests. The router test checked the discovered owner, original queue payload, durable checkpoint consumption, and dispatch flag. Installed Desktop turn production remains a separate live acceptance gate.
- **Prevention/follow-up:** Treat owner discovery as read-only. Verify a queue wake request and a real resumed turn separately on the installed Desktop build.
- **Reusable learning:** Do not consume a recovery checkpoint merely because a task mounted; require one owner-targeted action that can start or wake the work.
- **References:** `codex-switcher/src/recovery/target_dispatch.rs`, `codex-switcher/src/recovery/target_dispatch.test.rs`, `README.md`.
