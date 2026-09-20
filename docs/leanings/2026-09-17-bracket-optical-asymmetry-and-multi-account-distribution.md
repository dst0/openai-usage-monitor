# 2026-09-17 — Bracket optical asymmetry, color contrast, and adopting AGI CLI pattern architecture

- **Status:** Resolved
- **Task/context:** Aligning the status bar bracket implementation and color palette with the proven AGI CLI usage monitor (`agy-cli-usage-monitor`) architecture.
- **Unexpected observation or failure:**
  1. An earlier attempt used `NSTextAttachment` to render split brackets into custom raster images. This introduced baseline ascender offsets, canvas boundary clipping, and made paired brackets behave differently than text glyphs.
  2. The font weight disparity previously added (`.semibold` vs `.regular`) caused the right bracket `]` to bloom and appear 1.5x to 3x wider than `[`.
- **Evidence:**
  - In `/Users/dst/dev/agy-cli-usage-monitor/Sources/StatusBarStyle.swift`:
    - Brackets are rendered as pure text strings with attributes, using `NSColor(patternImage: patternImg)` (`makeSplitBracketColor`) for the dual-session top/bottom split.
    - `bracketFont` uses `.regular` with `width: -0.05`.
    - `rightBracketFont` uses `.medium` with `width: -0.05` to subtly compensate for right glyph optical density without blooming.
    - APP/IDE color is Royal Blue `(0.12, 0.65, 1.0)`.
    - CLI color is Spring Green `(0.35, 0.92, 0.45)`.
    - No artificial whitespace is inserted between brackets and the quota badge attachment, allowing the brackets to hug the badge cleanly.
- **Approaches tried:**
  - **Attempt 1:** Custom image attachments (`NSTextAttachment`) with CGContext clipping.
    - **Outcome:** Suboptimal; required artificial width margins and created optical discrepancies with text glyphs.
  - **Attempt 2:** Direct adoption of AGI CLI usage monitor architecture.
    - **Outcome:** Worked brilliantly.
    - **Why:** `NSColor(patternImage:)` operates natively at the AppKit CoreText rendering layer, smoothly splitting any text glyph top-to-bottom without attachments or clipping. Paired with `bracketFont` (.regular, -0.05) and `rightBracketFont` (.medium, -0.05), brackets look crisp, balanced, and elegant.
- **Root cause:** Overcomplicating split glyphs with raster image attachments instead of AppKit's native pattern-color fill mechanism.
- **Resolution:**
  - Ported `makeSplitBracketColor` and `bracketColor(for:isScreenActive:)` directly from `agy-cli-usage-monitor`.
  - Configured `bracketFont` (.regular, -0.05) and `rightBracketFont` (.medium, -0.05).
  - Adopted AGI CLI palette: APP Royal Blue `(0.12, 0.65, 1.0)` and CLI Spring Green `(0.35, 0.92, 0.45)`.
  - Removed artificial space padding so brackets hug the badge snugly.
  - Rebuilt, signed, and reinstalled `/Applications/Codex Monitor.app`.
- **Verification:**
  - `./scripts/test_swift.sh` passed 100% (ScreenContrastTests + AppDelegateTests).
  - Strict code signing passed.
  - Process PID 83109 running live.
- **Reusable learning:** When rendering two-tone or split-color typography in AppKit (such as a split glyph in macOS status bar), use `NSColor(patternImage:)` as the text `.foregroundColor` attribute rather than splitting into `NSTextAttachment` images.
- **References:** `/Users/dst/dev/agy-cli-usage-monitor`, `Sources/StatusBarBracketRenderer.swift`, `Sources/StatusBarStyle.swift`.
