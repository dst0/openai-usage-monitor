# 2026-09-17 — Status bar split bracket attachment bounds and line ascent compensation

- **Status:** Resolved
- **Task/context:** Adding dual-color split brackets (APP color top, CLI color bottom) around active account badges in the menu bar.
- **Unexpected observation or failure:** When introducing `NSTextAttachment` for dual-colored split brackets with `bounds = CGRect(x: 0, y: -5.0, width: 5.5, height: 22.0)`, Test 13.5 failed with `badgeCenter = 12.0` (expected `abs(badgeCenter - 10.5) <= 1.0`).
- **Evidence:** Test 13.5 composite image inspection showed the rightmost reserve badge shifting down from row 3..19 (center 11.0) to row 4..20 (center 12.0) because AppKit's typesetting layout computed a larger line ascender (`22.0 - 5.0 = 17.0`) compared to the standard 22x22 icon layout (`y = -7.0`).
- **Approaches tried:**
  - **Attempt 1:** Used `attachment.bounds = CGRect(x: 0, y: -5.0, width: 5.5, height: 22.0)`.
    - **Outcome:** Did not work.
    - **Why:** The -5.0 origin shifted the line's vertical baseline down in the 22pt status bar frame.
  - **Attempt 2:** Set `attachment.bounds = CGRect(x: 0, y: -7.0, width: 5.5, height: 22.0)` and baselineOffset `-0.4` inside the bracket glyph rendering.
    - **Outcome:** Worked.
    - **Why:** The `-7.0` vertical origin aligns identically with the 22x22 status bar app icon bounds, keeping line baseline at `y = 5.0` pt and resulting in an exact optical center of `11.0` pt (`minY = 3, maxY = 19`).
- **Root cause:** In AppKit attributed string rendering, full-height (22pt) attachments require `origin.y = -7.0` to preserve the standard baseline when drawn alongside text attributes and 16.5pt badge attachments.
- **Resolution:** Updated `StatusBarBracketRenderer.swift` to use `bounds = CGRect(x: 0, y: -7.0, width: width, height: height)` and `-0.4` baseline offset for the internal bracket glyph drawing.
- **Verification:** `./scripts/test_swift.sh` passed with 100% success across all screen contrast, layout, and regression tests.
- **Prevention/follow-up:** Regression test added in `AppDelegateTests.swift` (Test 17) covering single-session CLI brackets, APP brackets, dual split brackets, and distinct account selections.
- **Reusable learning:** When embedding a full-height (22pt) `NSTextAttachment` into the macOS menu bar attributed string, use `bounds = CGRect(x: 0, y: -7.0, width: w, height: 22.0)` to maintain identical line ascender and avoid shifting adjacent attachments vertically.
- **References:** `Sources/StatusBarBracketRenderer.swift`, `tests/AppDelegateTests.swift`.
