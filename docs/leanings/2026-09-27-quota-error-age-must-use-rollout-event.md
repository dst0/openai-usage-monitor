# 2026-09-27 — Date quota failures from rollout events

- **Status:** Resolved
- **Task/context:** Detect quota-blocked Codex tasks eligible for recovery or a reset credit.
- **Unexpected observation or failure:** Opening an old failed task can refresh its SQLite `threads.updated_at`, making a quota failure older than four hours appear recent.
- **Evidence:** A focused fixture with a five-hour-old quota `task_complete`, a newer settings event, and a current SQLite `updated_at` was selected by the old pending-target detector. A separate manifest-prune regression retained the same old failure and discarded a fresh quota failure whose SQLite row was stale. Both tests failed before their fixes and passed afterward.
- **Approaches tried:**
  - **Attempt:** Use SQLite `updated_at` as the failure age.
    - **Outcome:** Did not work.
    - **Why:** Desktop navigation updates task metadata without creating a new failed turn.
  - **Attempt:** Date the latest quota-classified `task_complete` in the bounded rollout tail.
    - **Outcome:** Worked.
    - **Why:** The event timestamp belongs to the failure being considered; absent, malformed, or future timestamps fail closed.
- **Root cause:** The detector conflated task metadata recency with quota-event recency in discovery, active-lock, and restart-manifest paths.
- **Resolution:** Use the rollout event's RFC 3339 timestamp for every quota-recency decision, including manifest pruning. Keep SQLite `updated_at` for non-quota pending turns. Manifest pruning checks rollout identity and metadata around state and timestamp reads; a concurrent append leaves the checkpoint intact for retry. For an ownerless retry with an undated quota event, it returns an error so the checkpoint survives without authorizing deferred IPC.
- **Verification:** `cargo test thread_detection_service::tests` (2), `cargo test manifest_prune_service::tests` (3), `cargo test manifest_store_tests::` (40), `cargo clippy --all-targets -- -D warnings`, and the file-limit tests (2) passed. The new tests cover a reopened old failure, a recent failure with stale SQLite metadata, missing and malformed timestamps, future time, the four-hour boundary, and appends during both rollout-state inspection and quota-timestamp extraction.
- **Prevention/follow-up:** Treat SQLite `updated_at` as task-list ordering data, not evidence of when a failure occurred. The top-30 SQLite candidate cap can still miss a fresh quota event on a row outside that rank; complete discovery would need an event-time index or a bounded rotating scan with a persisted cursor.
- **Reusable learning:** Time eligibility for an error must come from the error event, not a mutable task metadata timestamp.
- **References:** `codex-switcher/src/switcher/thread_detection_service.rs`; `codex-switcher/src/switcher/thread_detection_service.test.rs`; `codex-switcher/src/recovery/manifest_prune_service.rs`; `codex-switcher/src/recovery/manifest_prune_service.test.rs`.
