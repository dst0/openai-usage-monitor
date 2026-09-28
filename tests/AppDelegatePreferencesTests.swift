import AppKit
import Foundation
import ServiceManagement

/// Login-item script runner that always fails, so launch-at-login state falls back to the
/// injected store and no test asks System Events through `osascript`.
final class FailingLoginItemScripts: ScriptExecuting {
  func executeAppleScript(_ script: String) -> (exitCode: Int32, output: String) { (1, "") }
}

/// The main-app login service of a binary without a bundle: never available.
final class UnavailableMainAppService: SMAppServiceManaging {
  var isAvailable: Bool { false }
  var status: SMAppService.Status { .notFound }
  func register() throws {}
  func unregister() throws {}
}

/// Builds a delegate that reads and writes preferences only in `preferences`.
func makeTestAppDelegate(client: CodexClient, preferences: TestPreferencesSuite) -> AppDelegate {
  let store = preferences.defaults
  let scripts = FailingLoginItemScripts(), service = UnavailableMainAppService()
  let loginItems = AutoLaunchManager(scriptExecutor: scripts, smService: service, userDefaults: store)
  return AppDelegate(client: client, defaults: store, autoLaunchManager: loginItems)
}

/// Test 4: the delegate's menu preferences come from the store it was given.
///
/// The delegate used to read the test binary's standard store, which every concurrent run of
/// the suite shares, so another run's write could change what this run read. This test proves
/// the delegate reads and writes the given store; tests/swift_test_defaults_isolation.sh
/// proves no source or test names another one. (A runtime spy on
/// `UserDefaults.didChangeNotification` cannot do that: AppKit's class initializers call
/// `register(defaults:)` on the standard store, which posts the same notification.)
func runAppDelegatePreferencesTests() {
  // The menu reads Monitor settings from CODEX_HOME. Point it at a path that does not exist so
  // those reads give defaults instead of the live ~/.codex; nothing here writes there.
  let codexHome = FileManager.default.temporaryDirectory.appendingPathComponent(
    "codex-preferences-home-\(UUID().uuidString)")
  let previousCodexHome = ProcessInfo.processInfo.environment["CODEX_HOME"]
  setenv("CODEX_HOME", codexHome.path, 1)
  defer {
    if let previousCodexHome { setenv("CODEX_HOME", previousCodexHome, 1) } else { unsetenv("CODEX_HOME") }
    try? FileManager.default.removeItem(at: codexHome)
  }
  let client = CodexClient(distributionRunner: { _ in false })
  let stackKey = AppDelegate.stackPercentagesKey
  let intervalKey = AppDelegate.refreshIntervalKey
  let loginKey = AutoLaunchManager.userDefaultsKey

  // An empty store gives the documented defaults.
  let fresh = TestPreferencesSuite(purpose: "fresh")
  let freshDelegate = makeTestAppDelegate(client: client, preferences: fresh)
  _ = freshDelegate.buildMenu()
  assertTrue(freshDelegate.stacksPercentages, "An unset stackPercentages must mean stacked")
  assertEqual(freshDelegate.stackPercentagesItem?.state, .on, "Menu must show the stacked default")
  assertEqual(freshDelegate.refreshInterval, 60.0, "An unset refresh interval must be one minute")
  assertEqual(freshDelegate.launchAtLoginItem?.state, .off, "Unset launch-at-login must be off")
  assertTrue(fresh.defaults.object(forKey: stackKey) == nil, "Reading a default must not store it")
  for unusable in [0.0, -5.0] {
    fresh.defaults.set(unusable, forKey: intervalKey)
    assertEqual(
      makeTestAppDelegate(client: client, preferences: fresh).refreshInterval, 60.0,
      "A stored interval of \(unusable) must fall back to one minute")
  }
  fresh.tearDown()

  // The app's own delegate keeps the user's saved preferences and login item. These two lines
  // are the only ones tests/swift_test_defaults_isolation.sh lets a test spell this way.
  assertTrue(AppDelegate().defaults === UserDefaults.standard, "AppDelegate() must use the standard store")
  assertTrue(AppDelegate().autoLaunchManager === AutoLaunchManager.shared, "AppDelegate() must use the shared login items")

  // Both values of every preference must reach the delegate. No other store holds true and
  // false at once, so a delegate that reads any store but its own fails one of the two.
  let stored = TestPreferencesSuite(purpose: "stored")
  let snapshot = preferencesSnapshot()
  let stackedWidth = compositeWidth(snapshot: snapshot, stacked: true)
  let horizontalWidth = compositeWidth(snapshot: snapshot, stacked: false)
  assertTrue(stackedWidth != horizontalWidth, "Stacked and horizontal status items must differ in width")
  for value in [false, true] {
    stored.defaults.set(value, forKey: stackKey)
    stored.defaults.set(value, forKey: loginKey)
    stored.defaults.set(value ? 900.0 : 300.0, forKey: intervalKey)
    let delegate = makeTestAppDelegate(client: client, preferences: stored)
    _ = delegate.buildMenu()
    assertEqual(delegate.stacksPercentages, value, "stackPercentages must be read from the given store")
    assertEqual(delegate.stackPercentagesItem?.state, value ? .on : .off, "Stack menu item must follow the store")
    assertEqual(delegate.launchAtLoginItem?.state, value ? .on : .off, "Launch-at-login fallback must follow the store")
    assertEqual(delegate.refreshInterval, value ? 900.0 : 300.0, "Refresh interval must be read from the given store")

    let statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
    delegate.statusItem = statusItem
    delegate.updateStatusBar(with: snapshot)
    assertEqual(
      delegate.statusItem?.button?.image?.size.width, value ? stackedWidth : horizontalWidth,
      "updateStatusBar must lay out percentages from the given store")
    delegate.updateStatusBarDisplay(
      fiveHPct: "90%", fiveHColor: .systemGreen, weeklyPct: "85%", weeklyColor: .systemGreen,
      accounts: [])
    assertEqual(
      delegate.statusItem?.button?.image?.size.width,
      displayWidth(stacked: value),
      "updateStatusBarDisplay must lay out percentages from the given store")
    NSStatusBar.system.removeStatusItem(statusItem)
  }
  stored.tearDown()

  // The menu actions write the given store, and a new delegate restores what they wrote.
  let actions = TestPreferencesSuite(purpose: "actions")
  let acting = makeTestAppDelegate(client: client, preferences: actions)
  acting.quotaRefreshOverride = { completion in completion(nil) }
  _ = acting.buildMenu()
  acting.toggleStackPercentages()
  assertEqual(actions.defaults.object(forKey: stackKey) as? Bool, false, "First toggle must store false")
  assertEqual(acting.stackPercentagesItem?.state, .off, "First toggle must clear the menu check")
  assertTrue(!acting.stacksPercentages, "First toggle must switch to horizontal")
  let restoredLayout = makeTestAppDelegate(client: client, preferences: actions)
  assertTrue(!restoredLayout.stacksPercentages, "A new delegate must restore the stored layout")
  acting.toggleStackPercentages()
  assertEqual(actions.defaults.object(forKey: stackKey) as? Bool, true, "Second toggle must store true")
  assertEqual(acting.stackPercentagesItem?.state, .on, "Second toggle must restore the menu check")

  // With a snapshot on screen, a toggle redraws the status item in the new layout.
  let actingItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
  acting.statusItem = actingItem
  acting.lastSnapshot = snapshot
  acting.toggleStackPercentages()
  assertEqual(acting.statusItem?.button?.image?.size.width, horizontalWidth, "Toggle must redraw horizontally")
  acting.toggleStackPercentages()
  assertEqual(acting.statusItem?.button?.image?.size.width, stackedWidth, "Toggle must redraw stacked")
  NSStatusBar.system.removeStatusItem(actingItem)

  let intervalMenu = NSMenu()
  let oneMinute = NSMenuItem(title: "1m", action: nil, keyEquivalent: "")
  oneMinute.tag = 60
  let fifteenMinutes = NSMenuItem(title: "15m", action: nil, keyEquivalent: "")
  fifteenMinutes.tag = 900
  intervalMenu.addItem(oneMinute)
  intervalMenu.addItem(fifteenMinutes)
  acting.changeInterval(fifteenMinutes)
  acting.refreshTimer?.invalidate()
  assertEqual(actions.defaults.double(forKey: intervalKey), 900.0, "Interval change must be stored")
  assertEqual(acting.refreshInterval, 900.0, "Interval change must apply immediately")
  assertEqual(fifteenMinutes.state, .on, "Chosen interval must be checked")
  assertEqual(oneMinute.state, .off, "Other intervals must be unchecked")
  let restored = makeTestAppDelegate(client: client, preferences: actions)
  assertEqual(restored.refreshInterval, 900.0, "A new delegate must restore the stored interval")
  actions.tearDown()
  assertTrue(
    !FileManager.default.fileExists(atPath: codexHome.path),
    "Menu preferences must stay in the defaults store, not in Monitor settings files")

  print("  ✅ Menu preferences read and write the delegate's own store")
}

