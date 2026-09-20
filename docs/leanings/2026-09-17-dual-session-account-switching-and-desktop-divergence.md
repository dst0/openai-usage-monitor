# 2026-09-17 — Dual CLI/APP session divergence, session tracking, and 300-line Swift view decomposition

- **Status:** Resolved
- **Task/context:** Implementing dual independent switching controls for CLI (`cxi switch <id> --no-restart`) and Desktop App (`cxi switch <id> --restart`), alongside colored status bar brackets (cyan for APP, green for CLI, split for both).
- **Unexpected observation or failure:**
  1. When switching CLI independently via `--no-restart`, `usage-status.json` only wrote `active_account_id`. On the next daemon cycle or status refresh, `loadCachedSnapshot()` defaulted `appAccount` to the newly switched CLI account, erroneously claiming `ChatGPT.app` switched accounts even though it was still running in memory under the previous account.
  2. Case-sensitive strict string equality `acc.id == effectiveAppId` failed when account IDs were formatted as `email:uuid` while target arguments were emails, nicknames, or different-case strings, causing brackets to disappear entirely.
  3. `AccountSectionCardView.swift` reached 297 lines, and adding dual switch buttons threatened the strict 300-line file limit invariant.
- **Evidence:** Running the initial refactor on `AccountSectionCardView.swift` produced 435 lines, violating the architectural invariant enforced by `AppDelegateTests.swift`. `loadCachedSnapshot()` inspection confirmed `appAccount` was assigned `primary` without verifying if Desktop was actually restarted.
- **Approaches tried:**
  - **Attempt 1:** In-memory tracking only in `AppDelegate`.
    - **Outcome:** Partial failure.
    - **Why:** Every background daemon update or cache reload refreshed `lastSnapshot` directly from `usage-status.json`, losing the in-memory app account identity.
  - **Attempt 2:** Persisted `desktopAppAccountId` in `~/.codex/desktop-app-session.json` (POSIX 0600) and `UserDefaults`. Decomposed `AccountSectionCardView` into `AccountSwitchButtonsView.swift` (118 lines), `AccountSectionCardView.swift` (279 lines), and `AccountSectionCardView+Tracking.swift` (163 lines). Implemented canonical case-insensitive/email ID resolution in `AppDelegate+StatusBar.swift`.
    - **Outcome:** Fully resolved.
- **Root cause:** Desktop App (`ChatGPT.app`) and CLI share `auth.json`, but when switching CLI without restarting Desktop App, their running sessions legitimately diverge. The monitor must track which account was active when Desktop was launched/restarted separately from CLI's active account.
- **Resolution:**
  - Added `desktop-app-session.json` storage and `CodexClient.SwitchTarget` (`.cli`, `.app`, `.both`).
  - Added dedicated `AccountSwitchButtonsView` providing `[> CLI]` (green) and `[🖥 APP]` (cyan) buttons with active badges (`✓ CLI`, `✓ APP`).
  - Added alignment action items in active CLI and APP blocks to easily synchronize divergent sessions.
  - Added canonical ID resolution before bracket rendering.
- **Verification:** Both Swift test suites (`ScreenContrastTests` and `AppDelegateTests`) passed with 100% success; all Swift menu files verified `<= 287 lines`.
- **Prevention/follow-up:** Test 18 added in `tests/AppDelegateTests.swift` covering dual switch button instantiation, callbacks, active state badges, canonical email matching, and single-column fallback split brackets.
- **Reusable learning:** When a daemon/subsystem manages shared on-disk credentials between an in-memory GUI app and one-shot CLI tools, decoupled session identity must be recorded at the restart boundary to prevent background monitors from assuming the GUI app reloaded uncommitted credentials.
- **References:** `Sources/CodexClient.swift`, `Sources/AccountSwitchButtonsView.swift`, `Sources/AccountSectionCardView+Tracking.swift`, `tests/AppDelegateTests.swift`.
