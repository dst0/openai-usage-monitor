# 2026-09-26 — Ownerless rotation across homes

- **Status:** Resolved
- **Task/context:** Rotate which cold recovery checkpoint receives the bounded scan on each prune pass.
- **Unexpected observation or failure:** An otherwise unrelated prune in another `CODEX_HOME` advanced the same global cursor. Interleaved calls skipped one target in a three-target rotation.
- **Evidence:** The full binary test run failed `prune_rotates_one_ownerless_rollout_scan_per_pass` under parallel tests. The deterministic `ownerless_rotation_is_fair_when_other_homes_are_probed` regression covers two intervening probes of another home.
- **Approaches tried:**
  - **Attempt:** Use one global atomic call count modulo each invocation's target count.
    - **Outcome:** Did not work.
    - **Why:** Calls for other homes can repeatedly alias the same target index.
  - **Attempt:** Keep a bounded rotation cursor per home.
    - **Outcome:** Worked.
    - **Why:** Interleaved unrelated probes no longer consume a home's turn.
- **Root cause:** Rotation state was shared by different task sets.
- **Resolution:** `ManifestPruneService` stores at most 128 home-scoped cursors and recovers a poisoned coordination lock.
- **Verification:** Both rotation tests passed after the change.
- **Prevention/follow-up:** Keep rotation state tied to the task set; do not infer fairness from a single isolated invocation.
- **Reusable learning:** A global round-robin counter is not fair when independent sets share it.
- **References:** `codex-switcher/src/recovery/manifest_prune_service.rs`, `codex-switcher/src/recovery/manifest_store.test.rs`.
