# 2026-09-26 — Distribution journal cleanup raced live recovery

- **Status:** Partial
- **Task/context:** Review account distribution while a Desktop recovery owns the shared operation lock.
- **Unexpected observation or failure:** A second distribution evaluated a journal older than five minutes and could clear it before attempting to acquire the recovery lock, even though the first recovery was still active. A stop error after signalling Desktop also cleared its own journal while the process state was uncertain.
- **Evidence:** A focused regression held the operation lock and placed an aged journal on disk. The old coordinator removed the journal before its transaction failed to acquire the lock. A second regression injected a post-signal stop error and observed the journal removed. These tests contain only synthetic account IDs and no credentials.
- **Approaches tried:**
  - **Attempt:** Use the journal's age and PID check before taking the recovery lock.
    - **Outcome:** Did not work
    - **Why:** A valid recovery can exceed the age threshold, and the check did not serialize with the active operation.
  - **Attempt:** Hold one operation lock through journal inspection, stale cleanup, candidate planning, and commit; retain the journal after an uncertain stop.
    - **Outcome:** Worked in focused tests
    - **Why:** A competing distribution cannot mutate the journal while recovery holds the lock.
- **Root cause:** Journal cleanup and planning occurred outside the lock that protected the actual Desktop recovery transaction.
- **Resolution:** The coordinator now acquires the recovery lock before reading the registry or journal and passes that lease through the transaction. A stop error after signal leaves the journal and checkpoint intact.
- **Verification:** The two regressions failed before the fix and passed after it. `cargo test --quiet distribution::` passed all 124 distribution tests. Full release gates and installed behavior remain pending.
- **Prevention/follow-up:** Keep journal inspection and candidate planning in the same critical section as account commit. Preserve recovery evidence when shutdown completion is uncertain.
- **Reusable learning:** An age threshold cannot make an in-flight operation stale while its lock is still held; acquire the lock before deciding to clean up its journal.
- **References:** `codex-switcher/src/distribution/distribution_coordinator.rs`, `codex-switcher/src/distribution/distribution_transaction_safety.test.rs`, `codex-switcher/src/distribution/distribution_desktop_switch_service.rs`.
