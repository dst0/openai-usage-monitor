# 2026-09-26 — Stale Desktop marker swapped APP and CLI quota labels

- **Status:** Resolved
- **Task/context:** Diagnose the OpenAI Usage Monitor menu while Desktop and CLI used separate accounts.
- **Unexpected observation or failure:** The menu showed `APP 0%` and `CLI 1160%` while the current Desktop session belonged to the account with `1160%` capacity.
- **Evidence:** Sanitized live inspection found a saved APP marker for an earlier ChatGPT process, while the exact running main process had a different PID and birth identity. Swift accepted a recent marker without checking its process. After that transaction completed, the cache reflected `APP 1160%` and `CLI 0%`. A separate code trace found that Rust wrote a new marker only after a potentially long recovery wait. A later process replacement again left the installed Monitor with an old marker.
- **Approaches tried:**
  - **Attempt:** Swap the APP and CLI labels in the menu.
    - **Outcome:** Rejected.
    - **Why:** The two consumers can legitimately use different accounts; a label swap would conceal the stale binding.
  - **Attempt:** Use the active CLI account as a fallback for APP.
    - **Outcome:** Rejected.
    - **Why:** A CLI credential change need not change the running Desktop session.
  - **Attempt:** Bind the saved APP account to the exact new process before recovery and verify that identity when reading it.
    - **Outcome:** Worked in focused regression tests.
    - **Why:** An old or absent marker can no longer supply a false APP percentage, and the current process marker is available during recovery.
- **Root cause:** The observed mismatch came from Swift treating marker recency as proof of the account in any running ChatGPT process. The separate late-write path left a longer interval with no current-process marker. Direct switch and restart paths also lacked consistent marker maintenance.
- **Resolution:** Rust writes a process-bound marker before recovery, verifies it before automatic distribution and under the operation lock, rechecks registry and CLI authentication under that lock, restores the previous Desktop on a post-stop write failure, and updates direct switch/restart paths. During recovery the marker names the account actually staged in `auth.json`; after CLI commit it names the committed CLI account. Swift checks exact PID, birth time, and marker lifetime; it shows `APP —` while identity is unverified and refreshes on marker and Desktop lifecycle events.
- **Verification:** Failing APP/CLI, stale-marker, stale-plan, CLI-auth-mismatch, and post-stop-write regression tests were reproduced before their fixes. `cargo test` passed 236 unit tests plus the integration suites; `./scripts/test_swift.sh` passed. Installed-app behavior still needs verification after a safe installation; another worktree has unrelated uncommitted recovery work.
- **Prevention/follow-up:** Keep APP and CLI identities independent. A same-PID account change inside ChatGPT is not detectable from process identity; an authoritative Desktop account read remains needed for that case. During an in-flight distribution, the CLI registry remains at the prior account until its commit, while `auth.json` temporarily holds the APP account for launch. Do not interpret the registry alone as live CLI identity during that interval.
- **Reusable learning:** A recent account marker is not live identity; bind it to the exact owning process and reject it after process replacement.
- **References:** `codex-switcher/src/distribution/distribution_desktop_relaunch_service.rs`, `codex-switcher/src/distribution/desktop_session_verification_service.rs`, `Sources/CodexClient.swift`, `codex-switcher/src/distribution/distribution.test.rs`, `tests/CodexClientIdentityTests.swift`.

## Post-install verification (2026-09-26)

The installer completed from commit `72df1e5`. The installed Monitor app and CLI both passed strict code-signature checks, and the installed Monitor process was running. After an independent ChatGPT app update, the live Desktop PID differed from the saved marker PID. The installed Swift code rejects that marker and renders the running APP quota as `—`; the current CLI quota cache was 940%. The official Desktop usage result also reported 47% remaining on a 20× plan (940%), down from the earlier 1160% observation. The menu-bar application has no ordinary window and the available UI automation timed out while binding to it, so the live rendered menu could not be observed directly. The regression tests and installed binary/source checks verify the behavior up to the UI rendering boundary; a visual menu check remains open.
