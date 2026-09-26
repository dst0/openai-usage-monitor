# 2026-09-26 — Error-ended ownerless retries need a narrow pruning exception

- **Status:** Resolved
- **Task/context:** Reconciling the merged explicit-resume policy with bounded deferred recovery scans.
- **Unexpected observation or failure:** The bounded pruner retained every undispatched ownerless target, including a turn whose latest verified state was a non-quota error. Unattended recovery could never dispatch that turn, so it would keep probing it through the four-hour window.
- **Evidence:** The merged `unattended_retry_intent_is_dropped_for_error_ended_turns_only` regression required dropping the synthetic 401 target while retaining a quota target. The initial rebased pruner retained both.
- **Approaches tried:**
  - **Attempt:** Retain all ownerless targets until newer substantive work is confirmed.
    - **Outcome:** Partial.
    - **Why:** This protects ambiguous and quota-paused work but keeps terminal errors that require explicit resume.
  - **Attempt:** Inspect only the selected ownerless tail, dropping a non-quota terminal error only when pathname, inode, size, modification time, and change time remain stable.
    - **Outcome:** Worked in focused tests.
    - **Why:** It preserves the one-target scan bound and refuses malformed or changed tails while respecting explicit-only error recovery.
- **Root cause:** The prior blanket no-tail-pruning rule did not distinguish an undispatchable terminal error from a potentially recoverable cold target.
- **Resolution:** The selected ownerless target may be pruned on a stable `InterruptedByError`; other ownerless targets are not tail-inspected in that pass.
- **Verification:** The merged error-retention test and selected-tail-read test pass after rebase. The combined-tree Rust workspace gate passed with 493 unit tests and all integration suites; Clippy passed with `-D warnings`.
- **Prevention/follow-up:** Keep explicit-only recovery modes reflected in journal pruning tests and docs. This does not prove live ChatGPT task continuation.
- **Reusable learning:** Retain uncertain retry intent, but expire a provably ineligible terminal state without scanning every target.
- **References:** `codex-switcher/src/recovery/manifest_prune_service.rs`, `codex-switcher/src/recovery/manifest_store.test.rs`, `docs/leanings/2026-09-26-ownerless-prune-tail-reads.md`.
