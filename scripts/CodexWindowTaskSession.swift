import ApplicationServices
import Cocoa

/// The Desktop menu titles, English only: another language fails closed.
private let fileMenuTitle = "File"
private let newWindowMenuTitle = "New Window"
private let maximumRestorePlanBytes = 256 * 1024

extension SystemWindowTaskProbe: WindowTaskSessionSystem {
  func frame(_ window: AXUIElement) -> CGRect? {
    guard let point = decodeAXPoint(copyAXValue(window, kAXPositionAttribute as String)),
      let size = decodeAXSize(copyAXValue(window, kAXSizeAttribute as String)) else { return nil }
    return CGRect(origin: point, size: size)
  }

  /// Position, size, then position again, so a size change that the window
  /// server constrains against the old origin still lands where planned.
  func setFrame(_ window: AXUIElement, _ frame: CGRect) -> Bool {
    var origin = frame.origin
    var size = frame.size
    guard let position = AXValueCreate(.cgPoint, &origin),
      let dimensions = AXValueCreate(.cgSize, &size) else { return false }
    let moved = AXUIElementSetAttributeValue(window, kAXPositionAttribute as CFString, position)
    let resized = AXUIElementSetAttributeValue(window, kAXSizeAttribute as CFString, dimensions)
    let placed = AXUIElementSetAttributeValue(window, kAXPositionAttribute as CFString, position)
    return moved == .success && resized == .success && placed == .success
  }

  func newWindowItemAvailable() -> Bool { newWindowItem() != nil }

  /// Presses exactly one enabled File > New Window item of this process.
  /// Desktop shows that item only when its multiwindow feature is on.
  func pressNewWindow() -> Bool {
    guard let item = newWindowItem() else { return false }
    return AXUIElementPerformAction(item, kAXPressAction as CFString) == .success
  }

  /// A window that does not have the attribute is not full screen; any
  /// other read failure counts as full screen, so the snapshot fails closed.
  func isFullScreen(_ window: AXUIElement) -> Bool {
    var raw: AnyObject?
    switch AXUIElementCopyAttributeValue(window, "AXFullScreen" as CFString, &raw) {
    case .success:
      guard let raw, CFGetTypeID(raw) == CFBooleanGetTypeID() else { return true }
      return raw as! Bool
    case .attributeUnsupported, .noValue: return false
    default: return true
    }
  }

  func frontmostApplication() -> Int32? {
    var raw: AnyObject?
    var pid: pid_t = 0
    guard AXUIElementCopyAttributeValue(
      AXUIElementCreateSystemWide(), kAXFocusedApplicationAttribute as CFString, &raw) == .success,
      let raw, CFGetTypeID(raw) == AXUIElementGetTypeID(),
      AXUIElementGetPid(raw as! AXUIElement, &pid) == .success else { return nil }
    return pid
  }

  func isDesktop(_ application: Int32) -> Bool { application == process.pid }

  func activate(_ application: Int32) {
    _ = NSRunningApplication(processIdentifier: application)?.activate(options: [])
  }

  private func newWindowItem() -> AXUIElement? {
    guard let bar = axElement(app, kAXMenuBarAttribute as String),
      let file = onlyChild(of: bar, titled: fileMenuTitle),
      let menus = axChildren(file), menus.count == 1,
      let item = onlyChild(of: menus[0], titled: newWindowMenuTitle),
      (copyAXValue(item, kAXEnabledAttribute as String) as? Bool) == true
    else { return nil }
    return item
  }

  func focusedWindow() -> AXUIElement? {
    guard let window = axElement(app, kAXFocusedWindowAttribute as String),
      (copyAXValue(window, kAXSubroleAttribute as String) as? String)
        == kAXStandardWindowSubrole as String else { return nil }
    return window
  }

  func standardWindows() throws -> [AXUIElement] {
    guard let windows = axArray(app, kAXWindowsAttribute as String) else {
      throw WindowTaskProbeFailure.windowAccessFailed
    }
    return windows.filter {
      (copyAXValue($0, kAXSubroleAttribute as String) as? String) == kAXStandardWindowSubrole as String
    }
  }

  /// Hands the link to the exact running bundle without activating it;
  /// Desktop itself shows and focuses the window it navigates.
  func openTaskLink(_ taskID: String) -> Bool {
    guard let link = taskLink(for: taskID), processBirthMatches(),
      let bundle = NSRunningApplication(processIdentifier: process.pid)?.bundleURL else { return false }
    let open = Process()
    open.executableURL = URL(fileURLWithPath: "/usr/bin/open")
    open.arguments = ["-g", "-a", bundle.path, link.absoluteString]
    open.environment = ["PATH": "/usr/bin:/bin:/usr/sbin:/sbin"]
    open.standardOutput = FileHandle.nullDevice
    open.standardError = FileHandle.nullDevice
    do { try open.run() } catch { return false }
    open.waitUntilExit()
    return open.terminationReason == .exit && open.terminationStatus == 0
  }

