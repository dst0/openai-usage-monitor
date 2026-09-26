# 2026-09-26 — Queued recovery must check turn mode

- **Status:** Resolved
- **Task/context:** Audit queued follow-up dispatch alongside explicit-only non-quota errors and historical user Stop rules.
- **Unexpected observation or failure:** When a queue item existed, the dispatcher bypassed `should_dispatch` and its owner revalidation skipped the current rollout state. A queued `TurnAborted` or `InterruptedByError` could therefore reach the dispatch marker in a discovery-only run.
- **Evidence:** The old queue branch entered on `pending > 0` without a turn-mode check, while `revalidate_after_owner_with_budget` inspected the current turn only when `prior_pending == 0`. Synthetic queued rollout tests cover an aborted turn and a non-quota error under discovery-only and allowed modes. Full dispatcher fixtures use a valid queued payload, a discoverable rollout, and a fake IPC router to assert that rejected modes neither contact a Desktop owner nor consume the checkpoint.
- **Approaches tried:**
  - **Attempt:** Rely on initial target discovery to enforce turn eligibility.
    - **Outcome:** Did not work.
    - **Why:** A queued target can reach dispatch through a checkpoint even when the underlying turn is ambiguous, and owner mounting can take 90 seconds.
  - **Attempt:** Check queue-specific turn eligibility before owner discovery and again from the current rollout after owner/banner waits.
    - **Outcome:** Focused tests passed for discovery-only rejection and captured or explicit authorization.
    - **Why:** The policy now distinguishes quota interruption, explicit-only errors, captured or explicit aborts, and queued follow-ups after a completed turn.
- **Root cause:** The queue path used queue presence as sufficient authorization and did not share the turn-mode gate with the empty-queue path.
- **Resolution:** Added a named dispatch policy for queued and empty-queue states. Queued non-quota errors require explicit targeting; a historical user Stop requires a captured restart or explicit targeting. Revalidation rereads the rollout state for both queue states.
- **Verification:** `CARGO_INCREMENTAL=0 cargo test --quiet recovery::target_dispatch_tests` passed 19/19 tests, including both full-dispatch negative cases: discovery-only user Stop and captured non-quota error kept their manifest checkpoint and sent no fake Desktop IPC request. Earlier IPC policy tests passed 7/7 and queue tests passed 3/3. A non-quota error was accepted only for an explicit target; a user-aborted turn was accepted only for a captured restart or explicit target. Synthetic IPC proof does not establish that the installed Desktop starts a real turn.
- **Prevention/follow-up:** Keep mode rules on every path that can mark a recovery dispatch, including queue-present paths and late owner rechecks.
- **Reusable learning:** Pending queue data does not authorize reviving an otherwise ineligible turn.
- **References:** `codex-switcher/src/recovery/target_dispatch_policy.rs`, `codex-switcher/src/recovery/target_dispatch.test.rs`, `codex-switcher/src/recovery/ipc_protocol.test.rs`, `AGENTS.md`.
