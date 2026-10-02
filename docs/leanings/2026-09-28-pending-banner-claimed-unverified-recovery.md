# 2026-09-28 — Pending banner claimed unverified recovery

- **Status:** Resolved
- **Task/context:** Review of a deferred ownerless task mount banner before publishing PR #45.
- **Unexpected observation or failure:** The banner was visible before navigation but described Desktop restarting and continuing tasks, although no owner or resumed turn had been established.
- **Evidence:** The deferred mount path showed the panel before URL navigation; its payload reused the ordinary restart explanation. The review identified a possible 90-second owner wait with that text.
- **Approaches tried:**
  - **Attempt:** Reuse the restart payload while waiting for an owner.
    - **Outcome:** Did not work.
    - **Why:** A pending mount is not proof of a restart or continuation.
- **Root cause:** The no-geometry payload was shared between a restart and an ownerless mount without accounting for their different state.
- **Resolution:** Use neutral pending text when window restoration is skipped for the mount path.
- **Verification:** A payload regression asserts the pending explanation does not claim a restart or continuation; installed UI behavior remains to be checked.
- **Prevention/follow-up:** Keep payload wording tied to verified recovery state; test the installed banner after deployment.
- **Reusable learning:** A visible progress panel must describe proven state, not expected outcome.
- **References:** `codex-switcher/src/recovery_banner/recovery_banner_payload.rs`, `codex-switcher/src/recovery_banner/recovery_banner.test.rs`, PR #45.
