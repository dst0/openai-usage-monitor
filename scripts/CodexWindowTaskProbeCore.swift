import Foundation

/// Every failure the window-task commands report. The raw values are the
/// helper's fixed stderr codes, which the Rust side names verbatim; none
/// carries task, window, or clipboard data.
enum WindowTaskProbeFailure: String, Error {
  case explicitOptInRequired = "EXPLICIT_OPT_IN_REQUIRED"
  case probeAccessDenied = "PROBE_ACCESS_DENIED"
  case pasteboardAccessNotAllowed = "PASTEBOARD_ACCESS_NOT_ALLOWED"
  case desktopVersionUnverified = "DESKTOP_VERSION_UNVERIFIED"
  case appShortcutConflict = "APP_SHORTCUT_CONFLICT"
  case keyboardLayoutUnsupported = "KEYBOARD_LAYOUT_UNSUPPORTED"
  case processIdentityRejected = "PROCESS_IDENTITY_REJECTED"
  case windowNotFound = "WINDOW_NOT_FOUND"
  case windowLimitExceeded = "WINDOW_LIMIT_EXCEEDED"
  case windowAccessFailed = "WINDOW_ACCESS_FAILED"
  case windowGeometryFailed = "WINDOW_GEOMETRY_FAILED"
  case windowInventoryMismatch = "WINDOW_INVENTORY_MISMATCH"
  case windowMinimized = "WINDOW_MINIMIZED"
  case windowMappingAmbiguous = "WINDOW_MAPPING_AMBIGUOUS"
  case windowFocusFailed = "WINDOW_FOCUS_FAILED"
  case copyLinkEventFailed = "COPY_LINK_EVENT_FAILED"
  case copyLinkMissing = "COPY_LINK_MISSING"
  case copyLinkAmbiguous = "COPY_LINK_AMBIGUOUS"
  case taskLinkDuplicate = "TASK_LINK_DUPLICATE"
  case windowMappingChanged = "WINDOW_MAPPING_CHANGED"
  /// File > New Window is missing, disabled, or not titled in English.
  case newWindowUnavailable = "NEW_WINDOW_UNAVAILABLE"
  /// No new, focused standard window appeared after New Window.
  case newWindowFailed = "NEW_WINDOW_FAILED"
  case windowFrameFailed = "WINDOW_FRAME_FAILED"
  case taskLinkOpenFailed = "TASK_LINK_OPEN_FAILED"
  /// Keyboard focus left the window a task link was sent for, so Desktop
  /// may have navigated a different window.
  case navigationTargetChanged = "NAVIGATION_TARGET_CHANGED"
  case taskNavigationFailed = "TASK_NAVIGATION_FAILED"
  case restorePlanInvalid = "RESTORE_PLAN_INVALID"
  /// The open windows are neither the planned layout nor one fresh window.
  case restoreLayoutMismatch = "RESTORE_LAYOUT_MISMATCH"
  /// A rehearsal found an original window no longer on its task.
  case originalWindowChanged = "ORIGINAL_WINDOW_CHANGED"
  /// A rehearsal could not close a window it opened.
  case rehearsalWindowLeftOpen = "REHEARSAL_WINDOW_LEFT_OPEN"
  /// A full-screen window cannot be restored to its frame.
  case windowFullScreen = "WINDOW_FULL_SCREEN"
  /// Any error that is not one of the cases above; none is expected.
  case probeFailed = "PROBE_FAILED"
}

/// Same limit the Rust side enforces on the response.
let maximumProbedWindows = 64
let probeFocusTimeout: TimeInterval = 1.0
let probeClipboardTimeout: TimeInterval = 1.5
let probePollInterval: TimeInterval = 0.02

