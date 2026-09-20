# 2026-09-19 — Stale restart recovery manifest and ghost session resumption

- **Status:** Resolved
- **Task/context:** Investigating and fixing an issue where automatic account switching reported resuming 3 sessions when only 2 active sessions existed in ChatGPT Desktop.
- **Unexpected observation or failure:**
  - During automatic or manual account switches, Codex Monitor's recovery banner announced "Codex Monitor is restoring 3 tasks", even though the user had only 2 active sessions.
  - The 3rd session was a 4-day-old thread (`01a09e61-c0a7-75e1-9bf4-1f045644d983`) that had stalled on quota exhaustion days prior.
- **Evidence:**
  - In `~/.codex/log/switcher.log`:
    `Starting account switch ... [running_threads=3]`
    `RECOVERY_FAILED thread=01a09e61-... reason=Thread state Unknown is ambiguous without a pre-restart checkpoint; refusing to touch a possibly manually resumed task`
  - In `~/.codex/desktop-recovery.json`:
    The thread was permanently recorded and persisted.
  - Rollout tail seek window: `inspect_thread_rollout_state` read only 128 KB. Because the thread emitted a massive command execution output (>260 KB) after quota failure, 128 KB yielded zero turn events and returned `ThreadRolloutState::Unknown`.
  - In `recovery.rs`: `pending_manifest.retain(|item| item.id != target.id)` was only executed when `target.failure.is_none()`. Any target that failed recovery remained in `pending_manifest` and was rewritten to `desktop-recovery.json`.
  - In `switcher.rs`: `append_eligible_pending()` blindly appended any unarchived thread from `load_pending()`, ignoring age (`updated_at`) and rollout state (`CleanCompleted` / `Unknown`).
- **Approaches tried:**
  - **Attempt 1:** Simply delete `~/.codex/desktop-recovery.json`.
    - **Outcome:** Temporary fix only; any subsequent recovery failure would recreate a persistent zombie thread.
  - **Attempt 2:** Address root causes across the detection and recovery pipeline:
    1. In `append_eligible_pending`: enforce that pending threads must have `updated_at` within `RECENT_QUOTA_WINDOW_SECS` (4 hours) and must have active/interrupted rollout states (`ActiveInProgress`, `InterruptedByQuota`, `TurnAborted`). Cleanly completed and unknown states are rejected.
    2. In `recovery.rs`: prune all attempted targets from `pending_manifest` upon completion of a recovery run, whether they completed cleanly or failed.
    3. In `write_manifest`: remove `desktop-recovery.json` completely when `targets` is empty.
    4. In `inspect_thread_rollout_state`: fall back to reading 512 KB if the initial 128 KB window yields `ThreadRolloutState::Unknown` due to large trailing tool outputs.
    - **Outcome:** Worked completely and deterministically.
- **Root cause:**
  1. Failed recovery targets were never purged from `desktop-recovery.json`, treating transient restart recovery state as an indefinite retry queue.
  2. `append_eligible_pending` lacked recency and rollout state filtering, treating any thread in the manifest as an active task regardless of age or completion status.
- **Resolution:**
  - Updated `append_eligible_pending` in `codex-switcher/src/switcher.rs` to validate `updated_at` within `RECENT_QUOTA_WINDOW_SECS` and filter on rollout state.
  - Updated `inspect_thread_rollout_state` to retry with 512 KB on `Unknown` when large trailing command output exceeds 128 KB.
  - Implemented `prune_ineligible_targets` in `codex-switcher/src/recovery.rs` to expunge stale, completed, or unrecoverable entries directly from `desktop-recovery.json` on load in `load_pending()` and after recovery in `recover_threads_with_banner()`.
  - Updated `write_manifest` to remove the manifest file when targets are empty.
  - Cleaned up the stale `desktop-recovery.json` on disk.
- **Verification:**
  - Added regression test `test_append_eligible_pending_rejects_stale_and_completed_tasks` in `switcher.rs` (verified failure before fix, pass after).
  - Added regression test `test_inspect_thread_rollout_state_large_tail_fallback` in `switcher.rs` verifying 512 KB tail seek resolution.
  - Added regression test `test_load_pending_expunges_stale_targets_from_disk` in `recovery.rs` using RAII `TestCodexHomeGuard` to verify in-place disk pruning and file removal.
  - Full test suites passed (`switcher::tests` [17 passed] and `recovery::tests` [25 passed]).
  - Compiled and deployed updated release binary `codex-mon` to `~/.local/bin/`.
- **Reusable learning:** Restart journals and recovery manifests are point-in-time coordination hints, not durable retry queues. Handled targets must be pruned upon completion, and consumers of recovery hints must validate task recency and rollout validity rather than blindly trusting stale manifest entries.
- **References:** `codex-switcher/src/switcher.rs`, `codex-switcher/src/recovery.rs`, `~/.codex/desktop-recovery.json`.
