# 2026-09-26 — Offline rollback must verify the previous account credentials

- **Status:** Resolved
- **Task/context:** Reviewing an offline APP/CLI switch after the Desktop session marker could not be saved.
- **Unexpected observation or failure:** The rollback restored old `auth.json` tokens and the previous active ID, then cleared its journal even if the previous account had been reauthenticated in the registry during the operation.
- **Evidence:** `marker_failure_cannot_clear_journal_after_previous_account_relogin` failed before the fix: the rollback claimed success after a synthetic previous-account token change.
- **Approaches tried:**
  - **Attempt:** Check only that the active registry ID remained the selected target.
    - **Outcome:** Did not work.
    - **Why:** That does not show the restored auth tokens still match the prior account's registry tokens.
  - **Attempt:** Check the prior account's exact identity and credentials before auth restoration, recheck under the fresh registry lock, then verify both stores after rollback.
    - **Outcome:** Worked for the reproduced interleaving.
    - **Why:** A changed prior binding retains the journal and cannot be reported as a verified rollback.
- **Root cause:** The rollback treated the prior account ID as sufficient proof of the prior credential binding.
- **Resolution:** Offline rollback now validates the prior account token and identity against its saved baseline and restored authentication, with final readback before journal cleanup.
- **Verification:** `cargo test --quiet --bin codex-mon distribution::distribution_offline -- --test-threads=1` passed the marker failure and registry preservation cases after the fix.
- **Prevention/follow-up:** Retain the distribution journal on any cross-file uncertainty and reconcile the live auth, registry, and Desktop session before another switch.
- **Reusable learning:** Restoring an active account ID is insufficient when the account's credentials may have changed; verify the exact binding before declaring rollback complete.
- **References:** `codex-switcher/src/distribution/distribution_offline_registry_service.rs`; `codex-switcher/src/distribution/distribution_offline_commit_service.test.rs`.
