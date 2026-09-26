import ApplicationServices
import Cocoa

struct WindowTaskProbeRecord: Codable {
  let process: ProcessRecord
  let window_ids: [UInt32]
  let observed_task_count: Int
}

private func axStandardWindows(_ pid: pid_t) -> [(AXUIElement, CGRect)] {
  let app = AXUIElementCreateApplication(pid)
  var raw: AnyObject?
  guard AXUIElementCopyAttributeValue(app, kAXWindowsAttribute as CFString, &raw) == .success,
    let windows = raw as? [AXUIElement] else { fail("WINDOW_ACCESS_FAILED") }
  return windows.compactMap { window in
    var role: AnyObject?
    guard AXUIElementCopyAttributeValue(window, kAXSubroleAttribute as CFString, &role) == .success,
      let subrole = role as? String else { fail("WINDOW_ACCESS_FAILED") }
    guard subrole == kAXStandardWindowSubrole as String else { return nil }
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

private func mappedWindows(_ process: (pid: pid_t, birth: String), ids: [UInt32]) -> [(UInt32, AXUIElement)] {
  let ax = axStandardWindows(process.pid)
  let frames = windowFrameByID(process.pid, expected: ids)
  guard ax.count == ids.count else { fail("WINDOW_INVENTORY_MISMATCH") }
  var result: [(UInt32, AXUIElement)] = []
  for id in ids {
    guard let frame = frames[id] else { fail("WINDOW_INVENTORY_MISMATCH") }
    let matches = ax.filter { framesMatch($0.1, frame) }
    guard matches.count == 1 else { fail("WINDOW_MAPPING_AMBIGUOUS") }
    result.append((id, matches[0].0))
  }
  let unique = Set(result.map { Unmanaged.passUnretained($0.1).toOpaque() })
  guard unique.count == ids.count else { fail("WINDOW_MAPPING_AMBIGUOUS") }
  return result
}

private func focusExactWindow(_ app: AXUIElement, window: AXUIElement, pid: pid_t) {
  guard let running = NSRunningApplication(processIdentifier: pid),
    running.activate(options: [.activateIgnoringOtherApps]) else { fail("WINDOW_FOCUS_FAILED") }
  guard AXUIElementPerformAction(window, kAXRaiseAction as CFString) == .success,
    AXUIElementSetAttributeValue(app, kAXFocusedWindowAttribute as CFString, window) == .success
    else { fail("WINDOW_FOCUS_FAILED") }
  Thread.sleep(forTimeInterval: 0.08)
  var focused: AnyObject?
  guard NSWorkspace.shared.frontmostApplication?.processIdentifier == pid,
    AXUIElementCopyAttributeValue(app, kAXFocusedWindowAttribute as CFString, &focused) == .success,
    let focused, CFEqual(focused, window) else { fail("WINDOW_FOCUS_FAILED") }
}

private func copyDeepLink() {
  guard let down = CGEvent(keyboardEventSource: nil, virtualKey: 37, keyDown: true),
    let up = CGEvent(keyboardEventSource: nil, virtualKey: 37, keyDown: false) else {
    fail("COPY_LINK_EVENT_FAILED")
  }
  down.flags = [.maskCommand, .maskAlternate]
  up.flags = [.maskCommand, .maskAlternate]
  down.post(tap: .cghidEventTap)
  up.post(tap: .cghidEventTap)
}

/// Explicit, opt-in diagnostic only. The copy shortcut changes the clipboard;
/// it is never called by restart, distribution, or automatic recovery.
func probeSelectedTasks(_ process: (pid: pid_t, birth: String)) -> WindowTaskProbeRecord {
  guard argument("--allow-focus-and-clipboard") == "yes" else { fail("EXPLICIT_OPT_IN_REQUIRED") }
  let before = countStandardWindows(process)
  guard !before.window_ids.isEmpty else { fail("WINDOW_NOT_FOUND") }
  let windows = mappedWindows(process, ids: before.window_ids)
  let app = AXUIElementCreateApplication(process.pid)
  let pasteboard = NSPasteboard.general
  var taskIDs = Set<String>()
  for (_, window) in windows {
    guard processBirth(process.pid) == process.birth else { fail("PROCESS_IDENTITY_REJECTED") }
    focusExactWindow(app, window: window, pid: process.pid)
    let priorChange = pasteboard.changeCount
    copyDeepLink()
    let deadline = Date().addingTimeInterval(1.5)
    while pasteboard.changeCount == priorChange && Date() < deadline {
      Thread.sleep(forTimeInterval: 0.02)
    }
    let afterChange = pasteboard.changeCount
    guard let task = validateCopiedTaskLink(pasteboard.string(forType: .string),
      beforeChange: priorChange, afterChange: afterChange) else { fail("COPY_LINK_AMBIGUOUS") }
    guard taskIDs.insert(task).inserted else { fail("WINDOW_MAPPING_AMBIGUOUS") }
    var focused: AnyObject?
    let remapped = mappedWindows(process, ids: before.window_ids)
    let sameMapping = remapped.count == windows.count && zip(remapped, windows).allSatisfy {
      $0.0.0 == $0.1.0 && CFEqual($0.0.1, $0.1.1)
    }
    guard AXUIElementCopyAttributeValue(app, kAXFocusedWindowAttribute as CFString, &focused) == .success,
      let focused, CFEqual(focused, window),
      NSWorkspace.shared.frontmostApplication?.processIdentifier == process.pid,
      processBirth(process.pid) == process.birth,
      countStandardWindows(process).window_ids == before.window_ids,
      sameMapping else {
      fail("WINDOW_MAPPING_CHANGED")
    }
  }
  return WindowTaskProbeRecord(
    process: ProcessRecord(pid: process.pid, birth_id: process.birth),
    window_ids: before.window_ids, observed_task_count: taskIDs.count)
}
