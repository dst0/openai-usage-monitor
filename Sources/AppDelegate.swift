import AppKit
import Foundation

public final class AppDelegate: NSObject, NSApplicationDelegate, NSMenuDelegate {
  public static let refreshIntervalKey = "codex_refresh_interval"
  public static let defaultMenuWidth: CGFloat = 440

  public var statusItem: NSStatusItem!
  internal var refreshTimer: Timer?
  internal var lastSnapshot: MultiAccountSnapshot?
  internal var menuBarIcon: NSImage?
  internal var menuBarIconActive: NSImage?
  internal var menuBarIconInactive: NSImage?
  internal var lastKnownScreenActive: Bool = true

  internal var fileWatcherSource: DispatchSourceFileSystemObject?
  internal var fileWatcherFD: Int32 = -1
  internal var authWatcherSource: DispatchSourceFileSystemObject?
  internal var authWatcherFD: Int32 = -1

  internal var statusUpdateWorkItem: DispatchWorkItem?
  internal var statusRestartWorkItem: DispatchWorkItem?
  internal var authRefreshWorkItem: DispatchWorkItem?
  internal var authRestartWorkItem: DispatchWorkItem?
  internal var appDeactivateObserver: NSObjectProtocol?

  internal var detectedCLIVersion: String? = "0.1.0"
  internal var isCLIUpdateAvailable: Bool = false
  internal var availableCLIVersion: String? = nil

  internal var refreshInterval: TimeInterval = {
    let saved = UserDefaults.standard.double(forKey: AppDelegate.refreshIntervalKey)
    return saved > 0 ? saved : 60.0  // 1 minute default
  }()

  internal let client = CodexClient.shared
  internal let singleGuard = SingleInstanceGuard()
  internal let autoLaunchManager = AutoLaunchManager.shared
  internal var ownsBackgroundAutomation = false

  internal var accountsSeparatorTop: NSMenuItem?
  internal var accountsSeparatorBottom: NSMenuItem?
  internal var dynamicAccountItems: [NSMenuItem] = []
  internal var lastUpdatedMenuItem: NSMenuItem?
  internal var updateCLIItem: NSMenuItem?
  internal var launchAtLoginItem: NSMenuItem?
  internal var stackPercentagesItem: NSMenuItem?
  internal var autoSwitchItem: NSMenuItem?
  internal var autoSwitchBusinessOnlyItem: NSMenuItem?
  internal var autoSwitchBusinessPriorityItem: NSMenuItem?
  internal var autoResetWeeklyItem: NSMenuItem?
  internal var autoResetWeeklyStatusItem: NSMenuItem?
  internal var autoResetWeeklyThresholdItems: [NSMenuItem] = []
  internal var autoResetWeeklyCustomThresholdItem: NSMenuItem?
  internal var isRefreshing: Bool = false
  internal var refreshPendingWhileBusy: Bool = false

  internal static let timeOfDayFormatter: DateFormatter = {
    let fmt = DateFormatter()
    fmt.dateFormat = "HH:mm:ss"
    return fmt
  }()

  // MARK: - Lifecycle

  public func applicationDidFinishLaunching(_ notification: Notification) {
    if !singleGuard.tryAcquire() || SingleInstanceGuard.isAnotherInstanceRunning() {
      print("Another instance of Codex Monitor is already running.")
      NSApp.terminate(nil)
      return
    }
    ownsBackgroundAutomation = true
    client.startBackgroundAutomation()

    loadMenuBarIcon()

    statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
    statusItem.button?.imagePosition = .imageLeading

    buildMenu()
    setupScreenObservers()
    setupAppDeactivationObserver()
    startStatusFileWatcher()
    startAuthFileWatcher()
    refreshCLIVersion()

    if let snap = client.loadCachedSnapshot() {
      self.lastSnapshot = snap
      updateUI(with: snap)
    } else {
      updateStatusBarDisplay(
        fiveHPct: "100%",
        fiveHColor: MenuBarAppearanceHelper.menuBarColor(forPercentage: 100.0, isScreenActive: true),
        weeklyPct: "100%",
        weeklyColor: MenuBarAppearanceHelper.menuBarColor(forPercentage: 100.0, isScreenActive: true),
        accounts: [],
        isScreenActive: true
      )
    }

    refreshNow()
    startTimer()
  }

  public func applicationWillTerminate(_ notification: Notification) {
    if ownsBackgroundAutomation {
      client.stopBackgroundAutomation()
      ownsBackgroundAutomation = false
    }
    removeAppDeactivationObserver()
    singleGuard.release()
    stopStatusFileWatcher()
    stopAuthFileWatcher()
    refreshTimer?.invalidate()
  }

  // MARK: - Menu Bar Icon Loading

