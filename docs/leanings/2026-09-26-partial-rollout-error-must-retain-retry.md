# 2026-09-26 — Partial rollout error must retain retry

- **Status:** Resolved
- **Task/context:** Confirm a replacement turn before retiring an ownerless recovery checkpoint.
- **Unexpected observation or failure:** A newline-free, partially written `task_complete` error followed substantive work. The confirmation marked the snapshot complete and replaced the old retry.
- **Evidence:** `incomplete_failed_turn_cannot_retire_ownerless_checkpoint` failed before the fix because `save_pending` cleared `awaiting_owner`.
- **Approaches tried:**
  - **Attempt:** Trust complete parsed lifecycle events at the current file length.
    - **Outcome:** Did not work.
    - **Why:** A later incomplete JSONL record could change the turn's final result.
  - **Attempt:** Require a complete newline-terminated snapshot with no oversized trailing record.
    - **Outcome:** Worked.
    - **Why:** Destructive journal changes now wait until the final record can be classified.
- **Root cause:** The confirmation equated reaching EOF with reaching a complete JSONL boundary.
- **Resolution:** Check the observer's final offset and oversized state before accepting confirmation.
- **Verification:** The regression passed after the change; the 96-test `recovery::` suite passed.
- **Prevention/follow-up:** Keep the original account binding and checkpoint whenever a trailing record is incomplete.
- **Reusable learning:** Parsed work is not proof of an error-free turn while a later JSONL record is unfinished.
- **References:** `codex-switcher/src/recovery/checkpoint_confirmation.rs`, `codex-switcher/src/recovery/manifest_store.test.rs`.
