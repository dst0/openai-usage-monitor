import AppKit
import Foundation

/// Test 5: the delegate reads, watches, and locks Monitor state only in its client's Codex home.
///
/// The suite used to build menus with `CodexClient.shared`, which read the developer's live
/// `~/.codex`, and every `AppDelegate` created `$HOME/.codex` when it was missing. Each check here
/// builds its client on a `TestCodexHome` and proves the value it sees came from that home;
/// tests/swift_test_codex_home_isolation.sh keeps every other test off the live home.
func runAppDelegateCodexHomeTests(preferences: TestPreferencesSuite) {
  checkCodexHomeResolution()
  checkRegistrySettingsComeFromTheClientHome(preferences: preferences)
  checkDelegateLeavesAMissingHomeAlone(preferences: preferences)
  checkStatusWatcherReadsTheClientHome(preferences: preferences)
  checkHelpPageComesFromTheClientHome()
  checkWindowBoundsUseTheClientHome(preferences: preferences)
  print("  ✅ Monitor state is read, watched, and locked only in the client's Codex home")
}

private func checkCodexHomeResolution() {
  let user = URL(fileURLWithPath: "/nonexistent-test-user", isDirectory: true)
  let unset = CodexClient.resolveCodexHome(environment: [:], userHome: user)
  assertEqual(unset.path, "/nonexistent-test-user/.codex", "Unset CODEX_HOME must select ~/.codex")
  assertTrue(unset.hasDirectoryPath, "The resolved home must be marked a directory")
  assertEqual(
    CodexClient.resolveCodexHome(environment: ["CODEX_HOME": ""], userHome: user), unset,
    "An empty CODEX_HOME must select ~/.codex")
  let configured = CodexClient.resolveCodexHome(
    environment: ["CODEX_HOME": "/srv/codex-home"], userHome: user)
  assertEqual(configured.path, "/srv/codex-home", "A set CODEX_HOME must be used as is")
  assertTrue(configured.hasDirectoryPath, "A configured home must be marked a directory")

  // The app's delegate, AppDelegate(), uses the shared client; it must be on the live home.
  // This is the only line tests/swift_test_codex_home_isolation.sh lets a test spell this way.
  assertTrue(CodexClient.shared.codexHome == CodexClient.liveCodexHome, "CodexClient.shared must use the live Codex home")

  let home = TestCodexHome(purpose: "resolution", created: false)
  let client = home.client()
  assertEqual(client.statusFileURL, home.file("usage-status.json"), "Status cache must be in the client home")
  assertEqual(client.authFileURL, home.file("auth.json"), "Auth file must be in the client home")
  assertEqual(client.accountsFileURL, home.file("accounts.json"), "Registry must be in the client home")
  assertEqual(
    client.desktopAppSessionURL, home.file("desktop-app-session.json"),
    "Desktop session marker must be in the client home")
  assertEqual(client.configTOMLURL, home.file("config.toml"), "Codex config must be in the client home")
  home.tearDown()
}

/// Two registries whose every setting differs, so a value read from anywhere else fails one.
private let enabledRegistry: [String: Any] = [
  "restart_app_on_switch": true, "auto_switch_enabled": true,
  "auto_switch_business_only": true, "auto_switch_business_priority": false,
  "auto_reset_weekly_enabled": true, "auto_reset_weekly_min_remaining_seconds": 172_800,
]
private let disabledRegistry: [String: Any] = [
  "restart_app_on_switch": false, "auto_switch_enabled": false,
  "auto_switch_business_only": false, "auto_switch_business_priority": true,
  "auto_reset_weekly_enabled": false, "auto_reset_weekly_min_remaining_seconds": 25_200,
]

