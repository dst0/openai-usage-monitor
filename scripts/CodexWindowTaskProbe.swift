import ApplicationServices
import Cocoa

struct WindowTaskProbeRecord: Codable {
  let process: ProcessRecord
  let window_ids: [UInt32]
  let observed_task_count: Int
}

/// kVK_ANSI_L. With Command held it must type `l`; see `copyShortcutKeyIsExpected`.
private let copyDeepLinkKeyCode: CGKeyCode = 37
private let copyDeepLinkCharacter = "l"
/// Bounds every Accessibility call so an unresponsive Desktop cannot stall
/// the probe for the default messaging timeout on every call.
private let accessibilityMessagingTimeout: Float = 1.0
private let infoPlistPath = "Contents/Info.plist"

/// The macOS side of the probe for one exact ChatGPT process.
struct SystemWindowTaskProbe: WindowTaskProbeSystem {
  let process: (pid: pid_t, birth: String)
  let app: AXUIElement
  let pasteboard = NSPasteboard.general

  init(process: (pid: pid_t, birth: String)) {
    self.process = process
    app = AXUIElementCreateApplication(process.pid)
    // A timeout set on the system-wide element applies to every element this
    // process uses, including those the shared inventory helpers create.
    AXUIElementSetMessagingTimeout(AXUIElementCreateSystemWide(), accessibilityMessagingTimeout)
  }

  func now() -> TimeInterval { ProcessInfo.processInfo.systemUptime }
  func pause(_ seconds: TimeInterval) { Thread.sleep(forTimeInterval: seconds) }
  func isOptedIn() -> Bool { argument("--allow-focus-and-clipboard") == "yes" }

  func unmetPrecondition() -> WindowTaskProbeFailure? {
    guard AXIsProcessTrusted(), CGPreflightPostEventAccess() else { return .probeAccessDenied }
    // Read through KVC so the helper still builds with SDKs older than 15.4.
    let accessBehavior: Int? = pasteboard.responds(to: NSSelectorFromString("accessBehavior"))
      ? (pasteboard.value(forKey: "accessBehavior") as? Int ?? -1) : nil
    guard pasteboardReadIsPermitted(accessBehavior: accessBehavior) else {
      return .pasteboardAccessNotAllowed
    }
    guard runningDesktopIsVerifiedBuild() else { return .desktopVersionUnverified }
    let defaults = UserDefaults.standard
    for domain in [verifiedDesktopBundleIdentifier, UserDefaults.globalDomain] {
      if keyEquivalentsConflictWithCopyShortcut(
        defaults.persistentDomain(forName: domain)?["NSUserKeyEquivalents"]
      ) { return .appShortcutConflict }
    }
    guard copyShortcutKeyIsExpected() else { return .keyboardLayoutUnsupported }
    return nil
  }

  func processBirthMatches() -> Bool { processBirth(process.pid) == process.birth }

  func windowIDs() throws -> [UInt32] { countStandardWindows(process).window_ids }

  func mappedWindows(_ ids: [UInt32]) throws -> [AXUIElement] {
    let ax = try standardWindows()
    let frames = try windowServerFrames(ids)
    guard ax.count == ids.count else { throw WindowTaskProbeFailure.windowInventoryMismatch }
    guard let mapping = uniqueWindowFrameMapping(
      ids: ids, windowServerFrames: frames, accessibilityFrames: ax.map { $0.frame }
    ) else { throw WindowTaskProbeFailure.windowMappingAmbiguous }
    return mapping.map { ax[$0].element }
  }

  func sameWindow(_ left: AXUIElement, _ right: AXUIElement) -> Bool { CFEqual(left, right) }

  func beginVisibleChanges() { VisibleChangeMarker.shared.started = true }