/// Everything the probe needs from macOS. The real implementation talks to
/// Accessibility, WindowServer, the pasteboard, and the event system; tests
/// script a fake to check the order of checks and every fail-closed path.
protocol WindowTaskProbeSystem {
  associatedtype Window
  func now() -> TimeInterval
  func pause(_ seconds: TimeInterval)
  func isOptedIn() -> Bool
  /// The first unmet precondition that must hold before any visible change.
  func unmetPrecondition() -> WindowTaskProbeFailure?
  func processBirthMatches() -> Bool
  /// Exact standard ChatGPT window IDs, cross-checked against Accessibility.
  func windowIDs() throws -> [UInt32]
  /// One Accessibility window per ID, in order; throws when any is ambiguous,
  /// minimized, or unreadable.
  func mappedWindows(_ ids: [UInt32]) throws -> [Window]
  func sameWindow(_ left: Window, _ right: Window) -> Bool
  /// Called once, immediately before the first focus request.
  func beginVisibleChanges()
  /// Best-effort activation requests. Only `hasKeyboardFocus` authorizes.
  func requestFocus(_ window: Window)
  func hasKeyboardFocus(_ window: Window) -> Bool
  /// Whether the Copy deeplink key still types its expected character now.
  func copyShortcutKeyIsExpected() -> Bool
  /// Posts the shortcut to the verified ChatGPT process only.
  func postCopyShortcut() -> Bool
  func pasteboardChangeCount() -> Int
  /// Whether the current contents offer plain text and carry no concealed or
  /// transient marker; decided from the types, without reading any content.
  func pasteboardOffersTaskText() -> Bool
  func pasteboardString() -> String?
  /// Keeps the clipboard's current items in memory for `restoreClipboard`,
  /// unless they are concealed, transient, or too large to hold.
  func preserveClipboard()
  /// Writes the preserved items back only while the change count still
  /// equals `expectedChangeCount`, so a newer write by another app wins.
  func restoreClipboard(expectedChangeCount: Int) -> Bool
  /// Whether the clipboard now holds exactly a canonical task link, read
  /// only when its types allow it.
  func pasteboardHoldsTaskLink() -> Bool
}

/// Focus and copy steps shared by every window-task command. Task IDs stay
/// in this process's memory.
final class WindowTaskReader<System: WindowTaskProbeSystem> {
  let system: System
  /// The change count after the latest copy this process attributed to
  /// ChatGPT; the clipboard is restored only while it is still current.
  private var lastOwnChange: Int?
  private var preservedChange: Int?
  /// The count this process last observed. A different count before the
  /// next copy means another app wrote in between; that newer content is
  /// what gets preserved and restored.
  private var lastKnownChange: Int?

  init(system: System) { self.system = system }

  /// Every check that can fail without a visible change, in order.
  func prepare() throws -> (ids: [UInt32], windows: [System.Window]) {
    guard system.isOptedIn() else { throw WindowTaskProbeFailure.explicitOptInRequired }
    if let unmet = system.unmetPrecondition() { throw unmet }
    guard system.processBirthMatches() else { throw WindowTaskProbeFailure.processIdentityRejected }
    let ids = try system.windowIDs()
    guard !ids.isEmpty else { throw WindowTaskProbeFailure.windowNotFound }
    guard ids.count <= maximumProbedWindows else { throw WindowTaskProbeFailure.windowLimitExceeded }
    let windows = try system.mappedWindows(ids)
    guard windows.count == ids.count else { throw WindowTaskProbeFailure.windowMappingAmbiguous }
    return (ids, windows)
  }

  func beginVisibleChanges() {
    preservedChange = system.pasteboardChangeCount()
    lastKnownChange = preservedChange
    system.preserveClipboard()
    system.beginVisibleChanges()
  }

  /// Restores the clipboard when this run copied at least one link and no
  /// other write followed it. Returns whether the clipboard was restored.
  func finishVisibleChanges() -> Bool {
    guard let preserved = preservedChange else { return false }
    preservedChange = nil
    guard let own = lastOwnChange, own != preserved else { return false }
    return system.restoreClipboard(expectedChangeCount: own)
  }

  /// Focuses each window in turn and copies its task link. Duplicate links
  /// fail closed: a copy that reached the previous window again would look
  /// the same.
  func readTasks(ids: [UInt32], windows: [System.Window]) throws -> [String] {
    var tasks: [String] = []
    for window in windows {
      try focus(window)
      let task = try copyTaskLink()
      guard !tasks.contains(task) else { throw WindowTaskProbeFailure.taskLinkDuplicate }
      tasks.append(task)
      try confirmUnchanged(ids: ids, windows: windows, focused: window)
    }
    return tasks
  }

  func focus(_ window: System.Window) throws {
    guard system.processBirthMatches() else { throw WindowTaskProbeFailure.processIdentityRejected }
    system.requestFocus(window)
    guard waitFor(probeFocusTimeout, { system.hasKeyboardFocus(window) }) else {
      throw WindowTaskProbeFailure.windowFocusFailed
    }
    try recheckBeforeShortcut(window, lostFocus: .windowFocusFailed)
  }