private func checkRegistrySettingsComeFromTheClientHome(preferences: TestPreferencesSuite) {
  let home = TestCodexHome(purpose: "registry")
  let client = home.client()

  // No registry, and a registry without these keys, both mean what the Rust core's defaults
  // mean: every automation off.
  for registry in [nil, ["settings": [String: Any]()]] as [[String: Any]?] {
    if let registry { home.writeJSON("accounts.json", registry) }
    let label = registry == nil ? "without a registry" : "with no saved settings"
    assertTrue(!client.getAutoSwitchEnabled(), "Auto-switch must be off \(label)")
    assertTrue(!client.getRestartAppOnSwitch(), "Restart on switch must be off \(label)")
    assertTrue(!client.getAutoSwitchBusinessOnly(), "Business-only must be off \(label)")
    assertTrue(!client.getAutoSwitchBusinessPriority(), "Business priority must be off \(label)")
    let reset = client.getAutoResetWeeklyConfiguration()
    assertTrue(!reset.enabled && reset.minRemainingHours == 0, "Weekly reset must be off \(label)")
    let delegate = makeTestAppDelegate(client: client, preferences: preferences)
    _ = delegate.buildMenu()
    assertEqual(delegate.autoSwitchItem?.state, .off, "Menu must show auto-switch off \(label)")
  }

  for (settings, on) in [(enabledRegistry, true), (disabledRegistry, false)] {
    home.writeJSON("accounts.json", ["settings": settings])
    assertEqual(client.getRestartAppOnSwitch(), on, "Restart on switch must come from the home")
    assertEqual(client.getAutoSwitchEnabled(), on, "Auto-switch must come from the home")
    assertEqual(client.getAutoSwitchBusinessOnly(), on, "Business-only must come from the home")
    assertEqual(client.getAutoSwitchBusinessPriority(), !on, "Business priority must come from the home")
    let reset = client.getAutoResetWeeklyConfiguration()
    assertEqual(reset.enabled, on, "Weekly reset must come from the home")
    assertEqual(reset.minRemainingHours, on ? 48 : 7, "Weekly reset threshold must come from the home")

    let delegate = makeTestAppDelegate(client: client, preferences: preferences)
    let menu = delegate.buildMenu()
    let state: (Bool) -> NSControl.StateValue = { $0 ? .on : .off }
    let restartItem = menu.items.first(where: { $0.title == L10n.restartAppOnSwitch })
    assertEqual(restartItem?.state, state(on), "Menu must show the home's restart setting")
    assertEqual(delegate.autoSwitchItem?.state, state(on), "Menu must show the home's auto-switch")
    assertEqual(
      delegate.autoSwitchBusinessOnlyItem?.state, state(on), "Menu must show the home's business-only")
    // Business priority is saved on in the second registry, but auto-switch is off there.
    assertEqual(
      delegate.autoSwitchBusinessPriorityItem?.state, .off,
      "Business priority must show off whenever it or auto-switch is off")
    assertEqual(delegate.autoResetWeeklyItem?.state, state(on), "Menu must show the home's weekly reset")
    let checkedThresholds = delegate.autoResetWeeklyThresholdItems.filter { $0.state == .on }.map(\.tag)
    assertEqual(checkedThresholds, on ? [48] : [], "Menu must check the home's weekly threshold")
    assertEqual(
      delegate.autoResetWeeklyCustomThresholdItem?.state, state(!on),
      "A threshold that is not a preset must check the custom item")
  }
  home.tearDown()
}

private func checkDelegateLeavesAMissingHomeAlone(preferences: TestPreferencesSuite) {
  let home = TestCodexHome(purpose: "missing", created: false)
  let client = home.client()
  let delegate = makeTestAppDelegate(client: client, preferences: preferences)
  assertEqual(
    delegate.singleGuard.lockPath, home.file(SingleInstanceGuard.lockFileName).path,
    "The single-instance lock must be in the client's Codex home")

  _ = delegate.buildMenu()
  assertTrue(client.loadCachedSnapshot() == nil, "A missing home must have no cached snapshot")
  delegate.startStatusFileWatcher()
  delegate.startDesktopSessionFileWatcher()
  delegate.startAuthFileWatcher()
  RunLoop.main.run(until: Date().addingTimeInterval(0.1))
  delegate.stopStatusFileWatcher()
  delegate.stopDesktopSessionFileWatcher()
  delegate.stopAuthFileWatcher()
  assertTrue(!home.exists, "Building a delegate, its menu, and its watchers must not create the home")

  // Launch taking the lock is the one step that creates a missing home.
  assertTrue(delegate.singleGuard.tryAcquire(), "Launch must take the lock in a missing home")
  let lock = home.file(SingleInstanceGuard.lockFileName).path
  let mode = (try? FileManager.default.attributesOfItem(atPath: lock))?[.posixPermissions] as? NSNumber
  assertEqual(mode?.intValue, 0o600, "The lock file must be private to the user")
  let second = SingleInstanceGuard(codexHome: home.url)
  assertTrue(!second.tryAcquire(), "A second Monitor must not take a held lock")
  delegate.singleGuard.release()
  assertTrue(second.tryAcquire(), "The lock must be free once the first Monitor releases it")
  second.release()
  home.tearDown()
}

