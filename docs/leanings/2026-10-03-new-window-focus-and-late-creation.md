# 2026-10-03 — New Window can outlast the helper's focus wait

- **Status:** Partial
- **Task/context:** Diagnose the live `NEW_WINDOW_FAILED` during the selected-task restoration rehearsal.
- **Unexpected observation or failure:** The previous helper waited five seconds for a newly focused, keyed AX window after File > New Window. One failure code covered a delayed window, a background window, no window, and multiple windows. The earlier live rehearsal failed with that code; its exact cause remains unknown.
- **Evidence:** The installed bundle's menu handler awaits asynchronous window creation. Scripted tests reproduced a background window and a seven-second creation delay that the old predicate rejected. No current live rehearsal has run.
- **Approaches tried:**
  - **Attempt:** Treat a successful menu press as proof that a new window exists.
    - **Outcome:** Did not use.
    - **Why:** The press can return before a window appears, and no task link may be sent to an unverified target.
  - **Attempt:** Poll for exactly one new standard window, then focus and verify that exact window.
    - **Outcome:** Worked in scripted sequencing tests.
    - **Why:** It accepts a delayed or background window while preserving the exact-focus check before a task link.
- **Root cause:** The helper conflated window creation with immediate keyboard focus. Whether that caused the earlier live failure is unproven.
- **Resolution:** Wait up to ten seconds with a bounded poll interval, record observed windows for cleanup, and require verified keyboard focus before navigation. Track each accepted menu action independently; a rehearsal that cannot observe its new window reports uncertain cleanup because one may appear later.
- **Verification:** `CodexWindowTaskSessionCoreTests.swift` first failed for background and delayed windows and for late-window uncertainty, then passed. It covers a delayed second window, a failed post-action AX inventory, an unfocusable window, and multiple new windows. These are fake Desktop tests, not installed-app proof.
- **Prevention/follow-up:** Run the explicit rehearsal on the installed ChatGPT build with multiple windows. A simultaneous user-created window cannot be attributed to the menu action, so avoid other window actions during the operation; keep automatic switching disabled until the live path and cold-task mounting pass.
- **Reusable learning:** A menu action's return value is not a window identity or a focus witness; verify both before routing a task link.
- **References:** `scripts/CodexWindowTaskSessionCore.swift`, `tests/CodexWindowTaskSessionCoreTests.swift`, `README.md`.