  /// Immediately before a shortcut: same process, same focus, and a key that
  /// still types the character the binding expects.
  func recheckBeforeShortcut(_ window: System.Window, lostFocus: WindowTaskProbeFailure) throws {
    guard system.processBirthMatches() else { throw WindowTaskProbeFailure.processIdentityRejected }
    guard system.hasKeyboardFocus(window) else { throw lostFocus }
    guard system.copyShortcutKeyIsExpected() else {
      throw WindowTaskProbeFailure.keyboardLayoutUnsupported
    }
  }

  /// Reads count, then text, then count again, and reads the text only after
  /// exactly one write that offers unconcealed plain text.
  func copyTaskLink() throws -> String {
    let before = system.pasteboardChangeCount()
    if let known = lastKnownChange, before != known, !system.pasteboardHoldsTaskLink() {
      // Another app wrote since this run last looked: that newer copy is
      // what comes back. A task link there is ChatGPT's late answer to an
      // earlier shortcut, not the user's copy, so it is not kept.
      system.preserveClipboard()
      preservedChange = before
      lastOwnChange = nil
    }
    lastKnownChange = before
    do {
      let task = try copyOnce(before: before)
      lastKnownChange = lastOwnChange
      return task
    } catch {
      lastKnownChange = system.pasteboardChangeCount()
      throw error
    }
  }

  private func copyOnce(before: Int) throws -> String {
    guard system.postCopyShortcut() else { throw WindowTaskProbeFailure.copyLinkEventFailed }
    _ = waitFor(probeClipboardTimeout) {
      let count = system.pasteboardChangeCount()
      return count != before && (count != before &+ 1 || system.pasteboardOffersTaskText())
    }
    let after = system.pasteboardChangeCount()
    guard after != before else { throw WindowTaskProbeFailure.copyLinkMissing }
    guard before < Int.max, after == before + 1, system.pasteboardOffersTaskText(),
      system.pasteboardChangeCount() == after else {
      throw WindowTaskProbeFailure.copyLinkAmbiguous
    }
    // A write racing into the gap before this read is still read into
    // memory; the confirming count below then rejects it.
    let link = system.pasteboardString()
    let confirmed = system.pasteboardChangeCount()
    guard let task = validateCopiedTaskLink(
      link, beforeChange: before, afterChange: after, confirmedChange: confirmed
    ) else { throw WindowTaskProbeFailure.copyLinkAmbiguous }
    lastOwnChange = confirmed
    return task
  }

  func confirmUnchanged(
    ids: [UInt32], windows: [System.Window], focused: System.Window
  ) throws {
    guard try system.windowIDs() == ids else { throw WindowTaskProbeFailure.windowMappingChanged }
    let remapped = try system.mappedWindows(ids)
    guard remapped.count == windows.count,
      zip(remapped, windows).allSatisfy({ system.sameWindow($0.0, $0.1) }),
      system.hasKeyboardFocus(focused),
      system.processBirthMatches() else {
      throw WindowTaskProbeFailure.windowMappingChanged
    }
  }

  func waitFor(_ timeout: TimeInterval, _ condition: () -> Bool) -> Bool {
    let deadline = system.now() + timeout
    while !condition() {
      guard system.now() < deadline else { return false }
      system.pause(probePollInterval)
    }
    return true
  }
}

/// Focuses each ChatGPT window in turn and copies its task link. Every check
/// that can fail without a visible change runs before the first focus
/// request. Task IDs stay in memory here; the result is the probed window IDs,
/// how many distinct task links they yielded, and whether the clipboard was
/// put back.
struct WindowTaskProbe<System: WindowTaskProbeSystem> {
  let system: System

  func run() throws -> (windowIDs: [UInt32], taskCount: Int, clipboardRestored: Bool) {
    let reader = WindowTaskReader(system: system)
    let (ids, windows) = try reader.prepare()
    reader.beginVisibleChanges()
    do {
      let tasks = try reader.readTasks(ids: ids, windows: windows)
      return (ids, tasks.count, reader.finishVisibleChanges())
    } catch {
      _ = reader.finishVisibleChanges()
      throw error
    }
  }
}