private func checkStatusWatcherReadsTheClientHome(preferences: TestPreferencesSuite) {
  let home = TestCodexHome(purpose: "status-watcher")
  func writeStatus(_ percentage: Double) {
    home.writeJSON(
      "usage-status.json",
      ["timestamp": "2026-09-28T00:00:00Z", "five_hour_percentage": percentage, "accounts": []])
  }
  writeStatus(41)
  let delegate = makeTestAppDelegate(client: home.client(), preferences: preferences)
  let statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
  delegate.statusItem = statusItem
  delegate.startStatusFileWatcher()
  assertTrue(delegate.fileWatcherSource != nil, "The watcher must open the home's status cache")
  for percentage in [42.0, 43.0] {
    // Each replacement is a rename, like the Rust writer's, so the watcher reloads and then
    // reopens the new file before the next one.
    writeStatus(percentage)
    waitUntil("A replaced status cache must reach the menu (\(percentage)%)") {
      delegate.lastSnapshot?.fiveHourPercentage == percentage
    }
    waitUntil("The watcher must reopen the replaced status cache") {
      delegate.fileWatcherSource != nil
    }
  }
  delegate.stopStatusFileWatcher()
  NSStatusBar.system.removeStatusItem(statusItem)
  home.tearDown()
}

private func checkHelpPageComesFromTheClientHome() {
  let home = TestCodexHome(purpose: "help")
  home.write("helps.html", Data("<html></html>".utf8))
  // An executable with no Resources/helps.html beside it, so the lookup reaches the home copy.
  let executable = home.url.appendingPathComponent("Contents/MacOS/CodexMonitor", isDirectory: false).path
  assertEqual(
    HelpsDocHelper.findHelpsHTMLURL(codexHome: home.url, arguments: [executable])?.path,
    home.file("helps.html").path, "The help page copy must come from the client's Codex home")
  let localized = HelpsDocHelper.localizedHelpsHTMLURL(
    languageCode: "uk", codexHome: home.url, arguments: [executable])
  assertEqual(localized?.path, home.file("helps.html").path, "Localized help must use the same page")
  assertEqual(localized?.query, "lang=uk", "Localized help must name its language")
  home.tearDown()
}

private func checkWindowBoundsUseTheClientHome(preferences: TestPreferencesSuite) {
  let home = TestCodexHome(purpose: "window-bounds")
  let delegate = makeTestAppDelegate(client: home.client(), preferences: preferences)
  let recoveryLock = home.file("desktop-recovery.lock").path
  home.writeJSON("accounts.json", ["settings": ["preserve_window_bounds_on_restart": false]])
  delegate.saveDesktopWindowBoundsPassive(for: getpid())
  assertTrue(
    !FileManager.default.fileExists(atPath: recoveryLock),
    "Bounds preservation turned off in the home's registry must stop before the recovery lock")
  home.writeJSON("accounts.json", ["settings": [String: Any]()])
  delegate.saveDesktopWindowBoundsPassive(for: getpid())
  assertTrue(
    FileManager.default.fileExists(atPath: recoveryLock),
    "Bounds preservation must check the recovery lock in the client's Codex home")
  assertTrue(
    !FileManager.default.fileExists(atPath: home.file("desktop-window.json").path),
    "A process without a Desktop window must not save bounds")
  home.tearDown()
}
