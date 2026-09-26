# 2026-09-26 — Reject malformed WindowServer bounds before banner placement

- **Status:** Resolved
- **Task/context:** Place the recovery banner above the exact ChatGPT window while keeping account-switch recovery fail closed.
- **Unexpected observation or failure:** `captureBannerWindow` force-cast a WindowServer bounds payload to `CFDictionary`. A malformed record for the target process could crash the helper instead of returning a controlled geometry failure.
- **Evidence:** The old branch used `bounds as! CFDictionary` before `CGRect(dictionaryRepresentation:)`. A new regression first failed to compile because no validated banner-frame classifier existed; its cases cover foreign payload types, missing fields, booleans, non-finite numbers, and a valid native `NSDictionary`.
- **Approaches tried:**
  - **Attempt:** Use `as? CFDictionary` and let `CGRect(dictionaryRepresentation:)` decide.
    - **Outcome:** Rejected.
    - **Why:** It gives no explicit finite-number or Boolean validation and can silently skip a malformed target window.
  - **Attempt:** Parse the known WindowServer numeric fields after checking the record belongs to the exact target PID.
    - **Outcome:** Worked.
    - **Why:** A malformed candidate now maps to `WINDOW_GEOMETRY_FAILED`; unrelated windows are ignored.
- **Root cause:** The helper trusted an OS-provided but dynamically typed dictionary at a presentation boundary.
- **Resolution:** `CodexWindowSafetyChecks.swift` validates bounds and classifies target windows. The helper also rechecks PID birth immediately before each Accessibility position or size write.
- **Verification:** `CodexWindowSafetyChecksTests.swift` passed valid negative-origin, native dictionary, malformed-type, missing-field, non-finite, Boolean, unrelated-PID, and changed-birth cases. The optimized helper built. No live window mutation was attempted.
- **Prevention/follow-up:** Run the safety test through `scripts/test_swift.sh` and keep WindowServer parsing and exact birth checks at the helper boundary.
- **Reusable learning:** Treat dynamically typed window metadata as untrusted geometry and fail closed on malformed target records.
- **References:** `scripts/CodexWindowSafetyChecks.swift`, `scripts/codex-window-restore.swift`, `tests/CodexWindowSafetyChecksTests.swift`, `scripts/test_swift.sh`, `AGENTS.md`, `CODEX.md`.