  func requestFocus(_ window: AXUIElement) {
    // All requests are best effort: macOS may decline or defer activation
    // (`.activateIgnoringOtherApps` has no effect from macOS 14 on), so only
    // the focus observed by `hasKeyboardFocus` authorizes the shortcut.
    _ = AXUIElementSetAttributeValue(app, kAXFrontmostAttribute as CFString, kCFBooleanTrue)
    _ = NSRunningApplication(processIdentifier: process.pid)?.activate(options: [.activateIgnoringOtherApps])
    _ = AXUIElementPerformAction(window, kAXRaiseAction as CFString)
    _ = AXUIElementSetAttributeValue(window, kAXMainAttribute as CFString, kCFBooleanTrue)
    _ = AXUIElementSetAttributeValue(app, kAXFocusedWindowAttribute as CFString, window)
  }

  /// Reads focus live from the Accessibility server. `NSWorkspace` focus
  /// state is refreshed by notifications that a helper without a run loop
  /// never processes, so it is not a trustworthy witness here.
  func hasKeyboardFocus(_ window: AXUIElement) -> Bool {
    let systemWide = AXUIElementCreateSystemWide()
    var rawApp: AnyObject?
    var focusedPID: pid_t = 0
    guard AXUIElementCopyAttributeValue(
      systemWide, kAXFocusedApplicationAttribute as CFString, &rawApp
    ) == .success, let rawApp, CFGetTypeID(rawApp) == AXUIElementGetTypeID(),
      AXUIElementGetPid(rawApp as! AXUIElement, &focusedPID) == .success,
      focusedPID == process.pid else { return false }
    guard let focused = copyAXValue(app, kAXFocusedWindowAttribute as String),
      CFGetTypeID(focused) == AXUIElementGetTypeID() else { return false }
    return CFEqual(focused, window)
  }

  /// Menus match a Command shortcut by the character the key types with
  /// Command held. On Dvorak or Colemak, key 37 types another letter, which
  /// could select a different hidden Desktop command.
  func copyShortcutKeyIsExpected() -> Bool {
    // Let any pending input-source change notification arrive first; this
    // helper has no running run loop of its own. Not verified to be needed.
    _ = CFRunLoopRunInMode(CFRunLoopMode.defaultMode, 0.01, true)
    guard let layout = currentKeyboardLayout() else { return false }
    return commandCharacter(forKeyCode: copyDeepLinkKeyCode, layout: layout) == copyDeepLinkCharacter
  }

  func postCopyShortcut() -> Bool {
    guard let down = CGEvent(keyboardEventSource: nil, virtualKey: copyDeepLinkKeyCode, keyDown: true),
      let up = CGEvent(keyboardEventSource: nil, virtualKey: copyDeepLinkKeyCode, keyDown: false)
    else { return false }
    down.flags = [.maskCommand, .maskAlternate]
    up.flags = [.maskCommand, .maskAlternate]
    // Deliver only to the verified ChatGPT process. A HID-level post would go
    // to whichever application became frontmost after the focus check.
    down.postToPid(process.pid)
    up.postToPid(process.pid)
    return true
  }

  func pasteboardChangeCount() -> Int { pasteboard.changeCount }

  func pasteboardOffersTaskText() -> Bool {
    pasteboardTypesAllowTaskRead((pasteboard.types ?? []).map { $0.rawValue })
  }

  func pasteboardString() -> String? { pasteboard.string(forType: .string) }

  /// The running Desktop's identity, build, and version must match the
  /// inspected build, and its Info.plist must be unchanged since launch: an
  /// update replaced on disk while the old code runs would otherwise pass.
  private func runningDesktopIsVerifiedBuild() -> Bool {
    guard let running = NSRunningApplication(processIdentifier: process.pid),
      let url = running.bundleURL, let bundle = Bundle(url: url) else { return false }
    let modified = (try? FileManager.default.attributesOfItem(
      atPath: url.appendingPathComponent(infoPlistPath).path))?[.modificationDate] as? Date
    return isVerifiedDesktopBuild(
      bundleIdentifier: bundle.bundleIdentifier,
      version: bundle.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String,
      buildNumber: bundle.object(forInfoDictionaryKey: "CFBundleVersion") as? String
    ) && bundleUnchangedSinceLaunch(infoModified: modified, launched: running.launchDate)
  }

