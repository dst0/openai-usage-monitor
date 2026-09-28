# 2026-09-28 — Auth rotation errors need operation evidence

- **Status:** Partial
- **Task/context:** Resume a selected cold ChatGPT task after Monitor changes between the device owner's accounts.
- **Unexpected observation or failure:** A token-refresh failure after an account change was classified as a generic non-quota error, so automatic detection, deferred pruning, and dispatch all excluded it. A selected chat could mount without receiving a resume IPC request.
- **Evidence:** `thread_rollout_inspector.rs` classifies the exact logged-out-or-other-account terminal error as `InterruptedByError`; `target_dispatch_policy.rs` previously allowed that state only for explicit requests. The first and second calls to `save_pending` replace the recovery offset, so the old manifest alone cannot prove when an error occurred.
- **Approaches tried:**
  - **Attempt:** Allow all 401 or `InterruptedByError` turns to resume automatically. **Outcome:** Did not use. **Why:** That would also revive policy blocks, user stops, and errors predating the switch.
  - **Attempt:** Treat an accepted task URL as proof of a mounted owner. **Outcome:** Did not use. **Why:** ChatGPT can accept the URL while IPC still returns `no-client-found`.
  - **Attempt:** Keep operation-specific pre-stop evidence and confirm the exact terminal error before resetting the proof offset. **Outcome:** Focused tests pass. **Why:** It binds the exception to an active turn, the verified account route, and a stable bounded interval.
- **Root cause:** A generic error state had no durable evidence connecting it to the current account switch. The post-stop checkpoint intentionally erased the timing information needed to make that distinction.
- **Resolution:** Store source and target account IDs, active turn ID, first offset, rollout device/inode, and the queue revision captured before that offset in the private recovery manifest. Confirm the exact auth-refresh terminal event in that same file between checkpoints with no Stop, new turn, or new user input; permit automatic dispatch only with that confirmed record under the verified target account and real IPC owner.
- **Verification:** Focused Rust checkpoint, parser, account-binding, rollout-replacement, and ownerless-prune tests pass. The manifest reader also has focused oversized-file and dangling-symlink regressions. Full Rust gates and installed multi-account cold-chat behavior remain to be verified.
- **Prevention/follow-up:** Keep historical errors explicit-only. Run a live owned-account switch with a cold task and confirm owner discovery, one IPC dispatch, post-dispatch agent work, and banner/window behavior before claiming the full flow works.
- **Reusable learning:** A terminal auth error is not proof of a recoverable account-switch interruption; require a bounded operation-specific before/after checkpoint and exact account and owner binding.
- **References:** `codex-switcher/src/recovery/auth_rotation_checkpoint_service.rs`, `codex-switcher/src/recovery/target_dispatch_policy.rs`, `README.md`.