  private func loadMenuBarIcon() {
    let bundle = Bundle.main
    if let image = bundle.image(forResource: "statusbar_icon") {
      menuBarIcon = image
    } else if let iconPath = bundle.path(forResource: "statusbar_icon", ofType: "png") {
      menuBarIcon = NSImage(contentsOfFile: iconPath)
    }

    if menuBarIcon == nil {
      let searchDirs: [URL] = [
        bundle.resourceURL,
        URL(fileURLWithPath: ProcessInfo.processInfo.arguments[0]).deletingLastPathComponent()
          .appendingPathComponent("../Resources"),
        FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent(
          "dev/openai-usage-monitor/resources"),
      ].compactMap { $0 }

      for dir in searchDirs {
        let p1x = dir.appendingPathComponent("statusbar_icon.png")
        let p2x = dir.appendingPathComponent("statusbar_icon@2x.png")
        let p3x = dir.appendingPathComponent("statusbar_icon@3x.png")

        if FileManager.default.fileExists(atPath: p1x.path) {
          let image = NSImage(size: NSSize(width: 22, height: 22))
          if let rep3 = NSImageRep(contentsOf: p3x) { image.addRepresentation(rep3) }
          if let rep2 = NSImageRep(contentsOf: p2x) { image.addRepresentation(rep2) }
          if let rep1 = NSImageRep(contentsOf: p1x) { image.addRepresentation(rep1) }
          if !image.representations.isEmpty {
            menuBarIcon = image
            break
          }
        }
      }
    }

    if menuBarIcon == nil {
      let lightIcon = URL(
        fileURLWithPath: "/Applications/ChatGPT.app/Contents/Resources/icon-codex-light.png")
      if FileManager.default.fileExists(atPath: lightIcon.path) {
        menuBarIcon = NSImage(contentsOf: lightIcon)
      } else {
        let darkIcon = URL(
          fileURLWithPath: "/Applications/ChatGPT.app/Contents/Resources/icon-codex-dark-color.png")
        menuBarIcon = NSImage(contentsOf: darkIcon)
      }
    }

    if let icon = menuBarIcon {
      icon.size = NSSize(width: 22, height: 22)
      icon.isTemplate = false
      menuBarIconActive = icon
      menuBarIconInactive = icon
    }
  }

  // MARK: - Screen Parameter Observers

  private func setupScreenObservers() {
    NotificationCenter.default.addObserver(
      self, selector: #selector(handleScreenParametersChanged),
      name: NSApplication.didChangeScreenParametersNotification, object: nil)
  }

  @objc private func handleScreenParametersChanged() {
    DispatchQueue.main.async { [weak self] in
      guard let self = self, let snap = self.lastSnapshot else { return }
      self.updateStatusBar(with: snap)
    }
  }

  // MARK: - Timer & Refresh Actions

  internal func startTimer() {
    refreshTimer?.invalidate()
    refreshTimer = Timer.scheduledTimer(withTimeInterval: refreshInterval, repeats: true) { [weak self] _ in
      self?.refreshNow()
    }
  }

  @objc internal func noop() {}

  @objc internal func refreshNow() {
    guard !isRefreshing else {
      refreshPendingWhileBusy = true
      return
    }
    isRefreshing = true

    client.refreshQuotas { [weak self] snapshot in
      DispatchQueue.main.async {
        guard let self = self else { return }
        self.isRefreshing = false
        if let snap = snapshot {
          self.lastSnapshot = snap
          self.updateUI(with: snap)
        }
        if self.refreshPendingWhileBusy {
          self.refreshPendingWhileBusy = false
          self.refreshNow()
        }
      }
    }
  }

  // MARK: - Desktop App Window Tracking

  internal func setupAppDeactivationObserver() {
    appDeactivateObserver = NSWorkspace.shared.notificationCenter.addObserver(
      forName: NSWorkspace.didDeactivateApplicationNotification,
      object: nil,
      queue: .main
    ) { [weak self] note in
      guard let app = note.userInfo?[NSWorkspace.applicationUserInfoKey] as? NSRunningApplication,
        let bundleID = app.bundleIdentifier,
        bundleID == "com.openai.codex" || bundleID == "com.openai.chat"
          || app.executableURL?.lastPathComponent == "ChatGPT"
      else { return }
      self?.saveDesktopWindowBoundsPassive(for: app.processIdentifier)
    }
  }

  internal func removeAppDeactivationObserver() {
    if let obs = appDeactivateObserver {
      NSWorkspace.shared.notificationCenter.removeObserver(obs)
      appDeactivateObserver = nil
    }
  }

  internal func saveDesktopWindowBoundsPassive(for targetPID: pid_t) {
    let windowInfo =
      CGWindowListCopyWindowInfo(
        [.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID)
      as? [[String: Any]] ?? []

    for info in windowInfo {
      let ownerPID = (info[kCGWindowOwnerPID as String] as? NSNumber)?.int32Value ?? 0
      let layer = (info[kCGWindowLayer as String] as? NSNumber)?.intValue ?? -1
      let alpha = (info[kCGWindowAlpha as String] as? NSNumber)?.doubleValue ?? 0
      guard ownerPID == targetPID, layer == 0, alpha > 0 else { continue }
      guard let boundsValue = info[kCGWindowBounds as String],
        let frame = CGRect(dictionaryRepresentation: boundsValue as! CFDictionary),
        frame.width >= 300 && frame.height >= 300
      else { continue }

      let codexDir = CodexClient.codexHome
      let targetURL = codexDir.appendingPathComponent("desktop-window.json")
      let payload: [String: Any] = [
        "version": 1,
        "x": Double(frame.origin.x),
        "y": Double(frame.origin.y),
        "width": Double(frame.size.width),
        "height": Double(frame.size.height),
        "updated_at": Int(Date().timeIntervalSince1970),
      ]
      guard let data = try? JSONSerialization.data(withJSONObject: payload, options: [.prettyPrinted]) else { return }
      let tmpPath = targetURL.path + ".\(getpid()).tmp"
      FileManager.default.createFile(atPath: tmpPath, contents: data, attributes: [.posixPermissions: 0o600])
      let tmpURL = URL(fileURLWithPath: tmpPath)
      _ = try? FileManager.default.replaceItemAt(targetURL, withItemAt: tmpURL)
      chmod(targetURL.path, 0o600)
      break
    }
  }

}
