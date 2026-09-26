# 2026-09-26 — CLI-only rollback reported success

- **Status:** Resolved
- **Task/context:** Review distribution status during a CLI-only account switch.
- **Unexpected observation or failure:** A failed registry write rolled back CLI authentication but the transaction returned `PartialSuccess` with an accounts-distributed message.
- **Evidence:** A focused test blocked the atomic accounts temporary file after planning. The transaction left both auth and registry on the original account but reported success before the fix.
- **Approaches tried:**
  - **Attempt:** Treat every CLI commit error as partial success.
    - **Outcome:** Rejected.
    - **Why:** A rolled-back transaction has not distributed an account.
  - **Attempt:** Return an error and clean the distribution journal after a failed CLI-only commit.
    - **Outcome:** Worked.
    - **Why:** The response matches durable auth and registry state.
- **Root cause:** The CLI-only commit service converted a commit error into an optional recovery error, which the transaction mapped to `PartialSuccess`.
- **Resolution:** Propagate CLI-only commit failure as an error; keep partial success for committed changes with incomplete follow-up verification.
- **Verification:** `cli_only_registry_write_failure_is_not_reported_as_distributed` failed before the fix and passed after it; the full Rust suite is the final gate.
- **Prevention/follow-up:** Compare returned status with durable state in error-path tests.
- **Reusable learning:** A successful rollback must never be labelled a successful commit.
- **References:** `codex-switcher/src/distribution/distribution_cli_commit_service.rs`, `codex-switcher/src/distribution/distribution_transaction_service.rs`, `codex-switcher/src/distribution/distribution_transaction_safety.test.rs`.
