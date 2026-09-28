# 2026-09-28 — Post-stop rollback discarded window tasks

- **Status:** Resolved
- **Task/context:** Preserve selected Desktop tasks during running-Desktop account distribution, including rollback after shutdown.
- **Unexpected observation or failure:** Post-stop errors relaunched the previous account, but the rollback and target-launch-failure paths finished the captured task session first. The previous Desktop could reopen without its selected tasks.
- **Evidence:** Source order in `distribution_desktop_rollback_service.rs` and `distribution_desktop_switch_service.rs` placed `finish_window_tasks` before `relaunch_if_auth_identity_matches` or `rollback_and_relaunch_previous`. Mock regressions now assert the exact previous-account session and the `launch`, `restore`, `finish` order for pre-commit rollback, post-commit rollback, and target launch failure.
- **Approaches tried:**
  - **Finish the task session before rollback:** Failed because `finish` consumes the snapshot, leaving nothing to restore.
  - **Carry the verified previous-account session out of guarded relaunch:** Worked in mock tests. Production restore waits for IPC and rechecks auth, marker, PID, and birth before navigating.
- **Root cause:** The new distribution task session was integrated into the success path, but post-stop rollback retained the earlier cleanup order.
- **Resolution:** Guarded previous-account relaunch returns its verified `DesktopAppSession`. Rollback calls window-task restore with that binding and finishes the snapshot afterward. Failed relaunch still finishes and reports incomplete restoration; auth rollback and journal handling remain separate.
- **Verification:** Focused Rust regressions pass for all three post-stop paths, including a partial restore result after target launch failure. `cargo check --locked`, `cargo fmt --all --check`, and `git diff --check` pass. Full Rust tests and Clippy were deferred when the shared host had only 2.0 GiB free and another compile reported `ENOSPC`. Installed-app restoration remains unverified.
- **Prevention/follow-up:** Any new post-stop exit must either restore the saved tasks against a verified relaunched session or report why that restore could not run. Keep automatic switching disabled until installed-app cold-task and multiwindow proof.
- **Reusable learning:** Do not consume a captured UI recovery snapshot before the guarded rollback has established the process that will receive it.
- **References:** `codex-switcher/src/distribution/distribution_desktop_rollback_service.rs`, `codex-switcher/src/distribution/distribution_desktop_switch_service.rs`, `codex-switcher/src/distribution/distribution_desktop_switch_service.test.rs`, `CODEX.md`.
