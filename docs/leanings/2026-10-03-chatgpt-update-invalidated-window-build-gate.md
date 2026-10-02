# 2026-10-03 — ChatGPT update invalidated the window helper's build gate

- **Status:** Partial
- **Task/context:** Restore selected ChatGPT window tasks after a Monitor account switch.
- **Unexpected observation or failure:** The installed ChatGPT was 26.928.31416 (build 12553), while the helper accepted only 26.924.22138 (build 11645). Every window-task command would reject the installed build before interacting with a window.
- **Evidence:** Read-only bundle inspection found the same `copyDeeplink` default binding, File > New Window command, and most-recently-focused primary-window routing in 26.928.31416. Its cold-task deep-link handler still stops navigation when its thread read returns no thread. No ChatGPT main process was running during this inspection.
- **Approaches tried:**
  - **Attempt:** Accept any future ChatGPT version.
    - **Outcome:** Did not use.
    - **Why:** A changed key binding or routing rule could send a link to the wrong window.
  - **Attempt:** Allowlist the newly inspected exact version and build.
    - **Outcome:** Worked in the build-gate regression test.
    - **Why:** Unknown builds still fail closed.
- **Root cause:** The exact build gate became stale after the app update. This explains preflight refusal on the new build; it does not explain the earlier `NEW_WINDOW_FAILED` on the previously accepted build.
- **Resolution:** Add 26.928.31416/12553 to the inspected-build list while retaining the older inspected build.
- **Verification:** `CodexWindowTaskProbeTests.swift` first failed on the new-build assertion, then passed; near-version and wrong-build cases remain rejected. Static inspection is not installed-app behavior proof.
- **Prevention/follow-up:** Re-inspect binding, keymap, link routing, and New Window after every update. Keep automatic switching disabled until a live multiwindow rehearsal and cold-task recovery pass.
- **Reusable learning:** Pin window automation to inspected exact app builds, and treat an update as a new compatibility check.
- **References:** `scripts/CodexWindowTaskProbeValidation.swift`, `tests/CodexWindowTaskProbeTests.swift`, `CODEX.md`.
