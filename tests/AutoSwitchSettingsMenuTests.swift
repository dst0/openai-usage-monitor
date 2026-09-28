import AppKit
import Foundation

/// Test 5b: "⚙️ Auto-Switch Settings" holds every switch-time setting in one submenu, and its
/// marks show what the registry holds, never what a click asked for.
///
/// The toggles used to sit loose in the status menu, although README.md and the help page
/// already described one Auto-Switch Settings item. Each check builds its client on a
/// `TestCodexHome`; the CLI it runs is a fake in that home.
func runAutoSwitchSettingsMenuTests(preferences: TestPreferencesSuite) {
  checkSettingsLiveInOneSubmenu(preferences: preferences)
  checkMarksFollowTheRegistry(preferences: preferences)
  checkTogglesSaveThroughTheCLI(preferences: preferences)
  checkFailedSaveKeepsTheMarkAndSaysSo(preferences: preferences)
  checkConfigWritesRunInOrder()
  print("  ✅ Auto-Switch Settings submenu and its registry-backed marks verified")
}

private func checkSettingsLiveInOneSubmenu(preferences: TestPreferencesSuite) {
  let home = TestCodexHome(purpose: "auto-switch-menu", created: false)
  let delegate = makeTestAppDelegate(client: home.client(), preferences: preferences)
  let menu = delegate.buildMenu()

  let containers = menu.items.filter { $0.title == L10n.autoSwitchSettings }
  assertEqual(containers.count, 1, "The status menu must have one Auto-Switch Settings item")
  let container = containers[0]
  assertTrue(container === delegate.autoSwitchSettingsItem, "The delegate must keep that item")
  assertTrue(container.isEnabled, "Auto-Switch Settings must open its submenu")
  guard let submenu = container.submenu else {
    assertTrue(false, "Auto-Switch Settings must have a submenu")
    return
  }
  assertTrue(!submenu.autoenablesItems, "Submenu rows must stay enabled without a validator")

  // The AGY monitor's order, without its Gemini-only rows.
  let rows: [(NSMenuItem?, String, Selector)] = [
    (delegate.autoSwitchItem, L10n.autoSwitchOnLimit,
     #selector(AppDelegate.toggleAutoSwitchOnLimit(_:))),
    (delegate.autoSwitchBusinessPriorityItem, L10n.autoSwitchBusinessPriority,
     #selector(AppDelegate.toggleAutoSwitchBusinessPriority(_:))),
    (delegate.autoSwitchBusinessOnlyItem, L10n.autoSwitchBusinessOnly,
     #selector(AppDelegate.toggleAutoSwitchBusinessOnly(_:))),
    (delegate.restartAppOnSwitchItem, L10n.restartAppOnSwitch,
     #selector(AppDelegate.toggleRestartAppOnSwitch(_:))),
    (delegate.preserveWindowBoundsItem, L10n.preserveWindowBoundsOnRestart,
     #selector(AppDelegate.togglePreserveWindowBounds(_:))),
  ]
  assertEqual(submenu.items.count, rows.count, "The submenu must hold exactly the five settings")
  for (index, (stored, title, action)) in rows.enumerated() {
    let row = submenu.items[index]
    assertTrue(row === stored, "Row \(index) must be the delegate's \(title) item")
    assertEqual(row.title, title, "Row \(index) must be \(title)")
    assertTrue(row.action == action, "\(title) must run its own toggle")
    assertTrue(row.target === delegate, "\(title) must be sent to the delegate")
    assertTrue(row.isEnabled, "\(title) must be enabled")
    assertTrue(
      !menu.items.contains(where: { $0.title == title }), "\(title) must not stay in the status menu")
  }

  // One-shot actions and the credit-spending weekly reset stay where they were.
  let neighbours = [L10n.restartApp, L10n.autoSwitchSettings, L10n.autoDistributeAppCli]
    .map { title in menu.items.firstIndex(where: { $0.title == title }) ?? -1 }
  assertTrue(neighbours[0] >= 0, "Restart Codex App must stay in the status menu")
  assertEqual(neighbours[1], neighbours[0] + 1, "Auto-Switch Settings must follow Restart Codex App")
  assertEqual(neighbours[2], neighbours[1] + 1, "Auto-Distribute must follow Auto-Switch Settings")
  assertTrue(
    menu.items.contains(where: { $0.title == L10n.autoResetWeekly && $0.submenu != nil }),
    "The weekly reset submenu must stay in the status menu")
  assertTrue(!home.exists, "Building the submenu must not create the Codex home")
  home.tearDown()
}

private func checkMarksFollowTheRegistry(preferences: TestPreferencesSuite) {
  let home = TestCodexHome(purpose: "auto-switch-marks")
  let delegate = makeTestAppDelegate(client: home.client(), preferences: preferences)
  let menu = delegate.buildMenu()
  // Auto-switch, business priority, business-only, restart on switch, window bounds.
  func marks() -> [NSControl.StateValue] {
    [
      delegate.autoSwitchItem, delegate.autoSwitchBusinessPriorityItem,
      delegate.autoSwitchBusinessOnlyItem, delegate.restartAppOnSwitchItem,
      delegate.preserveWindowBoundsItem,
    ].map { $0?.state ?? .mixed }
  }
  assertEqual(marks(), [.off, .off, .off, .off, .on], "Without a registry only window bounds is on")

  // A change made outside the menu, with `cxi config`, shows once the menu opens.
  home.writeJSON(
    "accounts.json",
    ["settings": [
      "auto_switch_enabled": true, "auto_switch_business_priority": true,
      "restart_app_on_switch": true, "preserve_window_bounds_on_restart": false,
    ]])
  assertEqual(marks(), [.off, .off, .off, .off, .on], "Marks change only when the registry is read")
  delegate.menuWillOpen(menu)
  assertEqual(marks(), [.on, .on, .off, .on, .off], "Opening the menu must show the registry")

  // The business modes only shape automatic switching, so they show off while it is off.
  home.writeJSON(
    "accounts.json",
    ["settings": [
      "auto_switch_enabled": false, "auto_switch_business_only": true,
      "auto_switch_business_priority": true,
    ]])
  delegate.menuWillOpen(menu)
  assertEqual(marks(), [.off, .off, .off, .off, .on], "Business modes must show off with auto-switch off")

  // A status update re-reads the registry; the status cache no longer carries these settings.
  home.writeJSON(
    "accounts.json", ["settings": ["auto_switch_enabled": true, "auto_switch_business_only": true]])
  delegate.updateUI(
    with: MultiAccountSnapshot(
      timestamp: Date(), activeAccountId: nil, activeEmail: nil, activePlan: nil,
      fiveHourPercentage: 50, weeklyPercentage: nil, resetTime: nil, resetAfterSeconds: nil,
      credits: 0))
  assertEqual(marks(), [.on, .off, .on, .off, .on], "A status update must show the registry")
  home.tearDown()
}

private func checkTogglesSaveThroughTheCLI(preferences: TestPreferencesSuite) {
  let home = TestCodexHome(purpose: "auto-switch-save")
  let cli = makeFakeConfigCLI(in: home)
  let client = home.client(cliExecutable: { cli })
  let delegate = makeTestAppDelegate(client: client, preferences: preferences)
  _ = delegate.buildMenu()
  guard let bounds = delegate.preserveWindowBoundsItem else {
    assertTrue(false, "The window bounds row must exist")
    return
  }
  assertEqual(bounds.state, .on, "Window bounds must start kept")

  // The fake CLI saves what the Rust core would: the registry this test prepared.
  home.writeJSON("next-accounts.json", ["settings": ["preserve_window_bounds_on_restart": false]])
  choose(bounds)
  assertEqual(bounds.state, .on, "A choice must not move the mark before the registry changes")
  waitUntil("The mark must show the saved setting", timeout: 10) { bounds.state == .off }
  assertEqual(
    configLog(home), "config --preserve-window-bounds false\n",
    "Turning window bounds off must run config once with --preserve-window-bounds false")

  // Every row asks for the opposite of its mark. This CLI saves nothing, so the marks stay.
  for item in [
    delegate.autoSwitchItem, delegate.autoSwitchBusinessPriorityItem,
    delegate.autoSwitchBusinessOnlyItem, delegate.restartAppOnSwitchItem, bounds,
  ] {
    guard let item else {
      assertTrue(false, "Every Auto-Switch Settings row must exist")
      return
    }
    choose(item)
  }
  // Writes finish in order, so this one's completion comes after every toggle's.
  var barrierDone = false
  client.setRestartAppOnSwitch(false) { _ in barrierDone = true }
  waitUntil("Every config write must finish", timeout: 10) { barrierDone }
  assertEqual(
    configLog(home),
    [
      "config --preserve-window-bounds false",
      "config --auto-switch-enabled true",
      "config --auto-switch-business-priority true",
      "config --auto-switch-business-only true",
      "config --restart-app-on-switch true",
      "config --preserve-window-bounds true",
      "config --restart-app-on-switch false",
    ].map { $0 + "\n" }.joined(),
    "Each row must run its own config flag with the opposite of its mark")
  assertEqual(bounds.state, .off, "A save that changed nothing must leave the mark as saved")
  home.tearDown()
}

private func checkFailedSaveKeepsTheMarkAndSaysSo(preferences: TestPreferencesSuite) {
  let home = TestCodexHome(purpose: "auto-switch-failed")
  let cli = makeFakeConfigCLI(in: home)
  home.write("exit-status", Data("3\n".utf8))
  home.writeJSON("next-accounts.json", ["settings": ["restart_app_on_switch": true]])
  let delegate = makeTestAppDelegate(
    client: home.client(cliExecutable: { cli }), preferences: preferences)
  var alerts: [(title: String, message: String, style: NSAlert.Style)] = []
  delegate.alertOverride = { title, message, style in alerts.append((title, message, style)) }
  _ = delegate.buildMenu()
  guard let restart = delegate.restartAppOnSwitchItem else {
    assertTrue(false, "The restart-on-switch row must exist")
    return
  }
  // The CLI reports failure after it changed the registry: the mark still shows the registry.
  choose(restart)
  waitUntil("A failed save must say so", timeout: 10) { alerts.count == 1 }
  assertEqual(alerts[0].title, L10n.restartAppOnSwitch, "The alert must name the setting")
  assertEqual(alerts[0].message, L10n.settingSaveFailed, "The alert must say it was not saved")
  assertTrue(alerts[0].style == .warning, "A failed save must be a warning")
  assertEqual(restart.state, .on, "The mark must show the registry even after a failure")
  assertEqual(configLog(home), "config --restart-app-on-switch true\n", "The CLI must run once")
  home.tearDown()

  // Without a CLI nothing is saved, the mark stays, and the alert says so.
  let missing = TestCodexHome(purpose: "auto-switch-no-cli")
  let noCLI = makeTestAppDelegate(client: missing.client(), preferences: preferences)
  var missingAlerts: [String] = []
  noCLI.alertOverride = { title, _, _ in missingAlerts.append(title) }
  _ = noCLI.buildMenu()
  guard let bounds = noCLI.preserveWindowBoundsItem else {
    assertTrue(false, "The window bounds row must exist")
    return
  }
  choose(bounds)
  waitUntil("A missing CLI must say the setting was not saved", timeout: 10) {
    !missingAlerts.isEmpty
  }
  assertEqual(missingAlerts, [L10n.preserveWindowBoundsOnRestart], "One alert must name the setting")
  assertEqual(bounds.state, .on, "Without a CLI the mark must keep the saved value")
  missing.tearDown()
}

/// Two quick toggles must save in the order they were chosen. A concurrent queue let the
/// second write finish first when the first was slower.
private func checkConfigWritesRunInOrder() {
  let home = TestCodexHome(purpose: "config-order")
  let cli = makeFakeConfigCLI(in: home)
  home.write("slow-true", Data())
  let client = home.client(cliExecutable: { cli })
  var finished: [Bool] = []
  client.setRestartAppOnSwitch(true) { finished.append($0) }
  client.setRestartAppOnSwitch(false) { finished.append($0) }
  waitUntil("Both config writes must finish", timeout: 10) { finished.count == 2 }
  assertEqual(finished, [true, true], "Both config writes must succeed")
  assertEqual(
    configLog(home),
    "config --restart-app-on-switch true\nconfig --restart-app-on-switch false\n",
    "A slower first write must still save before the second")
  home.tearDown()
}

/// Chooses `item` the way AppKit does: its action, sent to its target with the item.
private func choose(_ item: NSMenuItem) {
  guard let action = item.action, let target = item.target as? NSObject else {
    assertTrue(false, "\(item.title) must have an action and a target")
    return
  }
  _ = target.perform(action, with: item)
}

/// A Monitor CLI in `home` that records each run's arguments in `cli-arguments.log`, then
/// puts `next-accounts.json` (when a test wrote one) in place as the registry, and exits with
/// the status in `exit-status` (0 without one). With `slow-true` present, a run whose last
/// argument is `true` waits a second first.
private func makeFakeConfigCLI(in home: TestCodexHome) -> URL {
  let cli = home.file("codex-mon")
  let script = """
    #!/bin/sh
    cd '\(home.url.path)' || exit 99
    case "$*" in *" true") if [ -f slow-true ]; then sleep 1; fi ;; esac
    printf '%s\\n' "$*" >> cli-arguments.log
    if [ -f next-accounts.json ]; then mv next-accounts.json accounts.json; fi
    if [ -f exit-status ]; then exit "$(cat exit-status)"; fi
    exit 0
    """
  try! Data(script.utf8).write(to: cli)
  assertEqual(chmod(cli.path, 0o700), 0, "The fake CLI must be executable")
  return cli
}

private func configLog(_ home: TestCodexHome) -> String {
  (try? String(contentsOf: home.file("cli-arguments.log"), encoding: .utf8)) ?? ""
}