  func closeWindow(_ window: AXUIElement) -> Bool {
    guard let button = axElement(window, kAXCloseButtonAttribute as String) else { return false }
    return AXUIElementPerformAction(button, kAXPressAction as CFString) == .success
  }

  func isWindowAlive(_ window: AXUIElement) -> Bool {
    var role: AnyObject?
    return AXUIElementCopyAttributeValue(window, kAXRoleAttribute as CFString, &role) == .success
  }

  private func onlyChild(of element: AXUIElement, titled title: String) -> AXUIElement? {
    let matches = (axChildren(element) ?? []).filter {
      (copyAXValue($0, kAXTitleAttribute as String) as? String) == title
    }
    return matches.count == 1 ? matches[0] : nil
  }
}

func axElement(_ element: AXUIElement, _ attribute: String) -> AXUIElement? {
  guard let raw = copyAXValue(element, attribute), CFGetTypeID(raw) == AXUIElementGetTypeID() else {
    return nil
  }
  return (raw as! AXUIElement)
}

func axArray(_ element: AXUIElement, _ attribute: String) -> [AXUIElement]? {
  guard let raw = copyAXValue(element, attribute), CFGetTypeID(raw) == CFArrayGetTypeID(),
    let values = raw as? [AnyObject],
    values.allSatisfy({ CFGetTypeID($0) == AXUIElementGetTypeID() }) else { return nil }
  return values.map { $0 as! AXUIElement }
}

func axChildren(_ element: AXUIElement) -> [AXUIElement]? {
  axArray(element, kAXChildrenAttribute as String)
}

/// Before a restart: every window's task, frame, and focus. The task IDs go
/// to the calling Monitor process on stdout and nowhere else.
func snapshotWindowTasks(_ process: (pid: pid_t, birth: String)) -> WindowTaskSnapshotRecord {
  let session = WindowTaskSession(system: SystemWindowTaskProbe(process: process))
  do {
    let result = try session.snapshot()
    return WindowTaskSnapshotRecord(
      process: ProcessRecord(pid: process.pid, birth_id: process.birth),
      windows: result.entries.map {
        WindowTaskEntryRecord(
          window_id: $0.windowID, frame: FrameRecord($0.frame), task_id: $0.taskID, focused: $0.focused)
      },
      clipboard_restored: result.clipboardRestored)
  } catch {
    fail(((error as? WindowTaskProbeFailure) ?? .probeFailed).rawValue)
  }
}

/// After a relaunch or recovery: the plan arrives on stdin, never in argv.
func restoreWindowTasks(_ process: (pid: pid_t, birth: String)) -> WindowTaskRestoreRecord {
  let input = FileHandle.standardInput.readData(ofLength: maximumRestorePlanBytes + 1)
  guard input.count <= maximumRestorePlanBytes,
    let plan = try? JSONDecoder().decode(WindowTaskRestorePlanRecord.self, from: input),
    let mode = restoreMode(plan) else {
    fail(WindowTaskProbeFailure.restorePlanInvalid.rawValue)
  }
  let session = WindowTaskSession(system: SystemWindowTaskProbe(process: process))
  do {
    let result = try session.restore(
      plan.windows.map { PlannedWindowTask(taskID: $0.task_id, frame: $0.frame.rect) },
      focusIndex: plan.focus_index, mode: mode)
    return WindowTaskRestoreRecord(
      process: ProcessRecord(pid: process.pid, birth_id: process.birth),
      verified: result.verified, clipboard_restored: result.clipboardRestored)
  } catch {
    fail(((error as? WindowTaskProbeFailure) ?? .probeFailed).rawValue)
  }
}


/// Explicit diagnostic: opens, navigates, checks, and closes one extra window
/// per original. It carries only counts back to the caller.
func rehearseWindowTaskRestore(_ process: (pid: pid_t, birth: String)) -> WindowTaskRehearsalRecord {
  let session = WindowTaskSession(system: SystemWindowTaskProbe(process: process))
  do {
    let result = try session.rehearse()
    return WindowTaskRehearsalRecord(
      process: ProcessRecord(pid: process.pid, birth_id: process.birth),
      window_ids: result.windowIDs, verified_count: result.verified,
      clipboard_restored: result.clipboardRestored)
  } catch {
    fail(((error as? WindowTaskProbeFailure) ?? .probeFailed).rawValue)
  }
}
