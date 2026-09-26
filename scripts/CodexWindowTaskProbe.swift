import ApplicationServices
import Cocoa

struct WindowTaskProbeRecord: Codable {
  let process: ProcessRecord
  let window_ids: [UInt32]
  let observed_task_count: Int
}

/// Same limit the Rust side enforces on the response.
private let maximumProbedWindows = 64
/// kVK_ANSI_L. ChatGPT 26.924.20706 binds its hidden `copyDeeplink` menu
/// command to CmdOrCtrl+Alt+L by default. The Rust caller refuses to run when
/// the user's keymap overrides any binding, because an override could move
/// this shortcut to a different command.
private let copyDeepLinkKeyCode: CGKeyCode = 37
private let focusTimeout: TimeInterval = 1.0
private let clipboardTimeout: TimeInterval = 1.5
private let pollInterval: TimeInterval = 0.02

private func axStandardWindows(_ pid: pid_t) -> [(element: AXUIElement, frame: CGRect)] {
  let app = AXUIElementCreateApplication(pid)
  var raw: AnyObject?
  guard AXUIElementCopyAttributeValue(app, kAXWindowsAttribute as CFString, &raw) == .success,
    let windows = raw as? [AXUIElement] else { fail("WINDOW_ACCESS_FAILED") }
  return windows.compactMap { window in
    var role: AnyObject?
    guard AXUIElementCopyAttributeValue(window, kAXSubroleAttribute as CFString, &role) == .success,
      let subrole = role as? String else { fail("WINDOW_ACCESS_FAILED") }
    guard subrole == kAXStandardWindowSubrole as String else { return nil }
    guard let minimized = copyAXValue(window, kAXMinimizedAttribute as String),
      CFGetTypeID(minimized) == CFBooleanGetTypeID(),
      let isMinimized = minimized as? Bool else { fail("WINDOW_ACCESS_FAILED") }
    // A minimized window cannot take keyboard focus, so Desktop would copy
    // another window's link. Refuse before any focus or clipboard change.
    guard !isMinimized else { fail("WINDOW_MINIMIZED") }
    guard let point = decodeAXPoint(copyAXValue(window, kAXPositionAttribute as String)),
      let size = decodeAXSize(copyAXValue(window, kAXSizeAttribute as String)),
      size.width >= 300, size.height >= 250 else { fail("WINDOW_GEOMETRY_FAILED") }
    return (window, CGRect(origin: point, size: size))
  }
}

private func windowFrameByID(_ pid: pid_t, expected: [UInt32]) -> [UInt32: CGRect] {
  guard let records = CGWindowListCopyWindowInfo([.optionAll, .excludeDesktopElements], kCGNullWindowID)
    as? [[String: Any]] else { fail("WINDOW_ACCESS_FAILED") }
  var frames: [UInt32: CGRect] = [:]
  for info in records {
    guard (info[kCGWindowOwnerPID as String] as? NSNumber)?.int32Value == pid,
      let id = (info[kCGWindowNumber as String] as? NSNumber)?.uint32Value,
      expected.contains(id) else { continue }
    guard let bounds = info[kCGWindowBounds as String] as? [String: Any],
      let x = (bounds["X"] as? NSNumber)?.doubleValue,
      let y = (bounds["Y"] as? NSNumber)?.doubleValue,
      let width = (bounds["Width"] as? NSNumber)?.doubleValue,
      let height = (bounds["Height"] as? NSNumber)?.doubleValue,
      x.isFinite, y.isFinite, width.isFinite, height.isFinite else { fail("WINDOW_GEOMETRY_FAILED") }
    guard frames.updateValue(CGRect(x: x, y: y, width: width, height: height), forKey: id) == nil
    else { fail("WINDOW_INVENTORY_MISMATCH") }
  }
  guard frames.count == expected.count else { fail("WINDOW_INVENTORY_MISMATCH") }
  return frames
}

private func mappedWindows(_ pid: pid_t, ids: [UInt32]) -> [(id: UInt32, element: AXUIElement)] {
  let ax = axStandardWindows(pid)
  let frames = windowFrameByID(pid, expected: ids)
  guard ax.count == ids.count else { fail("WINDOW_INVENTORY_MISMATCH") }
  guard let mapping = uniqueWindowFrameMapping(
    ids: ids, windowServerFrames: frames, accessibilityFrames: ax.map { $0.frame }
  ) else { fail("WINDOW_MAPPING_AMBIGUOUS") }
  return zip(ids, mapping).map { (id: $0, element: ax[$1].element) }
}

private func waitFor(_ timeout: TimeInterval, _ condition: () -> Bool) -> Bool {
  let deadline = Date().addingTimeInterval(timeout)
  while !condition() {
    guard Date() < deadline else { return false }
    Thread.sleep(forTimeInterval: pollInterval)
  }
  return true
}

/// Reads focus live from the Accessibility server. `NSWorkspace` focus state
/// is delivered by notifications that a helper without a run loop never
/// processes, so it is not a trustworthy witness here.
private func windowHasKeyboardFocus(_ app: AXUIElement, _ window: AXUIElement, pid: pid_t) -> Bool {
  var rawApp: AnyObject?
  var focusedPID: pid_t = 0
  guard AXUIElementCopyAttributeValue(
    AXUIElementCreateSystemWide(), kAXFocusedApplicationAttribute as CFString, &rawApp
  ) == .success, let rawApp, CFGetTypeID(rawApp) == AXUIElementGetTypeID(),
    AXUIElementGetPid(rawApp as! AXUIElement, &focusedPID) == .success,
    focusedPID == pid else { return false }
  guard let focused = copyAXValue(app, kAXFocusedWindowAttribute as String),
    CFGetTypeID(focused) == AXUIElementGetTypeID() else { return false }
  return CFEqual(focused, window)
}

