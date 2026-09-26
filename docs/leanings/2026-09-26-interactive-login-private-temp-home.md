# 2026-09-26 — Interactive login needs a private temporary home

- **Status:** Resolved
- **Task/context:** Inspect isolated CLI browser login during account setup.
- **Unexpected observation or failure:** The old PID-derived login directory was removed before login, even if it already contained another file.
- **Evidence:** A synthetic sentinel in the legacy PID directory disappeared in a red test and survived after the change.
- **Approaches tried:**
  - **Attempt:** Remove and recreate a predictable PID path.
    - **Outcome:** Did not work.
    - **Why:** Preexisting contents could belong to another process or earlier run.
  - **Attempt:** Reuse the private random `ReloginTempHome` lifecycle.
    - **Outcome:** Worked.
    - **Why:** Each login receives its own path and cleanup applies only to that owned directory.
- **Root cause:** Predictable path ownership was inferred from the process ID.
- **Resolution:** Interactive setup creates a private random home and drops it after login.
- **Verification:** `interactive_login_never_removes_a_preexisting_pid_directory` passed after failing against the old path.
- **Prevention/follow-up:** Keep temporary credential homes unpredictable and scoped to their creator.
- **Reusable learning:** A process ID does not prove ownership of a filesystem path.
- **References:** `codex-switcher/src/setup/interactive_setup.test.rs`, `codex-switcher/src/setup/relogin_temp_home.rs`, `CODEX.md`.
