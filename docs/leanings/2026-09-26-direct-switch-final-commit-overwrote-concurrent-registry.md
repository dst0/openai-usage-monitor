# 2026-09-26 — Direct switch final commit overwrote concurrent registry changes

- **Status:** Resolved
- **Task/context:** Hardening direct `cxi switch` after its Desktop authentication sync and shutdown.
- **Unexpected observation or failure:** The final active-account save could restore earlier settings, account metadata, or credentials, and could resurrect a target removed while Desktop was stopping.
- **Evidence:** Isolated on-disk registry tests inserted writes immediately before the final commit. The old whole-file save reverted an auto-switch disable and another account's synthetic token, accepted a rotated target token, and recreated a removed target. A later test showed that a concurrent active-account change also needed rejection. The tests failed against the old behavior and passed after the change. No live account was switched.
- **Approaches tried:**
  - **Attempt:** Persist the pre-shutdown `AccountsFile` snapshot after replacing active auth.
    - **Outcome:** Did not work.
    - **Why:** The operation lock does not cover configuration, daemon, or account registration writes; saving the old vector erases their updates.
  - **Attempt:** Reopen the registry under its transaction lock, check one selected ID with unchanged tokens, and update only `active_account_id`.
    - **Outcome:** Worked in focused tests.
    - **Why:** Concurrent unrelated fields remain in the fresh registry; target changes fail before commit and use the caller's auth rollback path.
- **Root cause:** `switch_to_account` called `save_accounts` with a registry snapshot retained across a potentially long Desktop shutdown.
- **Resolution:** The final direct-switch commit now merges only the active ID into a fresh locked registry and refuses changed prior active identity, missing or duplicate target, changed credentials or account identity, and newly invalid eligibility.
- **Verification:** Focused `cargo test switcher::account_switch_ --bin codex-mon` passed the isolated switch tests after the original implementation produced red regressions. Live installed-app switching remains a separate verification gate.
- **Prevention/follow-up:** Keep final switch commits field-scoped, and retain deterministic interleaving tests for registry changes during shutdown.
- **Reusable learning:** Never commit a pre-shutdown account-registry snapshot after writing shared authentication; merge the intended field into fresh state and fail closed when target credentials changed.
- **References:** `codex-switcher/src/switcher/account_switch_service.rs`, `codex-switcher/src/switcher/account_switch_commit_service.rs`, `codex-switcher/src/switcher/account_switch_commit_service.test.rs`, `CODEX.md`.
