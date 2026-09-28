import AppKit
import Foundation
import ServiceManagement

/// Test 4b: Launch at Login shows the login item macOS reports.
///
/// The menu used to set its checkmark to the requested state whatever enabling or disabling did,
/// never read the login item again after the menu was built, and showed a cached preference when
/// System Events could not be read. Every test here uses the in-memory fakes in
/// tests/FakeLoginItems.swift; none reaches System Events or the main-app login service.
func runLaunchAtLoginTests() {
  let fixture = InstalledBundleFixture()
  defer { fixture.remove() }
  checkStateReadsBothRegistrations(fixture)
  checkChangesAreReadBack(fixture)
  checkScriptsMatchTheInstaller()
  checkListingScriptOutput()
  checkFailureMapping()
  checkMenuController(fixture)
  checkToggleBeforeFirstRead(fixture)
  checkDelegateMenu(fixture)
  print("  ✅ Launch at Login shows the login item macOS reports")
}

/// A Monitor bundle in a temporary directory, laid out as scripts/install.sh leaves an
/// /Applications install: the bundle, a `home/Applications` link to it, and, for contrast, an
/// unrelated copy with the same name.
private final class InstalledBundleFixture {
  let root: URL
  let installed: URL
  let homeLink: URL
  let renamedLink: URL
  let otherCopy: URL
  let bundle: Bundle

  init() {
    let fileManager = FileManager.default
    root = fileManager.temporaryDirectory.appendingPathComponent(
      "codex-login-item-\(UUID().uuidString)", isDirectory: true)
    installed = root.appendingPathComponent("Applications/Codex Monitor.app", isDirectory: true)
    homeLink = root.appendingPathComponent("home/Applications/Codex Monitor.app")
    renamedLink = root.appendingPathComponent("home/Links/Monitor Shortcut.app")
    otherCopy = root.appendingPathComponent("Old/Codex Monitor.app", isDirectory: true)
    for app in [installed, otherCopy] {
      let contents = app.appendingPathComponent("Contents", isDirectory: true)
      try! fileManager.createDirectory(at: contents, withIntermediateDirectories: true)
      let info: NSDictionary = ["CFBundleDisplayName": "Codex Monitor", "CFBundleName": "Codex Monitor"]
      assertTrue(info.write(to: contents.appendingPathComponent("Info.plist"), atomically: true), "fixture Info.plist")
    }
    try! fileManager.createDirectory(
      at: homeLink.deletingLastPathComponent(), withIntermediateDirectories: true)
    try! fileManager.createSymbolicLink(at: homeLink, withDestinationURL: installed)
    try! fileManager.createDirectory(
      at: renamedLink.deletingLastPathComponent(), withIntermediateDirectories: true)
    try! fileManager.createSymbolicLink(at: renamedLink, withDestinationURL: installed)
    guard let bundle = Bundle(path: installed.path) else {
      assertTrue(false, "Fixture bundle must load")
      fatalError("unreachable")
    }
    self.bundle = bundle
  }

  /// The System Events item scripts/install.sh adds for this install.
  var installerItem: FakeSystemEventsLoginItems.Item {
    FakeSystemEventsLoginItems.Item(name: "Codex Monitor", path: installed.path)
  }

  func fakes() -> (FakeSystemEventsLoginItems, FakeMainAppLoginService, AutoLaunchManager) {
    let scripts = FakeSystemEventsLoginItems(appName: "Codex Monitor", appPath: bundle.bundlePath)
    let service = FakeMainAppLoginService()
    let manager = AutoLaunchManager(scriptExecutor: scripts, smService: service, bundle: bundle)
    return (scripts, service, manager)
  }

  func remove() {
    try? FileManager.default.removeItem(at: root)
  }
}

private typealias Item = FakeSystemEventsLoginItems.Item
private let dropbox = Item(name: "Dropbox", path: "/Applications/Dropbox.app")

