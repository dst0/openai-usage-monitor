import Foundation

/// Every failure the probe reports. The raw values are the helper's fixed
/// stderr codes, which the Rust side names verbatim; none carries task,
/// window, or clipboard data.
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
}

/// Focuses each ChatGPT window in turn and copies its task link. Every check
/// that can fail without a visible change runs before the first focus
/// request. Task IDs stay in memory here; the result is the probed window IDs
/// and how many distinct task links they yielded.
struct WindowTaskProbe<System: WindowTaskProbeSystem> {
  let system: System

  func run() throws -> (windowIDs: [UInt32], taskCount: Int) {
    guard system.isOptedIn() else { throw WindowTaskProbeFailure.explicitOptInRequired }
    if let unmet = system.unmetPrecondition() { throw unmet }
    guard system.processBirthMatches() else { throw WindowTaskProbeFailure.processIdentityRejected }
    let ids = try system.windowIDs()
    guard !ids.isEmpty else { throw WindowTaskProbeFailure.windowNotFound }
    guard ids.count <= maximumProbedWindows else { throw WindowTaskProbeFailure.windowLimitExceeded }
    let windows = try system.mappedWindows(ids)
    guard windows.count == ids.count else { throw WindowTaskProbeFailure.windowMappingAmbiguous }
    system.beginVisibleChanges()
    var tasks = Set<String>()
    for window in windows {
      try focus(window)
      guard tasks.insert(try copyTaskLink()).inserted else {
        throw WindowTaskProbeFailure.taskLinkDuplicate
      }
      try confirmUnchanged(ids: ids, windows: windows, focused: window)
    }
    return (ids, tasks.count)
  }

  private func focus(_ window: System.Window) throws {
    guard system.processBirthMatches() else { throw WindowTaskProbeFailure.processIdentityRejected }
    system.requestFocus(window)
    guard waitFor(probeFocusTimeout, { system.hasKeyboardFocus(window) }) else {
      throw WindowTaskProbeFailure.windowFocusFailed
    }
    // Recheck immediately before the shortcut: same process, same focus,
    // and a key that still types the character the binding expects.
    guard system.processBirthMatches() else { throw WindowTaskProbeFailure.processIdentityRejected }
    guard system.hasKeyboardFocus(window) else { throw WindowTaskProbeFailure.windowFocusFailed }
    guard system.copyShortcutKeyIsExpected() else {
      throw WindowTaskProbeFailure.keyboardLayoutUnsupported
    }
  }

  /// Reads count, then text, then count again, and reads the text only after
  /// exactly one write that offers unconcealed plain text.
  private func copyTaskLink() throws -> String {
    let before = system.pasteboardChangeCount()
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
    return task
  }

  private func confirmUnchanged(
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

  private func waitFor(_ timeout: TimeInterval, _ condition: () -> Bool) -> Bool {
    let deadline = system.now() + timeout
    while !condition() {
      guard system.now() < deadline else { return false }
      system.pause(probePollInterval)
    }
    return true
  }
}
