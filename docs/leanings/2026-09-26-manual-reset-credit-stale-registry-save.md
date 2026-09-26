# 2026-09-26 — Manual reset credit must merge into a fresh registry

- **Status:** Resolved
- **Task/context:** Audit manual account reset while registration and account settings can change concurrently.
- **Unexpected observation or failure:** After the reset service reported `Applied`, a whole-file save replayed the old registry, losing concurrent switch settings, account additions, or refreshed credentials.
- **Evidence:** Two mocked `Applied` tests failed against the old save. The first lost a concurrent disabled switch flag and added account; the second overwrote a rotated synthetic token.
- **Approaches tried:**
  - **Attempt:** Save the pre-request account snapshot after consumption.
    - **Outcome:** Did not work.
    - **Why:** The remote call left time for unrelated registry writers.
  - **Attempt:** Merge only the consumed credit count into a fresh locked registry, checking stable ID, tokens, route, and previous credit count.
    - **Outcome:** Worked.
    - **Why:** Unrelated fields survive, while a changed target reports consumed but uncertain cache state without another remote request.
- **Root cause:** A remote operation result was committed by replacing the entire stale account registry.
- **Resolution:** The reset flow now uses an atomic field-scoped commit; quota refresh follows the commit instead of running inside the stale snapshot.
- **Verification:** `cargo test --quiet account_reset_service::tests` passed three focused cases, covering unrelated changes, token conflict, and credit-count conflict.
- **Prevention/follow-up:** Keep remote credit requests outside the registry lock and retain the original idempotency semantics if commit conflicts.
- **Reusable learning:** Once an external action is applied, merge only its guarded local effect and report uncertainty rather than repeating it.
- **References:** `codex-switcher/src/setup/account_reset_service.rs`, `codex-switcher/src/setup/account_reset_service.test.rs`, `CODEX.md`.
