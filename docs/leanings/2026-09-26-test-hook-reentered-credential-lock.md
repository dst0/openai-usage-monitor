# 2026-09-26 — A test hook re-entered the credential lock

- **Status:** Resolved
- **Task/context:** Running the distribution transaction safety suite after credential compare-write hardening.
- **Unexpected observation or failure:** The first serial transaction test stopped indefinitely while simulating an external Desktop auth refresh.
- **Evidence:** A process sample placed the test thread in `flock` inside `write_active_auth_json`, called from a lifecycle hook invoked while the compare-write service already held the same Monitor lock. The full suite could not finish until the test process was interrupted.
- **Approaches tried:**
  - **Attempt:** Use the Monitor credential writer inside the external-writer hook.
    - **Outcome:** Did not work.
    - **Why:** It reacquired its own non-reentrant flock.
  - **Attempt:** Simulate the official Desktop's independent writer with a private `0600` same-filesystem staged file, sync, and rename, without acquiring the Monitor lock.
    - **Outcome:** Worked in focused tests.
    - **Why:** The test now exercises the intended competing pathname writer instead of blocking itself.
- **Root cause:** The test double used the cooperative Monitor writer to represent a Desktop writer that does not participate in that lock.
- **Resolution:** The synthetic external writer uses atomic private replacement without Monitor flock.
- **Verification:** All 16 transaction safety tests passed serially after the change; the earlier serial run hung at the first test and was interrupted.
- **Prevention/follow-up:** Review callback lock context before using repository helpers in concurrency tests.
- **Reusable learning:** Model an external writer through its actual coordination boundary; do not call a lock-taking helper from inside that lock.
- **References:** `codex-switcher/src/distribution/distribution_transaction_safety.test.rs`, `codex-switcher/src/storage/active_auth_compare_write_service.rs`.