private func preferencesSnapshot() -> MultiAccountSnapshot {
  let account = AccountQuota(
    id: "preferences-cli", email: "preferences@example.com", planType: "plus",
    isCurrentActive: true, fiveHourPercentage: 90, weeklyPercentage: 85, resetTime: nil,
    resetAfterSeconds: 3600, credits: 0)
  return MultiAccountSnapshot(
    timestamp: Date(), activeAccountId: account.id, activeEmail: account.email,
    activePlan: account.planType, fiveHourPercentage: 90, weeklyPercentage: 85,
    resetTime: nil, resetAfterSeconds: 3600, credits: 0,
    accounts: [account], cliAccount: account)
}

private func compositeWidth(snapshot: MultiAccountSnapshot, stacked: Bool) -> CGFloat {
  let title = AppDelegate.buildStatusBarAttributedString(
    snapshot: snapshot, icon: nil, isScreenActive: true, useQuotaIcons: true,
    stackPercentages: stacked)
  return AppDelegate.renderCompositeImage(from: title).size.width
}

private func displayWidth(stacked: Bool) -> CGFloat {
  let title = AppDelegate.buildStatusBarAttributedString(
    icon: nil, appSession: nil,
    cliSession: (fiveHPct: "90%", fiveHColor: .systemGreen, weeklyPct: "85%", weeklyColor: .systemGreen),
    accounts: [], isScreenActive: true, useQuotaIcons: true, stackPercentages: stacked)
  return AppDelegate.renderCompositeImage(from: title).size.width
}