private func checkStateReadsBothRegistrations(_ fixture: InstalledBundleFixture) {
  let (scripts, service, manager) = fixture.fakes()
  assertEqual(manager.appName, "Codex Monitor", "The login item name must come from the bundle")
  assertTrue(
    AutoLaunchManager.isSameFile(manager.appPath, fixture.installed.path),
    "The login item path must be the running bundle")

  // An enabled main-app service is enough, and System Events is not asked.
  service.status = .enabled
  assertEqual(manager.state, .enabled, "An enabled main-app service must read as enabled")
  assertEqual(scripts.reads, 0, "An enabled main-app service must not start osascript")

  // The item scripts/install.sh adds reads as enabled, however its path is spelled.
  service.status = .notRegistered
  scripts.items = [dropbox, fixture.installerItem]
  assertEqual(manager.state, .enabled, "The login item scripts/install.sh adds must read as enabled")
  scripts.items = [Item(name: "Codex Monitor", path: fixture.installed.path + "/")]
  assertEqual(manager.state, .enabled, "A trailing slash must not hide the login item")
  scripts.items = [Item(name: "Codex Monitor", path: fixture.homeLink.path)]
  assertEqual(manager.state, .enabled, "An item at the ~/Applications link must read as enabled")
  scripts.items = [Item(name: "Codex Monitor.app", path: fixture.installed.path)]
  assertEqual(manager.state, .enabled, "The item's name must not matter, only what it opens")
  scripts.listingNoise = "osascript: warning: stderr is merged into the listing"
  assertEqual(manager.state, .enabled, "Other lines in the listing must not hide the login item")
  scripts.items = []
  assertEqual(manager.state, .disabled, "Other lines in the listing must not read as a login item")
  scripts.listingNoise = nil
  if (try? fixture.root.resourceValues(forKeys: [.volumeSupportsCaseSensitiveNamesKey]))?
    .volumeSupportsCaseSensitiveNames == false
  {
    scripts.items = [Item(name: "Codex Monitor", path: fixture.installed.deletingLastPathComponent().path + "/codex monitor.app")]
    assertEqual(manager.state, .enabled, "On a case-insensitive volume a differently cased name must match")
  }

  // Only items named like the bundle are looked up on disk, so a link with another name, even
  // one that leads here, is not: other apps' items may sit in protected folders or on volumes
  // that do not answer.
  scripts.items = [Item(name: "Monitor Shortcut", path: fixture.renamedLink.path)]
  assertEqual(manager.state, .disabled, "An item whose name differs from the bundle's must not be looked up")

  // Items that open something else do not count, even when they share the name.
  scripts.items = [
    dropbox, Item(name: "Codex Monitor", path: fixture.otherCopy.path),
    Item(name: "Codex Monitor", path: "/no/such/Codex Monitor.app"),
  ]
  assertEqual(manager.state, .disabled, "A same-name item for another copy must not read as enabled")
  scripts.items = []
  assertEqual(manager.state, .disabled, "No login items must read as disabled")

  // A relative line would resolve against the working directory; only absolute paths count.
  let workingDirectory = FileManager.default.currentDirectoryPath
  assertTrue(
    FileManager.default.changeCurrentDirectoryPath(fixture.installed.deletingLastPathComponent().path),
    "chdir to the fixture")
  scripts.items = [Item(name: "Codex Monitor", path: "Codex Monitor.app")]
  let relative = manager.state
  assertTrue(FileManager.default.changeCurrentDirectoryPath(workingDirectory), "chdir back")
  assertEqual(relative, .disabled, "A relative login item path must not match the bundle")

  // A registration waiting for approval does not open the app at login.
  service.status = .requiresApproval
  scripts.items = []
  assertEqual(manager.state, .disabled, "An unapproved main-app registration must read as disabled")

  // Unreadable items leave the state unknown; no answer is made up.
  scripts.canRead = false
  assertEqual(manager.state, .unknown, "Unreadable login items must read as unknown")
  service.isAvailable = false
  service.status = .enabled
  assertEqual(manager.state, .unknown, "An unavailable main-app service must not be asked")
  assertTrue(scripts.unexpectedScripts.isEmpty, "Reading must send only the listing script")

  assertTrue(
    AutoLaunchManager.isSameFile("/no/such/Codex Monitor.app/", "/no/such/Codex Monitor.app"),
    "Paths that do not exist must compare without a trailing slash")
  assertTrue(
    !AutoLaunchManager.isSameFile("/no/such/Codex Monitor.app", "/no/such/Other.app"),
    "Different missing paths must differ")
  assertTrue(
    !AutoLaunchManager.isSameFile(fixture.installed.path, fixture.otherCopy.path),
    "Two copies must differ")
}

