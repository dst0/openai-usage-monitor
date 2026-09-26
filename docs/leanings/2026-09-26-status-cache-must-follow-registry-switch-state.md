# 2026-09-26 — Status cache must follow registry switch state

- **Status:** Resolved
- **Task/context:** Audit disabled auto-switch during cold-task recovery work.
- **Unexpected observation or failure:** A delayed status write could replay an older enabled flag after the registry had disabled auto-switch; Swift also defaulted missing cache flags to enabled.
- **Evidence:** The stale-status regression and synthetic Swift missing-flag regression failed before the fix and passed afterward.
- **Approaches tried:**
  - **Attempt:** Write a cached settings snapshot after a registry transaction.
    - **Outcome:** Did not work.
    - **Why:** A daemon or settings operation could commit newer settings between those steps.
  - **Attempt:** Copy current settings from the registry during each status write under the Monitor lock, and default missing Swift values to off.
    - **Outcome:** Worked.
    - **Why:** The final cache write reflects the final locked registry state, and missing data fails closed.
- **Root cause:** The derived cache had an independent last-writer-wins path and a fail-open UI default.
- **Resolution:** Rust status writers serialize with registry changes; Swift cache parsing and snapshot construction default to disabled.
- **Verification:** `cargo test --quiet status_file_service::tests` passed two tests; the focused `CodexClientIdentityTests` Swift binary passed missing, malformed, and explicit-true cases.
- **Prevention/follow-up:** Keep the status cache derived from current registry settings and exercise stale-writer order in tests.
- **Reusable learning:** A derived safety flag must never outlive or override its authoritative setting.
- **References:** `codex-switcher/src/storage/status_file_service.test.rs`, `tests/CodexClientIdentityTests.swift`, `README.md`.
