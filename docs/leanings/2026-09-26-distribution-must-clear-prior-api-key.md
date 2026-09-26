# 2026-09-26 — Desktop account switching must clear the prior API key

- **Status:** Resolved
- **Task/context:** Hardening shared `auth.json` changes during Desktop account distribution.
- **Unexpected observation or failure:** The replacement document cloned the prior account's authentication and changed only its token object, leaving an optional API key from the prior account in the new account's document.
- **Evidence:** The synthetic regression `desktop_switch_does_not_carry_previous_api_key_to_next_account` failed before the fix because the switched document retained the key.
- **Approaches tried:**
  - **Attempt:** Clone the previous authentication and replace tokens only.
    - **Outcome:** Did not work.
    - **Why:** A top-level credential field unrelated to the token object survived the switch.
  - **Attempt:** Clear the prior API key while replacing the entire token object.
    - **Outcome:** Worked.
    - **Why:** The target account receives only its own saved token fields while noncredential top-level extensions can remain.
- **Root cause:** The switch treated the prior account's authentication document as a safe template without clearing account-specific credential fields outside `tokens`.
- **Resolution:** Desktop distribution now removes the prior API key from the target authentication document before the guarded replacement.
- **Verification:** `cargo test --quiet --bin codex-mon distribution_account_commit_service::tests -- --test-threads=1` passed, including the synthetic cross-account key regression.
- **Prevention/follow-up:** Keep a regression for every credential field outside `tokens` when the shared authentication schema expands.
- **Reusable learning:** Account switching must explicitly enumerate and remove credentials that belong to the previous account, even when preserving unrelated document extensions.
- **References:** `codex-switcher/src/distribution/distribution_account_commit_service.rs`; `codex-switcher/src/distribution/distribution_account_commit_service.test.rs`.
