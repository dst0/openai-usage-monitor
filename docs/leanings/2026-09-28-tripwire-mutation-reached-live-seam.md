# 2026-09-28 — A mutation that removed a live-system tripwire let a test reach the real seam

- **Status:** Resolved
- **Task/context:** Hand-applied mutation checks of the reset durability change. One of the 20 mutations removed the new `crate::test_live_system::forbid("usage service")` call in `SystemWeeklyResetEnvironment::read_usage`, to prove that `unit_tests_cannot_reach_live_reset_effects` depends on it.
- **Unexpected observation or failure:** The test failed as intended. It failed because the call returned instead of panicking, which means the mutated test binary ran `quota::fetch_account_usage_read_only` for real.
- **Evidence:** The fixture account holds only synthetic tokens, and the test home was a private temporary `CODEX_HOME`. The run's output was not captured, so it is unknown whether an HTTPS request reached the service or failed earlier (DNS, sandbox, or connection). No real credential was involved.
- **Approaches tried:**
  - **Attempt:** Delete the tripwire to show that the test is not vacuous.
    - **Outcome:** Did not work
    - **Why:** The test did detect the missing tripwire. Detecting it required letting the live seam run, so the check caused exactly the side effect the tripwire exists to prevent.
  - **Attempt:** Change the tripwire's seam name, or keep the tripwire and mutate the logic behind it.
    - **Outcome:** Worked
    - **Why:** `assert_forbidden` checks the panic message for the exact seam, so a renamed seam still panics, which fails the assertion without reaching the live call.
- **Root cause:** A tripwire is the only thing between a unit test and a live seam. Any mutation that removes it turns the mutation run into a live run.
- **Resolution:** No mutation may remove or bypass a tripwire. Mutation checks of a tripwire test rename the seam string instead.
- **Verification:** Renaming the seam to `usage service (renamed by mutation)` made `unit_tests_cannot_reach_live_reset_effects` fail with "unexpected panic at the live usage service", and the renamed tripwire still panicked before the live call. The adversarial review was told not to mutate tripwires.
- **Prevention/follow-up:** The rule is recorded in `AGENTS.md` under "Test Quality & Adversarial Review".
- **Reusable learning:** Never mutate away a `test_live_system::forbid` tripwire. To prove that a tripwire test detects a gap, change the seam name so the panic message no longer matches, and leave the live call unreachable.
- **References:** `codex-switcher/src/test_live_system.rs`, `codex-switcher/src/auto_reset/system_weekly_reset_environment.rs`, `codex-switcher/src/auto_reset/system_weekly_reset_environment.test.rs`.
