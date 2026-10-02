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
  checkUnsavedSettingSaysSo(preferences: preferences)
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
  let client = home.client()
  let delegate = makeTestAppDelegate(client: client, preferences: preferences)
  let menu = delegate.buildMenu()
  assertEqual(marks(of: delegate), [.off, .off, .off, .off, .on], "Without a registry only window bounds is on")

  // A change made outside the menu, with `cxi config`, shows once the menu opens.
  home.writeJSON(
    "accounts.json",
    ["settings": [
      "auto_switch_enabled": true, "auto_switch_business_priority": true,
      "restart_app_on_switch": true, "preserve_window_bounds_on_restart": false,
    ]])
  delegate.menuWillOpen(menu)
  assertEqual(marks(of: delegate), [.on, .on, .off, .on, .off], "Opening the menu must show the registry")

  // The business modes only shape automatic switching, so they show off while it is off.
  home.writeJSON(
    "accounts.json",
    ["settings": [
      "auto_switch_enabled": false, "auto_switch_business_only": true,
      "auto_switch_business_priority": true,
    ]])
  delegate.menuWillOpen(menu)
  assertEqual(
    marks(of: delegate), [.off, .off, .off, .off, .on], "Business modes must show off with auto-switch off")

  // Only a JSON boolean counts. serde rejects 1 or "true" for a bool, and with it the whole
  // registry, so the daemon would not be switching; the Rust defaults show instead.
  home.writeJSON(
    "accounts.json",
    ["settings": [
      "auto_switch_enabled": 1, "auto_switch_business_only": true,
      "restart_app_on_switch": "true", "preserve_window_bounds_on_restart": 0,
    ]])
  let loose = client.getAutoSwitchSettings()
  assertTrue(!loose.autoSwitchEnabled, "The number 1 must not turn auto-switch on")
  assertTrue(!loose.restartAppOnSwitch, "The string \"true\" must not turn restart on")
  assertTrue(loose.preserveWindowBoundsOnRestart, "The number 0 must leave the default window bounds")
  delegate.menuWillOpen(menu)
  assertEqual(marks(of: delegate), [.off, .off, .off, .off, .on], "Non-boolean values must show the defaults")

  // A status update re-reads the registry; the status cache no longer carries these settings.
  home.writeJSON(
    "accounts.json", ["settings": ["auto_switch_enabled": true, "auto_switch_business_only": true]])
  delegate.updateUI(
    with: MultiAccountSnapshot(
      timestamp: Date(), activeAccountId: nil, activeEmail: nil, activePlan: nil,
      fiveHourPercentage: 50, weeklyPercentage: nil, resetTime: nil, resetAfterSeconds: nil,
      credits: 0))
  assertEqual(marks(of: delegate), [.on, .off, .on, .off, .on], "A status update must show the registry")
  home.tearDown()
}

private func checkTogglesSaveThroughTheCLI(preferences: TestPreferencesSuite) {
  let home = TestCodexHome(purpose: "auto-switch-save")
  let cli = makeFakeConfigCLI(in: home)
  // makeTestAppDelegate fails the run on any alert: every write here saves what it asked for.
  let delegate = makeTestAppDelegate(
    client: home.client(cliExecutable: { cli }), preferences: preferences)
  let menu = delegate.buildMenu()
  guard let bounds = delegate.preserveWindowBoundsItem,
    let businessOnly = delegate.autoSwitchBusinessOnlyItem
  else {
    assertTrue(false, "The Auto-Switch Settings rows must exist")
    return
  }
  assertEqual(bounds.state, .on, "Window bounds must start kept")

  // The fake CLI saves what the Rust core would: the registry this test prepared.
  home.writeJSON("next-accounts.json", ["settings": ["preserve_window_bounds_on_restart": false]])
  choose(bounds)
  assertEqual(bounds.state, .on, "A choice must not move the mark before the registry changes")
  for row in rows(of: delegate) {
    assertTrue(row?.isEnabled == false, "Every row must wait for the pending write")
  }
  waitForWrites(of: delegate)
  assertEqual(bounds.state, .off, "The mark must show the saved setting")
  assertEqual(
    configLog(home), "config --preserve-window-bounds false\n",
    "Turning window bounds off must run config once with --preserve-window-bounds false")

  // Business-only is saved while auto-switch is off, so its mark is off and a click asks to
  // turn it on; the Rust core turns auto-switch on with it.
  home.writeJSON(
    "accounts.json",
    ["settings": [
      "auto_switch_business_only": true, "preserve_window_bounds_on_restart": false,
    ]])
  delegate.menuWillOpen(menu)
  assertEqual(businessOnly.state, .off, "Business-only must show off while auto-switch is off")
  home.writeJSON(
    "next-accounts.json",
    ["settings": [
      "auto_switch_enabled": true, "auto_switch_business_only": true,
      "preserve_window_bounds_on_restart": false,
    ]])
  choose(businessOnly)
  waitForWrites(of: delegate)
  assertEqual(
    configLog(home),
    "config --preserve-window-bounds false\nconfig --auto-switch-business-only true\n",
    "A business-only mark that shows off must ask to turn business-only on")
  assertEqual(marks(of: delegate), [.on, .off, .on, .off, .off], "The marks must show what was saved")

  // A CLI that fails after it saved the registry (its status cache sync, say) still saved it.
  home.write("exit-status", Data("3\n".utf8))
  home.writeJSON(
    "next-accounts.json",
    ["settings": [
      "auto_switch_enabled": true, "auto_switch_business_only": true,
      "restart_app_on_switch": true, "preserve_window_bounds_on_restart": false,
    ]])
  choose(delegate.restartAppOnSwitchItem)
  waitForWrites(of: delegate)
  assertEqual(
    delegate.restartAppOnSwitchItem?.state, .on, "A saved setting must show on whatever the exit status")
  // An alert waits for the run loop's default mode; give any that was scheduled time to fail.
  RunLoop.current.run(until: Date().addingTimeInterval(0.2))
  home.tearDown()
}

