# 2026-09-26 — Interactive account setup must propagate state errors

- **Status:** Resolved
- **Task/context:** Audit multi-account setup before account distribution.
- **Unexpected observation or failure:** Setup treated an unreadable account registry as empty and ignored a failed save of the previous active session.
- **Evidence:** Focused synthetic regressions failed against the old fallbacks and passed after removing them; no live credentials were read.
- **Approaches tried:**
  - **Attempt:** Default the registry and continue after an active-session save failure.
    - **Outcome:** Did not work.
    - **Why:** A later account addition could hide stored accounts or lose the previous active session.
  - **Attempt:** Return each registry, active-auth, and save error to the setup caller.
    - **Outcome:** Worked.
    - **Why:** Setup stops before claiming a safe account addition.
- **Root cause:** Error-swallowing branches treated unavailable credential state as absent state.
- **Resolution:** Interactive setup now propagates load, read, and save failures and checks active-auth presence before first-account initialization.
- **Verification:** `cargo test --quiet interactive_setup::tests` passed all four focused tests.
- **Prevention/follow-up:** Preserve the error path in setup regressions; account creation must not replace unknown state.
- **Reusable learning:** An unreadable credential registry or active-auth file is never equivalent to an empty one.
- **References:** `codex-switcher/src/setup/interactive_setup.rs`, `codex-switcher/src/setup/interactive_setup.test.rs`, `README.md`.