private func checkChangesAreReadBack(_ fixture: InstalledBundleFixture) {
  // Enabling through the main-app service needs no System Events item.
  var (scripts, service, manager) = fixture.fakes()
  assertEqual(manager.setEnabled(true), .enabled, "A registered main-app service must read back as enabled")
  assertEqual(service.registerCalls, 1, "Enabling must register the main-app service")
  assertEqual(scripts.writes, [], "An enabled main-app service needs no System Events item")

  // A refused or unapproved main-app registration falls back to a System Events item.
  for unapproved in [false, true] {
    (scripts, service, manager) = fixture.fakes()
    if unapproved { service.statusAfterRegister = .requiresApproval } else { service.registerFails = true }
    assertEqual(manager.setEnabled(true), .enabled, "The System Events fallback must read back as enabled")
    assertEqual(scripts.writes, ["add"], "Enabling must fall back to one System Events item")
    assertEqual(
      scripts.items, [Item(name: "Codex Monitor", path: manager.appPath)],
      "The fallback item must open this bundle")
  }

  // When nothing takes effect, the read-back says so.
  (scripts, service, manager) = fixture.fakes()
  service.isAvailable = false
  scripts.canWrite = false
  assertEqual(manager.setEnabled(true), .disabled, "A failed enable must read back as disabled")
  scripts.canRead = false
  assertEqual(manager.setEnabled(true), .unknown, "An enable that cannot be read back must be unknown")

  // An add that System Events reports as done but that left no item is not trusted.
  (scripts, service, manager) = fixture.fakes()
  service.isAvailable = false
  scripts.acceptsChangesWithoutEffect = true
  assertEqual(manager.setEnabled(true), .disabled, "The add's exit status must not stand for the result")

  // Disabling removes both registrations: the main-app service and exactly the System Events
  // items that open this bundle, whatever their name, as scripts/uninstall.sh removes by path.
  (scripts, service, manager) = fixture.fakes()
  service.status = .enabled
  let otherCopyItem = Item(name: "Codex Monitor", path: fixture.otherCopy.path)
  let renamedItem = Item(name: "Codex Monitor.app", path: fixture.installed.path + "/")
  let linkItem = Item(name: "Codex Monitor", path: fixture.homeLink.path)
  scripts.items = [dropbox, fixture.installerItem, otherCopyItem, renamedItem, linkItem, fixture.installerItem]
  assertEqual(manager.setEnabled(false), .disabled, "A full disable must read back as disabled")
  assertEqual(service.unregisterCalls, 1, "Disabling must unregister the main-app service")
  assertEqual(scripts.writes, ["remove"], "Disabling must remove the System Events items once")
  assertEqual(
    scripts.removedPaths, [[fixture.installed.path, renamedItem.path, linkItem.path]],
    "Disabling must delete each listed path that opens this bundle once, as listed")
  assertEqual(scripts.items, [dropbox, otherCopyItem], "Items for other apps and other copies must stay")

  // With no System Events item for this bundle, disabling sends no remove.
  (scripts, service, manager) = fixture.fakes()
  service.status = .enabled
  scripts.items = [dropbox]
  assertEqual(manager.setEnabled(false), .disabled, "A service-only disable must read back as disabled")
  assertEqual(scripts.writes, [], "Nothing to remove must send no remove script")

  // Either registration that stays keeps the app opening at login.
  (scripts, service, manager) = fixture.fakes()
  service.status = .enabled
  service.unregisterFails = true
  assertEqual(manager.setEnabled(false), .enabled, "A refused unregister must read back as enabled")
  (scripts, service, manager) = fixture.fakes()
  scripts.items = [fixture.installerItem]
  scripts.canWrite = false
  assertEqual(manager.setEnabled(false), .enabled, "An undeletable install item must read back as enabled")
  (scripts, service, manager) = fixture.fakes()
  scripts.items = [fixture.installerItem]
  scripts.acceptsChangesWithoutEffect = true
  assertEqual(manager.setEnabled(false), .enabled, "The remove's exit status must not stand for the result")

  // With the main-app service gone but the items unreadable, the removal is unconfirmed.
  (scripts, service, manager) = fixture.fakes()
  service.status = .enabled
  scripts.canRead = false
  assertEqual(manager.setEnabled(false), .unknown, "A disable that cannot be read back must be unknown")
  assertEqual(scripts.writes, [], "Unreadable items must not be removed blindly")
  assertTrue(scripts.unexpectedScripts.isEmpty, "Changes must send only the add and remove scripts")

  let add = AutoLaunchManager.addLoginItemScript(name: "Codex \"Q\" Monitor", path: "/Apps\\X/Codex.app")
  assertTrue(
    add.contains("{name:\"Codex \\\"Q\\\" Monitor\", path:\"/Apps\\\\X/Codex.app\", hidden:false}"),
    "The add script must escape quotes and backslashes")
  assertEqual(
    AutoLaunchManager.removeLoginItemsScript(paths: ["/A \"B\"/C\\D.app", "/E.app"]),
    "tell application \"System Events\"\n"
      + "    delete (every login item whose path is \"/A \\\"B\\\"/C\\\\D.app\")\n"
      + "    delete (every login item whose path is \"/E.app\")\n"
      + "end tell",
    "The remove script must delete each escaped path")
}

