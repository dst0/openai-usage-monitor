import AppKit
import Foundation

/// Test 0: the suite reaches Monitor state only in this run's private Codex home.
///
/// The menu reads settings from `accounts.json` through `CodexClient`, which uses `CODEX_HOME`,
/// or the live `~/.codex` when it is unset. A delegate's single-instance lock defaults to the live
/// `~/.codex/monitor.lock` whatever `CODEX_HOME` says. The tripwires backed by
/// tests/TestCodexHome.swift stop the run before either live path is built, so this test fails,
/// without touching `~/.codex`, if the suite would still reach it.
func runCodexHomeIsolationTests(preferences: TestPreferencesSuite) {
  // What the rest of the suite does with the shared client, in this order: read a setting, build
  // a delegate and its menu, and show a snapshot in a status item.
  _ = CodexClient.shared.getAutoSwitchEnabled()
  let delegate = makeTestAppDelegate(client: CodexClient.shared, preferences: preferences)
  let statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
  delegate.statusItem = statusItem
  delegate.statusItem?.menu = delegate.buildMenu()
  delegate.updateUI(with: codexHomeSnapshot())
  assertTrue(
    delegate.statusItem?.button?.image != nil,
    "updateUI must reach the status item, so this pass covers the status bar path too")
  NSStatusBar.system.removeStatusItem(statusItem)

  // The menu shows the settings stored in the home CODEX_HOME names. Across the passes each
  // setting takes both values, so a menu that read another file would fail one of them. The
  // business items are checked only while auto-switch is on, because the menu shows them off
  // otherwise.
  let passes: [(restart: Bool, autoSwitch: Bool, businessOnly: Bool, businessPriority: Bool,
    weeklyReset: Bool, weeklyHours: Int)] = [
    (true, true, true, false, true, 48),
    (false, true, false, true, false, 24),
    (false, false, true, true, true, 72),
  ]
  for (index, pass) in passes.enumerated() {
    let home = TestCodexHome.enter("settings", create: true)
    let settings: [String: Any] = [
      "restart_app_on_switch": pass.restart,
      "auto_switch_enabled": pass.autoSwitch,
      "auto_switch_business_only": pass.businessOnly,
      "auto_switch_business_priority": pass.businessPriority,
      "auto_reset_weekly_enabled": pass.weeklyReset,
      "auto_reset_weekly_min_remaining_seconds": pass.weeklyHours * 3600,
    ]
    let registry: [String: Any] = ["accounts": [[String: Any]](), "settings": settings]
    try! JSONSerialization.data(withJSONObject: registry).write(to: home.appendingPathComponent("accounts.json"))

    // Read the states buildMenu() sets; updateUI(with:) would replace them from a snapshot.
    let reader = makeTestAppDelegate(client: CodexClient.shared, preferences: preferences)
    let menu = reader.buildMenu()
    func state(_ on: Bool) -> NSControl.StateValue { on ? .on : .off }
    let label = "pass \(index)"
    assertEqual(
      menu.items.first(where: { $0.title == L10n.restartAppOnSwitch })?.state, state(pass.restart),
      "\(label): restart on switch must come from CODEX_HOME")
    assertEqual(reader.autoSwitchItem?.state, state(pass.autoSwitch), "\(label): auto-switch must come from CODEX_HOME")
    assertEqual(
      reader.autoSwitchBusinessOnlyItem?.state, state(pass.autoSwitch && pass.businessOnly),
      "\(label): business-only must come from CODEX_HOME")
    assertEqual(
      reader.autoSwitchBusinessPriorityItem?.state, state(pass.autoSwitch && pass.businessPriority),
      "\(label): business priority must come from CODEX_HOME")
    assertEqual(reader.autoResetWeeklyItem?.state, state(pass.weeklyReset), "\(label): weekly reset must come from CODEX_HOME")
    assertEqual(
      reader.autoResetWeeklyThresholdItems.filter { $0.state == .on }.map { $0.tag }, [pass.weeklyHours],
      "\(label): the weekly reset threshold must come from CODEX_HOME")
    TestCodexHome.leave(home)
  }
  assertEqual(
    CodexClient.codexHome.path, TestCodexHome.home.path, "Leaving a test home must restore the run's home")

  print("  ✅ Monitor settings come only from this run's private Codex home")
}

private func codexHomeSnapshot() -> MultiAccountSnapshot {
  let account = AccountQuota(
    id: "codex-home-cli", email: "codex-home@example.com", planType: "plus",
    isCurrentActive: true, fiveHourPercentage: 90, weeklyPercentage: 85, resetTime: nil,
    resetAfterSeconds: 3600, credits: 0)
  return MultiAccountSnapshot(
    timestamp: Date(), activeAccountId: account.id, activeEmail: account.email,
    activePlan: account.planType, fiveHourPercentage: 90, weeklyPercentage: 85,
    resetTime: nil, resetAfterSeconds: 3600, credits: 0,
    accounts: [account], cliAccount: account)
}
