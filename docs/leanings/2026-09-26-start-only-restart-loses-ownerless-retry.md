# 2026-09-26 — Start-only restart lost an ownerless retry

- **Status:** Resolved
- **Task/context:** Adversarial review of repeated account-switch recovery in `save_pending`.
- **Unexpected observation or failure:** A second restart could replace a cold task's saved checkpoint and account binding after `task_started` alone, including while its queued follow-up still needed recovery.
- **Evidence:** Focused regressions failed before the fix: a start without agent work cleared `awaiting_owner`, and the same path ignored a nonempty `queue_1.sqlite` follow-up. The existing source comment and test expected that unsafe start-only replacement.
- **Approaches tried:**
  - **Attempt:** Use `task_started` as sufficient evidence of a replacement turn.
    - **Outcome:** Did not work.
    - **Why:** A turn can start without substantive work or while a queued follow-up remains.
  - **Attempt:** Require complete post-checkpoint evidence and an empty queue before replacing the retry.
    - **Outcome:** Worked in focused regressions.
    - **Why:** The original recovery intent survives ambiguous or still-queued states.
- **Root cause:** Restart journal merging used the `started` bit from the rollout scan instead of its stronger verified-work bit and never checked the queue.
- **Resolution:** `save_pending` keeps the old checkpoint unless the bounded scan proves substantive, error-free work and `pending_count` is zero.
- **Verification:** The start-only and queued-follow-up regressions failed before the fix and passed afterward; a long-rollout replacement with actual agent work also passed. Full integration validation remains part of this change.
- **Prevention/follow-up:** Keep checkpoint replacement and retry pruning on the same proof threshold. Correct the contradictory published learning and canonical docs.
- **Reusable learning:** A new turn start is an activity marker, not proof that a paused follow-up finished.
- **References:** `codex-switcher/src/recovery/restart_checkpoint_service.rs`, `codex-switcher/src/recovery/manifest_store.test.rs`, `docs/leanings/2026-09-26-stale-ownerless-checkpoint-after-second-switch.md`.
