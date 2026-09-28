import AppKit
import Foundation

public final class AppDelegate: NSObject, NSApplicationDelegate, NSMenuDelegate {
  public static let refreshIntervalKey = "codex_refresh_interval"
  public static let stackPercentagesKey = "stackPercentages"
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
  internal var desktopSessionWatcherSource: DispatchSourceFileSystemObject?
  internal var desktopSessionWatcherFD: Int32 = -1
  internal var authWatcherSource: DispatchSourceFileSystemObject?
  internal var authWatcherFD: Int32 = -1

  internal var statusUpdateWorkItem: DispatchWorkItem?
  internal var statusRestartWorkItem: DispatchWorkItem?
  internal var desktopSessionUpdateWorkItem: DispatchWorkItem?
  internal var desktopSessionRestartWorkItem: DispatchWorkItem?
  internal var desktopSessionSnapshotRefreshOverride: (() -> Void)?
  internal var desktopLifecycleObservers: [NSObjectProtocol] = []
  internal var authRefreshWorkItem: DispatchWorkItem?
  internal var authRestartWorkItem: DispatchWorkItem?
  internal var appDeactivateObserver: NSObjectProtocol?

  internal var detectedCLIVersion: String? = "0.1.0"
  internal var isCLIUpdateAvailable: Bool = false
  internal var availableCLIVersion: String? = nil

  internal var refreshInterval: TimeInterval

  /// Monitor state is read and watched only in `client.codexHome`. The app passes the shared
  /// client on the live home; tests pass a client on a temporary one.
  internal let client: CodexClient
  /// Menu preferences. The app passes the standard store; tests pass a suite of their own.
  internal let defaults: UserDefaults
  internal var quotaRefreshOverride: ((@escaping (MultiAccountSnapshot?) -> Void) -> Void)?
  /// Receives alerts instead of a modal NSAlert; tests set it, the app leaves it nil.
  internal var alertOverride: ((_ title: String, _ message: String, _ style: NSAlert.Style) -> Void)?
  internal let singleGuard: SingleInstanceGuard
  internal let autoLaunchManager: AutoLaunchManager
  /// Every app's on-screen windows, for saving the Desktop's bounds. `AppDelegate()` reads the
  /// live WindowServer list; tests pass their own.
  internal let desktopWindows: () -> [[String: Any]]
  /// Shows the login-item state read back from macOS in the Launch at Login item. A failed toggle
  /// shows its warning outside menu tracking.
  internal lazy var launchAtLogin = LaunchAtLoginMenuController(manager: autoLaunchManager) {
    [weak self] failure in
    self?.showAlertOutsideMenuTracking(title: L10n.launchAtLogin, message: failure.message)
  }
  internal var ownsBackgroundAutomation = false

  internal var accountsSeparatorTop: NSMenuItem?
  internal var accountsSeparatorBottom: NSMenuItem?
  internal var dynamicAccountItems: [NSMenuItem] = []
  internal var lastUpdatedMenuItem: NSMenuItem?
  internal var updateCLIItem: NSMenuItem?
  internal var stackPercentagesItem: NSMenuItem?
  /// "⚙️ Auto-Switch Settings" and the checkmark items of its submenu.
  internal var autoSwitchSettingsItem: NSMenuItem?
  internal var autoSwitchItem: NSMenuItem?
  internal var autoSwitchBusinessOnlyItem: NSMenuItem?
  internal var autoSwitchBusinessPriorityItem: NSMenuItem?
  internal var restartAppOnSwitchItem: NSMenuItem?
  internal var preserveWindowBoundsItem: NSMenuItem?
  /// Auto-Switch Settings writes not yet finished; the submenu rows stay disabled meanwhile.
  internal var pendingAutoSwitchSettingWrites = 0
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

  public override convenience init() {
    self.init(
      client: CodexClient.shared, defaults: UserDefaults.standard,
      autoLaunchManager: AutoLaunchManager.shared, desktopWindows: AppDelegate.liveDesktopWindows)
  }

  internal init(
    client: CodexClient, defaults: UserDefaults, autoLaunchManager: AutoLaunchManager,
    desktopWindows: @escaping () -> [[String: Any]]
  ) {
    self.client = client
    self.defaults = defaults
    self.autoLaunchManager = autoLaunchManager
    self.desktopWindows = desktopWindows
    self.singleGuard = SingleInstanceGuard(codexHome: client.codexHome)
    let saved = defaults.double(forKey: AppDelegate.refreshIntervalKey)
    self.refreshInterval = saved > 0 ? saved : 60.0  // 1 minute default
    super.init()
  }

  /// Whether the status bar stacks its two percentages; stacked until the user turns it off.
  internal var stacksPercentages: Bool {
    defaults.object(forKey: AppDelegate.stackPercentagesKey) as? Bool ?? true
  }

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
    startDesktopSessionFileWatcher()
    startDesktopLifecycleObservers()
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
    stopDesktopSessionFileWatcher()
    stopDesktopLifecycleObservers()
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

    let completion: (MultiAccountSnapshot?) -> Void = { [weak self] snapshot in
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
    if let quotaRefreshOverride {
      quotaRefreshOverride(completion)
    } else {
      client.refreshQuotas(completion: completion)
    }
  }
}
