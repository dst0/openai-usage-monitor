# 2026-09-26 — Ownerless prune tail reads

- **Status:** Resolved
- **Task/context:** Keep deferred recovery scans bounded while several cold tasks await owners.
- **Unexpected observation or failure:** A prune pass selected one ownerless checkpoint for bounded scanning but still classified every owner's rollout tail, reading up to hundreds of KiB per unselected task.
- **Evidence:** `ownerless_prune_avoids_tail_reads_for_unselected_targets` counted three tail-inspector calls before the fix although only one checkpoint cursor advanced.
- **Approaches tried:**
  - **Attempt:** Rotate one cached checkpoint scan but leave common tail classification in place.
    - **Outcome:** Did not work.
    - **Why:** The unselected tasks still caused rollout reads under the operation lock.
  - **Attempt:** Retain unselected ownerless retries without tail inspection; retire a selected one only on fresh confirmed work.
    - **Outcome:** Worked.
    - **Why:** Tail classification cannot by itself justify removal of an undispatched retry.
- **Root cause:** The bounded checkpoint scan and general tail classification were separate paths in the same loop.
- **Resolution:** Extracted `ManifestPruneService`; ownerless entries bypass tail classification after the selected confirmation check.
- **Verification:** The red regression passed after the fix; the 96-test `recovery::` suite passed.
- **Prevention/follow-up:** Count all rollout reads in performance tests, including tail inspection and boundary samples.
- **Reusable learning:** A bounded cursor does not bound a pass if another path reads every unselected file.
- **References:** `codex-switcher/src/recovery/manifest_prune_service.rs`, `codex-switcher/src/recovery/manifest_store.test.rs`.