private func checkUnsavedSettingSaysSo(preferences: TestPreferencesSuite) {
  // This CLI succeeds but saves nothing, so no row's request reaches the registry.
  let home = TestCodexHome(purpose: "auto-switch-unsaved")
  let cli = makeFakeConfigCLI(in: home)
  let delegate = makeTestAppDelegate(
    client: home.client(cliExecutable: { cli }), preferences: preferences)
  var alerts: [(title: String, message: String, style: NSAlert.Style)] = []
  delegate.alertOverride = { title, message, style in alerts.append((title, message, style)) }
  _ = delegate.buildMenu()
  let titles = [
    L10n.autoSwitchOnLimit, L10n.autoSwitchBusinessPriority, L10n.autoSwitchBusinessOnly,
    L10n.restartAppOnSwitch, L10n.preserveWindowBoundsOnRestart,
  ]
  let flags = [
    "--auto-switch-enabled true", "--auto-switch-business-priority true",
    "--auto-switch-business-only true", "--restart-app-on-switch true",
    "--preserve-window-bounds false",
  ]
  for (index, row) in rows(of: delegate).enumerated() {
    choose(row)
    waitForWrites(of: delegate)
    waitUntil("An unsaved \(titles[index]) must say so", timeout: 10) { alerts.count == index + 1 }
    assertEqual(alerts[index].title, titles[index], "The alert must name the setting")
    assertEqual(alerts[index].message, L10n.settingSaveFailed, "The alert must say it was not saved")
    assertTrue(alerts[index].style == .warning, "An unsaved setting must be a warning")
  }
  assertEqual(
    marks(of: delegate), [.off, .off, .off, .off, .on], "Unsaved requests must leave every mark as saved")
  assertEqual(
    configLog(home), flags.map { "config \($0)\n" }.joined(),
    "Each row must run its own config flag with the opposite of its mark")
  home.tearDown()

  // Without a CLI nothing is saved either.
  let missing = TestCodexHome(purpose: "auto-switch-no-cli")
  let noCLI = makeTestAppDelegate(client: missing.client(), preferences: preferences)
  var missingAlerts: [String] = []
  noCLI.alertOverride = { title, _, _ in missingAlerts.append(title) }
  _ = noCLI.buildMenu()
  choose(noCLI.preserveWindowBoundsItem)
  waitForWrites(of: noCLI)
  waitUntil("A missing CLI must say the setting was not saved", timeout: 10) {
    !missingAlerts.isEmpty
  }
  assertEqual(missingAlerts, [L10n.preserveWindowBoundsOnRestart], "One alert must name the setting")
  assertEqual(noCLI.preserveWindowBoundsItem?.state, .on, "Without a CLI the mark must keep the saved value")
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
  let record: (Bool) -> Void = { saved in
    assertTrue(Thread.isMainThread, "A config write must report on the main thread")
    finished.append(saved)
  }
  client.setRestartAppOnSwitch(true, completion: record)
  client.setRestartAppOnSwitch(false, completion: record)
  waitUntil("Both config writes must finish", timeout: 10) { finished.count == 2 }
  assertEqual(finished, [true, true], "Both config writes must succeed")
  assertEqual(
    configLog(home),
    "config --restart-app-on-switch true\nconfig --restart-app-on-switch false\n",
    "A slower first write must still save before the second")
  home.tearDown()
}

/// Chooses `item` the way AppKit does: its action, sent to its target with the item. AppKit
/// sends nothing for a disabled item, so choosing one fails the test.
private func choose(_ item: NSMenuItem?) {
  guard let item, let action = item.action, let target = item.target as? NSObject else {
    assertTrue(false, "The row must exist with an action and a target")
    return
  }
  assertTrue(item.isEnabled, "\(item.title) must be enabled when chosen")
  _ = target.perform(action, with: item)
}

/// The submenu rows in order: auto-switch, business priority, business-only, restart on
/// switch, window bounds.
private func rows(of delegate: AppDelegate) -> [NSMenuItem?] {
  [
    delegate.autoSwitchItem, delegate.autoSwitchBusinessPriorityItem,
    delegate.autoSwitchBusinessOnlyItem, delegate.restartAppOnSwitchItem,
    delegate.preserveWindowBoundsItem,
  ]
}

private func marks(of delegate: AppDelegate) -> [NSControl.StateValue] {
  rows(of: delegate).map { $0?.state ?? .mixed }
}

/// Waits for every pending write to report, then checks that the rows are enabled again.
private func waitForWrites(of delegate: AppDelegate) {
  waitUntil("Every config write must finish", timeout: 10) {
    delegate.pendingAutoSwitchSettingWrites == 0
  }
  for row in rows(of: delegate) {
    assertTrue(row?.isEnabled == true, "Every row must be enabled once its write finished")
  }
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