  private func standardWindows() throws -> [(element: AXUIElement, frame: CGRect)] {
    var raw: AnyObject?
    guard AXUIElementCopyAttributeValue(app, kAXWindowsAttribute as CFString, &raw) == .success,
      let windows = raw as? [AXUIElement] else { throw WindowTaskProbeFailure.windowAccessFailed }
    return try windows.compactMap { window in
      var role: AnyObject?
      guard AXUIElementCopyAttributeValue(window, kAXSubroleAttribute as CFString, &role) == .success,
        let subrole = role as? String else { throw WindowTaskProbeFailure.windowAccessFailed }
      guard subrole == kAXStandardWindowSubrole as String else { return nil }
      guard let minimized = copyAXValue(window, kAXMinimizedAttribute as String),
        CFGetTypeID(minimized) == CFBooleanGetTypeID(),
        let isMinimized = minimized as? Bool else { throw WindowTaskProbeFailure.windowAccessFailed }
      // Raising a minimized window could restore it; refuse before anything
      // visible happens instead of discovering the problem mid-run.
      guard !isMinimized else { throw WindowTaskProbeFailure.windowMinimized }
      guard let point = decodeAXPoint(copyAXValue(window, kAXPositionAttribute as String)),
        let size = decodeAXSize(copyAXValue(window, kAXSizeAttribute as String)),
        size.width >= 300, size.height >= 250 else { throw WindowTaskProbeFailure.windowGeometryFailed }
      return (window, CGRect(origin: point, size: size))
    }
  }

  private func windowServerFrames(_ expected: [UInt32]) throws -> [UInt32: CGRect] {
    guard let records = CGWindowListCopyWindowInfo([.optionAll, .excludeDesktopElements], kCGNullWindowID)
      as? [[String: Any]] else { throw WindowTaskProbeFailure.windowAccessFailed }
    var frames: [UInt32: CGRect] = [:]
    for info in records {
      guard (info[kCGWindowOwnerPID as String] as? NSNumber)?.int32Value == process.pid,
        let id = (info[kCGWindowNumber as String] as? NSNumber)?.uint32Value,
        expected.contains(id) else { continue }
      guard let bounds = info[kCGWindowBounds as String] as? [String: Any],
        let x = (bounds["X"] as? NSNumber)?.doubleValue,
        let y = (bounds["Y"] as? NSNumber)?.doubleValue,
        let width = (bounds["Width"] as? NSNumber)?.doubleValue,
        let height = (bounds["Height"] as? NSNumber)?.doubleValue,
        x.isFinite, y.isFinite, width.isFinite, height.isFinite
      else { throw WindowTaskProbeFailure.windowGeometryFailed }
      guard frames.updateValue(CGRect(x: x, y: y, width: width, height: height), forKey: id) == nil
      else { throw WindowTaskProbeFailure.windowInventoryMismatch }
    }
    guard frames.count == expected.count else { throw WindowTaskProbeFailure.windowInventoryMismatch }
    return frames
  }
}

/// Explicit, opt-in diagnostic only. It focuses windows and replaces the
/// clipboard, and is never called by restart, distribution, or recovery.
/// Task IDs stay in this process's memory; the record carries only a count.
func probeSelectedTasks(_ process: (pid: pid_t, birth: String)) -> WindowTaskProbeRecord {
  let system = SystemWindowTaskProbe(process: process)
  do {
    let result = try WindowTaskProbe(system: system).run()
    return WindowTaskProbeRecord(
      process: ProcessRecord(pid: process.pid, birth_id: process.birth),
      window_ids: result.windowIDs, observed_task_count: result.taskCount)
  } catch {
    fail(((error as? WindowTaskProbeFailure) ?? .probeFailed).rawValue)
  }
}
