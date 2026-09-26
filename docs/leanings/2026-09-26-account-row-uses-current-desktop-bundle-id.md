# 2026-09-26 — The account row must activate the current Desktop bundle ID

- **Status:** Resolved
- **Task/context:** Checking installed ChatGPT detection after the Desktop application name changed.
- **Unexpected observation or failure:** Clicking the active app account row searched only the historic `com.openai.chat` bundle ID, so it fell through to opening an app path even when the current Desktop process was already running.
- **Evidence:** The installed `/Applications/ChatGPT.app/Contents/Info.plist` reports `CFBundleIdentifier=com.openai.codex` and `CFBundleExecutable=ChatGPT`; the account-row lookup used only the historic identifier.
- **Approaches tried:**
  - **Attempt:** Rely on the path-opening fallback to activate the current process.
    - **Outcome:** Partial.
    - **Why:** It did not identify the already running application directly.
  - **Attempt:** Resolve `com.openai.codex` first, retaining the historic ID as a compatibility fallback.
    - **Outcome:** Worked for process identification.
    - **Why:** The lookup now matches the installed bundle identity.
- **Root cause:** One native menu interaction retained the old bundle identifier after the main detector and documentation changed.
- **Resolution:** The account row now activates the current bundle ID first.
- **Verification:** Installed app metadata was read directly, and `./scripts/test_swift.sh` passed after the account-row change.
- **Prevention/follow-up:** Keep all native Desktop activation paths aligned with the installed bundle metadata and rerun the Swift suite after edits.
- **Reusable learning:** An app rename can leave secondary UI activation paths using obsolete bundle identifiers even when core process detection is correct.
- **References:** `Sources/AccountRowView.swift`; `/Applications/ChatGPT.app/Contents/Info.plist`.
