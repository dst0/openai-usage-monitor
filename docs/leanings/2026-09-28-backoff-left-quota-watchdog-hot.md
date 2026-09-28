# 2026-09-28 — Backoff left the quota watchdog hot

- **Status:** Resolved
- **Task/context:** Review of PR #43's automatic distribution backoff after repeated launchd window-access failures.
- **Unexpected observation or failure:** Holding the distribution attempt did not hold the daemon's two-second watchdog. A depleted active account could still wake a complete tick, including network quota refreshes and status writes, throughout a five-minute hold.
- **Evidence:** `daemon_loop_service.rs` checked depletion every two seconds and broke sleep; `daemon_tick_service.rs` refreshed enabled account quotas before the coordinator consulted `AutomaticDistributionBackoff`.
- **Approaches tried:**
  - **Attempt:** Back off only inside the distribution coordinator.
    - **Outcome:** Did not work.
    - **Why:** The expensive tick prelude had already run.
- **Root cause:** The hold was scoped to the switch decision but the watchdog controlled whole-tick scheduling upstream.
- **Resolution:** Suppress immediate watchdog wake while any automatic plan hold is active. Normal configured polling, auth-file wakeups, and lightweight deferred recovery polling continue.
- **Verification:** A regression arms a hold and proves the watchdog declines the immediate wake and recent-task scan, then resumes after expiry; full CI and installed behavior remain to be checked.
- **Prevention/follow-up:** Keep throttling ahead of expensive tick work and verify scheduled call frequency after adding a new retry gate.
- **Reusable learning:** A backoff must gate the earliest loop wake that starts expensive work.
- **References:** PR #43, `codex-switcher/src/distribution/daemon_loop_service.rs`, `codex-switcher/src/distribution/daemon_tick_service.rs`.
