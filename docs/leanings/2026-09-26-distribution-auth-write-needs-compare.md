# 2026-09-26 — Desktop distribution must compare shared auth before replacement

- **Status:** Resolved
- **Task/context:** Hardening account distribution while the Desktop is stopped and another local credential writer may run.
- **Unexpected observation or failure:** Distribution read `auth.json`, checked that Desktop was stopped, then replaced the file unconditionally. An intervening credential rotation or login was overwritten; rollback had the same gap.
- **Evidence:** The synthetic regressions `desktop_switch_rejects_concurrent_auth_replacement` and `rollback_rejects_concurrent_auth_replacement` both failed before the fix, preserving neither external update.
- **Approaches tried:**
  - **Attempt:** Check only whether Desktop was running before each write.
    - **Outcome:** Did not work.
    - **Why:** Another local writer can change authentication while Desktop remains stopped.
  - **Attempt:** Compare the exact prior authentication and inode immediately before replacement, then verify readback.
    - **Outcome:** Worked for the reproduced race.
    - **Why:** A changed auth document blocks both the forward switch and rollback without overwriting the external update.
- **Root cause:** The stopped-Desktop check was mistaken for exclusive ownership of the shared credential file.
- **Resolution:** Forward Desktop distribution and rollback now use the guarded compare-and-replace service and require exact offline readback.
- **Verification:** `cargo test --quiet --bin codex-mon distribution_account_commit_service::tests -- --test-threads=1` passed, including both intervening-write regressions.
- **Prevention/follow-up:** The official Desktop does not honor the Monitor advisory lock, so an external write at the final rename boundary cannot be made fully atomic; retain the distribution journal on any uncertain outcome and verify the installed path before enabling automation.
- **Reusable learning:** An application process being stopped does not grant exclusive ownership of a shared credential file; compare the exact previous file immediately before replacing or rolling it back.
- **References:** `codex-switcher/src/distribution/distribution_account_commit_service.rs`; `codex-switcher/src/storage/active_auth_compare_write_service.rs`.
