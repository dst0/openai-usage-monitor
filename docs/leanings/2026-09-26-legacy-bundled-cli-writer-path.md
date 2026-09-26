# 2026-09-26 — Older bundled CLI path escaped writer detection

- **Status:** Resolved
- **Task/context:** Review same-user Desktop writer detection before replacing shared `auth.json`.
- **Unexpected observation or failure:** A bundled Codex process from an older ChatGPT layout could remain alive while the shared-auth activity probe returned false.
- **Evidence:** A focused regression using the documented `Contents/Resources/codex` executable returned false before the fix and true afterward. The newer layout remains under `Contents/Resources/codex-cli/`.
- **Approaches tried:**
  - **Attempt:** Match only the newer `codex-cli/` prefix.
    - **Outcome:** Did not work.
    - **Why:** It excludes the older official bundled executable.
  - **Attempt:** Match the older executable exactly alongside the newer prefix.
    - **Outcome:** Worked.
    - **Why:** Both official layouts block credential replacement, while an unrelated `codex` path does not.
- **Root cause:** The new process guard assumed one Desktop package layout, even though the installer and user setup still support the previous one.
- **Resolution:** Added an exact legacy-path match to the checked shared-auth activity probe.
- **Verification:** The targeted old-layout and unrelated-executable regression passed after failing before the fix. Full post-change gates remain required.
- **Prevention/follow-up:** Keep supported Desktop executable layouts in process-probe fixtures when updating the official app.
- **Reusable learning:** A process safety guard must cover every supported application layout, not only the currently installed one.
- **References:** `codex-switcher/src/switcher/codex_process_probe.rs`, `codex-switcher/src/switcher.test.rs`, `CODEX.md`.