private func focusExactWindow(_ app: AXUIElement, window: AXUIElement, pid: pid_t) {
  // Activation calls are requests that macOS may decline or defer; only the
  // observed focus below authorizes the shortcut.
  _ = AXUIElementSetAttributeValue(app, kAXFrontmostAttribute as CFString, kCFBooleanTrue)
  _ = NSRunningApplication(processIdentifier: pid)?.activate(options: [.activateIgnoringOtherApps])
  guard AXUIElementPerformAction(window, kAXRaiseAction as CFString) == .success else {
    fail("WINDOW_FOCUS_FAILED")
  }
  _ = AXUIElementSetAttributeValue(window, kAXMainAttribute as CFString, kCFBooleanTrue)
  _ = AXUIElementSetAttributeValue(app, kAXFocusedWindowAttribute as CFString, window)
  guard waitFor(focusTimeout, { windowHasKeyboardFocus(app, window, pid: pid) }) else {
    fail("WINDOW_FOCUS_FAILED")
  }
}

private func postCopyDeepLinkShortcut(to pid: pid_t) {
  guard let down = CGEvent(keyboardEventSource: nil, virtualKey: copyDeepLinkKeyCode, keyDown: true),
    let up = CGEvent(keyboardEventSource: nil, virtualKey: copyDeepLinkKeyCode, keyDown: false) else {
    fail("COPY_LINK_EVENT_FAILED")
  }
  down.flags = [.maskCommand, .maskAlternate]
  up.flags = [.maskCommand, .maskAlternate]
  // Deliver only to the verified ChatGPT process. A HID-level post would go
  // to whichever application became frontmost after the focus check.
  down.postToPid(pid)
  up.postToPid(pid)
}

private func copySelectedTaskLink(_ pasteboard: NSPasteboard, pid: pid_t) -> String {
  let before = pasteboard.changeCount
  postCopyDeepLinkShortcut(to: pid)
  // A copy may clear the pasteboard before writing its text; wait for both.
  _ = waitFor(clipboardTimeout) {
    pasteboard.changeCount != before && pasteboard.string(forType: .string) != nil
  }
  let after = pasteboard.changeCount
  let link = pasteboard.string(forType: .string)
  let confirmed = pasteboard.changeCount
  guard let task = validateCopiedTaskLink(
    link, beforeChange: before, afterChange: after, confirmedChange: confirmed
  ) else { fail("COPY_LINK_AMBIGUOUS") }
  return task
}

/// Explicit, opt-in diagnostic only. It focuses windows and replaces the
/// clipboard, and is never called by restart, distribution, or recovery.
/// Task IDs stay in this process; the record carries only a count.
func probeSelectedTasks(_ process: (pid: pid_t, birth: String)) -> WindowTaskProbeRecord {
  guard argument("--allow-focus-and-clipboard") == "yes" else { fail("EXPLICIT_OPT_IN_REQUIRED") }
  // Without these permissions the probe would raise windows and then fail at
  // the first shortcut, so check them before any visible change.
  guard AXIsProcessTrusted(), CGPreflightPostEventAccess() else { fail("PROBE_ACCESS_DENIED") }
  let before = countStandardWindows(process)
  guard !before.window_ids.isEmpty else { fail("WINDOW_NOT_FOUND") }
  guard before.window_ids.count <= maximumProbedWindows else { fail("WINDOW_LIMIT_EXCEEDED") }
  let windows = mappedWindows(process.pid, ids: before.window_ids)
  let app = AXUIElementCreateApplication(process.pid)
  let pasteboard = NSPasteboard.general
  var taskIDs = Set<String>()
  for (_, window) in windows {
    guard processBirth(process.pid) == process.birth else { fail("PROCESS_IDENTITY_REJECTED") }
    focusExactWindow(app, window: window, pid: process.pid)
    guard processBirth(process.pid) == process.birth else { fail("PROCESS_IDENTITY_REJECTED") }
    guard windowHasKeyboardFocus(app, window, pid: process.pid) else { fail("WINDOW_FOCUS_FAILED") }
    let task = copySelectedTaskLink(pasteboard, pid: process.pid)
    guard taskIDs.insert(task).inserted else { fail("TASK_LINK_DUPLICATE") }
    let remapped = mappedWindows(process.pid, ids: before.window_ids)
    let sameMapping = remapped.count == windows.count && zip(remapped, windows).allSatisfy {
      $0.0.id == $0.1.id && CFEqual($0.0.element, $0.1.element)
    }
    guard sameMapping, windowHasKeyboardFocus(app, window, pid: process.pid),
      processBirth(process.pid) == process.birth,
      countStandardWindows(process).window_ids == before.window_ids else {
      fail("WINDOW_MAPPING_CHANGED")
    }
  }
  return WindowTaskProbeRecord(
    process: ProcessRecord(pid: process.pid, birth_id: process.birth),
    window_ids: before.window_ids, observed_task_count: taskIDs.count)
}
