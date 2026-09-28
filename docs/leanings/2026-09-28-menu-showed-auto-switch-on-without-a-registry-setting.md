# 2026-09-28 — The menu showed automatic switching on for a registry without the setting

- **Status:** Resolved
- **Task/context:** Replacing the Swift test that read the live registry ([2026-09-28 — Swift tests read the live Codex home and every AppDelegate created it](2026-09-28-swift-tests-read-the-live-codex-home.md)) with checks on a fixture Codex home.
- **Unexpected observation or failure:** `CodexClient.getAutoSwitchEnabled()` returned `true` when `accounts.json` was missing or had no `auto_switch_enabled`, so `buildMenu()` checked "Auto-switch on limit". The Rust core reads the same registry as off (`Settings.auto_switch_enabled` is `#[serde(default)]`, and `Settings::default()` sets it `false`), and `AGENTS.md` says fresh and legacy registries default automatic switching to disabled. The old Test 5 read the developer's live registry and asserted only `autoSwitch == true || autoSwitch == false`, so it could not notice.
- **Evidence:** The new check in `tests/AppDelegateCodexHomeTests.swift` failed on the old default: `Assertion Failed: condition is false - Auto-switch must be off without a registry`. With the default changed to `false` it passes, for both a missing registry and one with an empty `settings` object, and the menu item shows off.
- **Approaches tried:**
  - **Attempt:** Keep the Swift default and rely on the status snapshot, which `updateUI` applies after the menu is built.
    - **Outcome:** Rejected.
    - **Why:** Until the daemon writes its first status file, the menu shows switching on while the daemon treats it as off. A user who wants it on sees no reason to click, and the toggle reads the checkmark, so a click sends `--auto-switch-enabled false`.
  - **Attempt:** Default the Swift reader to `false`, like the Rust core.
    - **Outcome:** Worked.
- **Root cause:** Confirmed. The Swift reader's default disagreed with the Rust `Settings` default and the documented policy.
- **Resolution:** `getAutoSwitchEnabled()` defaults to `false`. `CODEX.md` notes that a registry without the setting reads as off when the menu is built.
- **Verification:** `./scripts/test_swift.sh`.
- **Prevention/follow-up:** Test 5 checks every registry setting the menu reads, for a missing registry, an empty `settings` object, and two registries whose values all differ. The other readers already matched the Rust defaults.
- **Reusable learning:** A default the menu shows must match the default the Rust core applies to the same missing key, and a test that reads live state can only assert types, so it hides a wrong default.
- **References:** `Sources/CodexClient.swift`, `codex-switcher/src/models/settings.rs`, `tests/AppDelegateCodexHomeTests.swift`.
