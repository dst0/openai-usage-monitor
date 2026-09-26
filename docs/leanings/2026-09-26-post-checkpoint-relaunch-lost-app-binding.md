# 2026-09-26 — Post-checkpoint relaunch lost APP binding

- **Status:** Resolved
- **Task/context:** Merge the APP identity fix with the newer post-shutdown recovery checkpoint on `main`.
- **Unexpected observation or failure:** A failed second checkpoint relaunched the previous Desktop account but left its APP marker bound to the stopped process.
- **Evidence:** A focused regression test made the marker stale when the mock Desktop stopped. The existing branch returned an error and relaunched Desktop, but the marker still had the stale birth identity.
- **Approaches tried:**
  - **Attempt:** Reuse the full recovery relaunch after checkpoint failure.
    - **Outcome:** Rejected.
    - **Why:** An invalid checkpoint must not trigger recovery requests that could duplicate work.
  - **Attempt:** Relaunch the previous APP account, bind the exact process, and restore CLI authentication without recovery dispatch.
    - **Outcome:** Worked.
    - **Why:** The new marker has verified process identity while failed checkpoint state remains undispatched.
- **Root cause:** The checkpoint rollback path predated the process-bound APP marker contract and only verified Desktop stability after launch.
- **Resolution:** The rollback stages the previous APP authentication, relaunches and binds the new process with that staged authentication, then restores the original CLI authentication and reconciles the marker. If CLI authentication restoration fails, the marker keeps the staged APP account as its expected CLI binding. It reports any binding or stability failure.
- **Verification:** `failed_post_shutdown_checkpoint_keeps_old_auth_and_relaunches_desktop` and `failed_cli_restore_keeps_checkpoint_relaunch_binding_on_staged_app_auth` failed before their fixes and passed after them; the full Rust suite is the final gate.
- **Prevention/follow-up:** Every Desktop launch path must either write a verified account binding or leave APP explicitly unknown. Never dispatch recovery after a failed checkpoint.
- **Reusable learning:** A successful process relaunch is not complete account recovery until its marker is rebound to the new process.
- **References:** `codex-switcher/src/distribution/distribution_transaction_service.rs`, `codex-switcher/src/distribution/distribution_post_stop_recovery_service.rs`, `codex-switcher/src/distribution/distribution.test.rs`.
