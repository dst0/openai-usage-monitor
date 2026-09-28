# 2026-09-28 — Rehearsal liveness read masked an open window

- **Status:** Resolved
- **Task/context:** Review of cleanup after `cxi window rehearse-task-restore` opens temporary ChatGPT windows.
- **Unexpected observation or failure:** An Accessibility failure while reading a created window's role was treated as if the window had closed. The rehearsal could report success while leaving that window open. A failed post-opening window-list read could also discard the known focused new window before cleanup, or leave an unobserved extra window open.
- **Evidence:** `isWindowAlive` returned `false` for every unsuccessful AX role read; `close` skipped such windows. `createWindow` replaced a failed `standardWindows` read with an empty list. Scripted regressions cover both failure points.
- **Approaches tried:**
  - **Attempt:** Keep a Boolean liveness check and retry failed reads.
    - **Outcome:** Rejected.
    - **Why:** A retry limit cannot distinguish a closed window from persistently denied Accessibility access, and false still means closed.
  - **Attempt:** Make liveness reads throw on unknown AX status and preserve the focused new-window handle before re-enumeration.
    - **Outcome:** Worked in scripted tests.
    - **Why:** Cleanup can report uncertainty and attempt closure of a window already known to be new.
- **Root cause:** The code collapsed an unknown Accessibility state into the definite state “closed,” and discarded an enumeration error after opening a window.
- **Resolution:** Other AX role read errors throw; `invalidUIElement` is only a cleanup hint. Rehearsal success requires the final WindowServer/Accessibility IDs to equal the exact original IDs, so unknown or extra windows report `REHEARSAL_WINDOW_LEFT_OPEN`. After File > New Window, record a newly focused window before the full inventory read and propagate inventory errors.
- **Verification:** `CodexWindowTaskSessionCoreTests` covers an unreadable open-window liveness read, a stale AX element for a window still in WindowServer, a failed inventory read after opening with a known focused window, and a failed inventory read with an unobserved extra window. The latter regression failed before the exact final inventory check. These are scripted Desktop tests; live cleanup remains unverified.
- **Prevention/follow-up:** Preserve unknown states in Accessibility checks, keep handles to newly created resources before the next fallible inventory read, and compare the final OS window inventory to the initial inventory.
- **Reusable learning:** Never equate an Accessibility read failure with proof that a window disappeared.
- **References:** `scripts/CodexWindowTaskSession.swift`, `scripts/CodexWindowTaskSessionCore.swift`, `tests/CodexWindowTaskSessionCoreTests.swift`, `README.md`.