/// scripts/install.sh registers the bundle it installs, under the name the app uses, so the app
/// reads the installer's item as its own and removes it when turned off.
private func checkScriptsMatchTheInstaller() {
  let repository = URL(fileURLWithPath: #filePath).deletingLastPathComponent().deletingLastPathComponent()
  guard
    let installer = try? String(contentsOf: repository.appendingPathComponent("scripts/install.sh"), encoding: .utf8),
    let uninstaller = try? String(contentsOf: repository.appendingPathComponent("scripts/uninstall.sh"), encoding: .utf8),
    let info = NSDictionary(contentsOf: repository.appendingPathComponent("resources/Info.plist"))
  else {
    assertTrue(false, "scripts/install.sh, scripts/uninstall.sh and resources/Info.plist must be readable")
    return
  }
  let lines = installer.components(separatedBy: "\n")
  assertTrue(
    lines.contains("activate_app_bundle_staging \"${INSTALL_DIR}/${BUNDLE_NAME}\""),
    "The installer must install the bundle at ${INSTALL_DIR}/${BUNDLE_NAME}")
  let registrations = lines.filter { $0.contains("make login item") }
  assertEqual(registrations.count, 1, "The installer must register one login item")
  assertTrue(
    registrations.first?.contains("{name:\\\"${APP_NAME}\\\", path:\\\"${INSTALL_DIR}/${BUNDLE_NAME}\\\"") == true,
    "The installer's login item must open the installed bundle")
  assertTrue(lines.contains("APP_NAME=\"Codex Monitor\""), "The installer must name the item Codex Monitor")
  assertEqual(
    info["CFBundleDisplayName"] as? String, "Codex Monitor",
    "The app must look for the name the installer registers")
  // The uninstaller removes the installer's item by bundle path at either install location.
  assertTrue(
    uninstaller.contains(
      "set expectedPaths to {\"/Applications/Codex Monitor.app\", \"/Applications/Codex Monitor.app/\", "
        + "userHome & \"/Applications/Codex Monitor.app\", userHome & \"/Applications/Codex Monitor.app/\"}"),
    "The uninstaller must remove the login item at both install locations")
  // The app sends Apple events to System Events through osascript; macOS names this reason when
  // it asks the user.
  assertTrue(
    (info["NSAppleEventsUsageDescription"] as? String)?.contains("System Events") == true,
    "Info.plist must explain why the Monitor asks System Events")
}

/// The printing half of the listing, spelled out so the test runs nothing it has not read.
private let expectedPrintingScript = """
  set output to ""
  repeat with itemPathReference in itemPaths
    set itemPath to contents of itemPathReference
    if class of itemPath is text then set output to output & itemPath & linefeed
  end repeat
  return output
  """

/// The listing's AppleScript after the System Events query prints what the fake prints.
private func checkListingScriptOutput() {
  assertEqual(
    AutoLaunchManager.printLoginItemPathsScript, expectedPrintingScript,
    "The printing script must be the one this test runs")
  assertEqual(
    AutoLaunchManager.loginItemPathsScript,
    "tell application \"System Events\" to set itemPaths to path of every login item\n" + expectedPrintingScript,
    "The listing must be one System Events query followed by the printing script")
  assertEqual(
    printPaths(from: "{\"/Applications/Codex Monitor.app\", missing value, \"/Users/me/Applications/Other App.app/\"}"),
    "/Applications/Codex Monitor.app\n/Users/me/Applications/Other App.app/",
    "The printing script must print one text path per line and skip missing ones")
  assertEqual(printPaths(from: "{}"), "", "No login items must print nothing")
}

/// Runs the printing script on a literal list through `osascript`, trimmed as
/// `DefaultScriptExecutor` trims it. Only this file's literal lists and the pinned printing
/// script run, and neither addresses an application.
private func printPaths(from literalList: String) -> String {
  let process = Process()
  process.executableURL = URL(fileURLWithPath: "/usr/bin/osascript")
  process.arguments = ["-e", "set itemPaths to \(literalList)\n" + expectedPrintingScript]
  let output = Pipe()
  process.standardOutput = output
  process.standardError = FileHandle.nullDevice
  do {
    try process.run()
  } catch {
    assertTrue(false, "osascript must start: \(error)")
  }
  let data = output.fileHandleForReading.readDataToEndOfFile()
  process.waitUntilExit()
  assertEqual(process.terminationStatus, 0, "The printing script must run")
  return String(data: data, encoding: .utf8)?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
}

private func checkFailureMapping() {
  typealias Failure = LaunchAtLoginMenuController.Failure
  assertEqual(Failure.of(requested: true, readBack: .enabled), nil, "A confirmed enable is no failure")
  assertEqual(Failure.of(requested: false, readBack: .disabled), nil, "A confirmed disable is no failure")
  assertEqual(Failure.of(requested: true, readBack: .disabled), .notEnabled, "enable, read off")
  assertEqual(Failure.of(requested: false, readBack: .enabled), .notDisabled, "disable, read on")
  assertEqual(Failure.of(requested: true, readBack: .unknown), .unconfirmed, "enable, unreadable")
  assertEqual(Failure.of(requested: false, readBack: .unknown), .unconfirmed, "disable, unreadable")
  assertEqual(Failure.notEnabled.message, L10n.launchAtLoginEnableFailed, "enable failure message")
  assertEqual(Failure.notDisabled.message, L10n.launchAtLoginDisableFailed, "disable failure message")
  assertEqual(Failure.unconfirmed.message, L10n.launchAtLoginUnknown, "unconfirmed message")
  let keys = ["launch_at_login_enable_failed", "launch_at_login_disable_failed", "launch_at_login_unknown"]
  for language in [AppLanguage.en, .ru, .uk] {
    for key in keys {
      assertTrue(
        LocalizationManager.translations[language]?[key]?.isEmpty == false,
        "\(key) must be translated for \(language.rawValue)")
    }
  }
  assertTrue(
    LocalizationManager.translations[.en]?["launch_at_login_unknown"]?.contains("Login Items") == true,
    "The unknown-state text must point at Login Items")
}

private func waitForLoginItem(_ controller: LaunchAtLoginMenuController, _ message: String) {
  waitUntil(message) { !controller.isRefreshing && !controller.isChanging }
}

/// Runs every block already waiting for the main run loop's default mode, where the delegate
/// shows its alerts: blocks run in the order they were added.
private func flushDefaultModeBlocks() {
  var flushed = false
  RunLoop.main.perform(inModes: [.default]) { flushed = true }
  waitUntil("default-mode blocks ran") { flushed }
}

private func checkToggleBeforeFirstRead(_ fixture: InstalledBundleFixture) {
  let (scripts, service, manager) = fixture.fakes()
  var failures: [LaunchAtLoginMenuController.Failure] = []
  let controller = LaunchAtLoginMenuController(manager: manager) { failures.append($0) }
  let item = NSMenuItem(title: L10n.launchAtLogin, action: nil, keyEquivalent: "")
  controller.item = item
  controller.toggle()
  waitForLoginItem(controller, "toggle before any read")
  assertEqual(service.registerCalls, 1, "A toggle before the first read must ask to enable")
  assertEqual(item.state, .on, "A confirmed enable before the first read must show a checkmark")
  assertEqual(failures, [], "A confirmed enable must not be reported")
  assertTrue(scripts.unexpectedScripts.isEmpty, "Only the manager's scripts may run")
}

private func checkMenuController(_ fixture: InstalledBundleFixture) {
  let (scripts, service, manager) = fixture.fakes()
  var failures: [LaunchAtLoginMenuController.Failure] = []
  let controller = LaunchAtLoginMenuController(manager: manager) { failures.append($0) }
  let item = NSMenuItem(title: L10n.launchAtLogin, action: nil, keyEquivalent: "")
  controller.item = item
  assertEqual(item.state, .mixed, "Before the first read the item must claim neither state")
  assertTrue(controller.state == nil, "Nothing is read before the first refresh")

  // Each read shows what macOS reports now.
  scripts.items = [fixture.installerItem]
  controller.refresh()
  waitForLoginItem(controller, "first read")
  assertEqual(item.state, .on, "The installer's login item must show a checkmark")
  assertTrue(item.toolTip == nil && item.isEnabled, "A known state needs no explanation")
  scripts.items = []
  controller.refresh()
  waitForLoginItem(controller, "second read")
  assertEqual(item.state, .off, "A login item removed elsewhere must lose its checkmark")
  scripts.canRead = false
  controller.refresh()
  waitForLoginItem(controller, "unreadable read")
  assertEqual(item.state, .mixed, "An unreadable login item must show a dash")
  assertEqual(item.toolTip, L10n.launchAtLoginUnknown, "The dash must be explained")
  scripts.canRead = true
  controller.refresh()
  waitForLoginItem(controller, "readable again")
  assertEqual(item.state, .off, "A readable state must replace the dash")
  assertTrue(item.toolTip == nil, "The explanation must go with the dash")

  // Enabling fails: no checkmark, and the failure is reported once.
  service.registerFails = true
  scripts.canWrite = false
  controller.toggle()
  assertTrue(!item.isEnabled && controller.isChanging, "The item must be disabled while it changes")
  waitForLoginItem(controller, "failed enable")
  assertEqual(item.state, .off, "A failed enable must leave Launch at Login unchecked")
  assertEqual(failures, [.notEnabled], "A failed enable must be reported")
  assertTrue(item.isEnabled, "The item must be usable again after a change")

  // Enabling works through System Events.
  failures = []
  scripts.canWrite = true
  controller.toggle()
  waitForLoginItem(controller, "enable")
  assertEqual(item.state, .on, "A confirmed enable must show a checkmark")
  assertEqual(failures, [], "A confirmed enable must not be reported")

  // Disabling fails: the checkmark stays.
  scripts.canWrite = false
  controller.toggle()
  waitForLoginItem(controller, "failed disable")
  assertEqual(item.state, .on, "A failed disable must keep the checkmark")
  assertEqual(failures, [.notDisabled], "A failed disable must be reported")

  // Disabling cannot be confirmed: the dash, explained, and reported.
  failures = []
  scripts.canRead = false
  controller.toggle()
  waitForLoginItem(controller, "unconfirmed disable")
  assertEqual(item.state, .mixed, "An unconfirmed disable must show a dash")
  assertEqual(failures, [.unconfirmed], "An unconfirmed disable must be reported")

  // From an unknown state a toggle asks to enable, which the main-app service confirms.
  failures = []
  service.registerFails = false
  controller.toggle()
  waitForLoginItem(controller, "enable from unknown")
  assertEqual(item.state, .on, "A toggle from the dash must enable")
  assertEqual(failures, [], "A confirmed enable must not be reported")
  assertEqual(service.status, .enabled, "The main-app service must be registered")

  // While a change runs, a second toggle and a read are ignored.
  service.status = .notRegistered
  service.isAvailable = false
  scripts.canRead = true
  scripts.canWrite = true
  scripts.items = []
  controller.refresh()
  waitForLoginItem(controller, "reset to off")
  let writesBefore = scripts.writes.count
  let readsBefore = scripts.reads
  let writeGate = DispatchSemaphore(value: 0)
  scripts.writeGate = writeGate
  controller.toggle()
  controller.toggle()
  controller.refresh()
  assertTrue(!item.isEnabled, "The item must stay disabled until the change is read back")
  scripts.writeGate = nil
  writeGate.signal()
  waitForLoginItem(controller, "gated enable")
  assertEqual(scripts.writes.count, writesBefore + 1, "A second toggle during a change must be ignored")
  assertEqual(scripts.reads, readsBefore + 1, "Only the change's own read-back may run")
  assertEqual(item.state, .on, "The gated enable must show a checkmark")

  // A read that started before a toggle must not show its older answer, even briefly.
  scripts.items = []
  controller.refresh()
  waitForLoginItem(controller, "off before stale read")
  assertEqual(item.state, .off, "stale-read setup")
  scripts.items = [fixture.installerItem]
  let readGate = DispatchSemaphore(value: 0)
  let staleWriteGate = DispatchSemaphore(value: 0)
  scripts.readGate = readGate
  controller.refresh()
  scripts.writeGate = staleWriteGate
  controller.toggle()
  scripts.readGate = nil
  readGate.signal()
  // The read's result reaches the main queue, which ends the read, while the toggle still waits.
  waitUntil("stale read delivered") { !controller.isRefreshing }
  assertEqual(item.state, .off, "A read that started before a toggle must not replace what it shows")
  assertTrue(!item.isEnabled, "The toggle is still running")
  scripts.writeGate = nil
  staleWriteGate.signal()
  waitForLoginItem(controller, "toggle after stale read")
  assertEqual(item.state, .on, "The toggle's own read-back must be shown")

  // Requests during a read are merged into one more read, which sees later changes.
  scripts.items = []
  let coalesceGate = DispatchSemaphore(value: 0)
  scripts.readGate = coalesceGate
  let readsBeforeMerge = scripts.reads
  controller.refresh()
  // The fake takes a listing's answer before it waits, so change the items only after that.
  waitUntil("first merged read started") { scripts.reads == readsBeforeMerge + 1 }
  controller.refresh()
  controller.refresh()
  scripts.items = [fixture.installerItem]
  scripts.readGate = nil
  coalesceGate.signal()
  waitForLoginItem(controller, "merged reads")
  assertEqual(scripts.reads, readsBeforeMerge + 2, "Requests during a read must add one read")
  assertEqual(item.state, .on, "The extra read must show the change made during the first")
  assertTrue(scripts.unexpectedScripts.isEmpty, "The controller must send only the manager's scripts")
}

private func checkDelegateMenu(_ fixture: InstalledBundleFixture) {
  // The menu reads Monitor settings from CODEX_HOME; give it a private home that does not exist.
  let codexHome = TestCodexHome.enter("launch-at-login", create: false)
  defer { TestCodexHome.leave(codexHome) }
  let preferences = TestPreferencesSuite(purpose: "launch-at-login")
  defer { preferences.tearDown() }
  // Earlier versions showed this stored preference when System Events could not be read.
  preferences.defaults.set(true, forKey: "CodexMonitorLaunchAtLogin")
  let (scripts, service, manager) = fixture.fakes()
  let client = CodexClient(distributionRunner: { _ in false })
  let delegate = AppDelegate(client: client, defaults: preferences.defaults, autoLaunchManager: manager)
  var alerts: [String] = []
  delegate.alertOverride = { title, message, style in
    alerts.append("\(title)|\(message)|\(style == .warning ? "warning" : "other")")
  }

  let menu = delegate.buildMenu()
  assertTrue(menu.delegate === delegate, "The status menu must tell the delegate when it opens")
  guard let item = delegate.launchAtLogin.item else {
    assertTrue(false, "The menu must have a Launch at Login item")
    return
  }
  assertTrue(menu.items.contains(item), "The Launch at Login item must be in the status menu")
  assertTrue(
    item.action == #selector(AppDelegate.toggleLaunchAtLogin) && item.target === delegate,
    "The Launch at Login item must toggle through the delegate")
  waitForLoginItem(delegate.launchAtLogin, "menu build read")
  assertEqual(item.state, .off, "A stored preference must not produce a checkmark")

  // Opening the menu reads the login item again.
  service.status = .enabled
  menu.delegate?.menuWillOpen?(menu)
  waitForLoginItem(delegate.launchAtLogin, "menu open read")
  assertEqual(item.state, .on, "Opening the menu must show a login item added elsewhere")
  service.status = .notRegistered
  menu.delegate?.menuWillOpen?(menu)
  waitForLoginItem(delegate.launchAtLogin, "second menu open read")
  assertEqual(item.state, .off, "Opening the menu must show a login item removed elsewhere")

  // The menu action: a failed enable stays unchecked and shows one warning.
  service.registerFails = true
  scripts.canWrite = false
  delegate.toggleLaunchAtLogin()
  waitForLoginItem(delegate.launchAtLogin, "menu failed enable")
  flushDefaultModeBlocks()
  assertEqual(item.state, .off, "A failed enable from the menu must stay unchecked")
  assertEqual(
    alerts, ["\(L10n.launchAtLogin)|\(L10n.launchAtLoginEnableFailed)|warning"],
    "A failed enable must show one warning")

  // A confirmed enable shows no alert; a failed disable keeps the checkmark and warns.
  alerts = []
  service.registerFails = false
  delegate.toggleLaunchAtLogin()
  waitForLoginItem(delegate.launchAtLogin, "menu enable")
  flushDefaultModeBlocks()
  assertEqual(item.state, .on, "A confirmed enable from the menu must be checked")
  assertEqual(alerts, [], "A confirmed enable must not show an alert")
  service.unregisterFails = true
  delegate.toggleLaunchAtLogin()
  waitForLoginItem(delegate.launchAtLogin, "menu failed disable")
  flushDefaultModeBlocks()
  assertEqual(item.state, .on, "A failed disable from the menu must stay checked")
  assertEqual(
    alerts, ["\(L10n.launchAtLogin)|\(L10n.launchAtLoginDisableFailed)|warning"],
    "A failed disable must show one warning")

  // A failure is shown after the current pass, in the run loop's default mode, never while the
  // menu that was reopened is still tracking.
  alerts = []
  delegate.launchAtLogin.reportFailure(.unconfirmed)
  assertEqual(alerts, [], "A failure alert must wait for the run loop's default mode")
  flushDefaultModeBlocks()
  assertEqual(
    alerts, ["\(L10n.launchAtLogin)|\(L10n.launchAtLoginUnknown)|warning"],
    "An unconfirmed change must show one warning")

  // A rebuilt menu shows the last state read at once, then reads again.
  let rebuilt = delegate.buildMenu()
  guard let rebuiltItem = delegate.launchAtLogin.item, rebuiltItem !== item else {
    assertTrue(false, "A rebuilt menu must have its own Launch at Login item")
    return
  }
  assertTrue(rebuilt.items.contains(rebuiltItem), "The rebuilt menu must hold the new item")
  assertEqual(rebuiltItem.state, .on, "A rebuilt menu must show the last state read at once")
  waitForLoginItem(delegate.launchAtLogin, "rebuilt menu read")
  assertEqual(rebuiltItem.state, .on, "The rebuilt menu's read must agree")
  assertTrue(scripts.unexpectedScripts.isEmpty, "The menu must send only the manager's scripts")
}
