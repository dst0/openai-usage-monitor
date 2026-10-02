import CoreGraphics
import Foundation

/// A scripted Desktop for `WindowTaskSession`: windows with frames and
/// selected tasks, keyboard focus, Desktop's most recently focused window
/// (where a task link lands), File > New Window, and a pasteboard. It models
/// the routing inspected in ChatGPT 26.924.22138 and 26.928.31416, not a live app.
final class FakeDesktop: WindowTaskSessionSystem {
  typealias Window = Int
  var log: [String] = []
  var clock: TimeInterval = 0
  var frames: [Int: CGRect] = [:]
  /// The task each window shows; a window without one is on the home page.
  var tasks: [Int: String] = [:]
  var alive: [Int] = []
  var focused: Int?
  var lastActive: Int?
  var nextWindow = 0
  var persistedFrame = CGRect(x: 10, y: 40, width: 1200, height: 800)
  var newWindowAvailable = true
  /// The clock time from which File > New Window is in the menu, as after a
  /// relaunch before the renderer reports the multiwindow feature.
  var newWindowItemFrom: TimeInterval = 0
  var newWindowsAreKeyed = true
  /// The new window is created asynchronously after the menu action returns.
  var newWindowOpenDelay: TimeInterval = 0
  var secondNewWindowOpenDelay: TimeInterval?
  var newWindowRequests = 0
  var pendingNewWindowAt: TimeInterval?
  /// Accessibility refuses a focus request for an opened window.
  var newWindowsRefuseFocus = false
  var closeSucceeds = true
  /// Simulates an Accessibility liveness read failing for an open window.
  var unreadableLiveness: Set<Int> = []
  /// AX says its element is invalid while WindowServer still lists the window.
  var staleLiveness: Set<Int> = []
  /// A newly opened window becomes temporarily unavailable in AXWindows.
  var standardWindowsFailsAtCount: Int?
  /// AXWindows fails after the menu accepted a delayed creation request.
  var standardWindowsFailsAfterNewRequest = false
  var setFrameSucceeds = true
  /// Accepts a frame change without moving the window.
  var setFrameIgnored = false
  var fullScreen: Set<Int> = []
  var openTaskLinkFails = false
  /// This many copies of a shown task answer late, after the reader's wait.
  var lateCopies = 0
  var lateCopyDelay: TimeInterval = 2
  var lateWrites: [(at: TimeInterval, text: String)] = []
  /// The frontmost application: 0 is ChatGPT, anything else another app.
  var frontmost: Int32? = 0
  var activations: [Int32] = []
  /// Desktop navigates without focusing the window it navigates.
  var navigationKeepsFocus = false
  /// New Window makes the new window Accessibility's focused window without
  /// keying it, as in an inactive app.
  var newWindowsFocusedWithoutKey = false
  var axOnlyFocus: Int?
  /// The process birth check fails once the log holds an entry with this prefix.
  var birthFailsAfterLog: String?
  /// The process birth check fails from this call on (1-based).
  var birthFailsFromCheck: Int?
  var birthChecks = 0
  var frameHistory: [Int: [CGRect]] = [:]
  /// Seconds before a link lands; nil means Desktop never mounts the task.
  var navigationDelay: TimeInterval? = 1
  /// Sends every link to this window instead of the most recently focused.
  var linksGoTo: Int?
  /// Moves this window to this task whenever a link lands in another window.
  var crossTalk: (window: Int, task: String)?
  /// Moves keyboard focus after the given copy shortcut, as a click would.
  var focusMove: (afterCopy: Int, to: Int)?
  var copyPosts = 0
  /// A Copy deeplink action in these windows never produces readable text.
  var unreadableTaskLinks: Set<Int> = []
  /// New Window opens a second, unexpected window as well.
  var newWindowOpensTwo = false
  var pending: [(at: TimeInterval, window: Int, task: String)] = []
  var linksSent: [(focused: Int?, task: String)] = []
  var changeCount = 500
  var text: String? = "user clipboard"
  var types = ["public.utf8-plain-text"]
  var preserved: (text: String?, count: Int)?

  @discardableResult
  func addWindow(_ frame: CGRect, task: String?) -> Int {
    let window = nextWindow
    nextWindow += 1
    alive.append(window)
    frames[window] = frame
    tasks[window] = task
    return window
  }

  func focus(_ window: Int) {
    focused = window
    lastActive = window
  }

  func now() -> TimeInterval { clock }
  func pause(_ seconds: TimeInterval) {
    clock += seconds
    if let due = pendingNewWindowAt, due <= clock {
      pendingNewWindowAt = nil
      openNewWindow()
    }
    deliverLinks()
  }
  func isOptedIn() -> Bool { true }
  func unmetPrecondition() -> WindowTaskProbeFailure? { nil }
  func processBirthMatches() -> Bool {
    if let prefix = birthFailsAfterLog, log.contains(where: { $0.hasPrefix(prefix) }) {
      return false
    }
    birthChecks += 1
    guard let from = birthFailsFromCheck else { return true }
    return birthChecks < from
  }
  func windowIDs() throws -> [UInt32] { alive.map { UInt32(100 + $0) } }
  func mappedWindows(_ ids: [UInt32]) throws -> [Int] {
    let windows = ids.map { Int($0) - 100 }
    // Like the real mapping, equal frames cannot be told apart.
    for window in windows
    where windows.filter({ framesMatch(frames[$0]!, frames[window]!) }).count > 1 {
      throw WindowTaskProbeFailure.windowMappingAmbiguous
    }
    return windows
  }
  func sameWindow(_ left: Int, _ right: Int) -> Bool { left == right }
  func beginVisibleChanges() { log.append("visible") }
  func requestFocus(_ window: Int) {
    guard alive.contains(window) else { return }
    log.append("focus \(window)")
    if newWindowsRefuseFocus && window > 0 { return }
    frontmost = 0
    axOnlyFocus = nil
    focus(window)
  }
  func hasKeyboardFocus(_ window: Int) -> Bool { focused == window }
  func copyShortcutKeyIsExpected() -> Bool { true }
  func postCopyShortcut() -> Bool {
    if let focused, !unreadableTaskLinks.contains(focused), let task = tasks[focused] {
      if lateCopies > 0 {
        lateCopies -= 1
        lateWrites.append((clock + lateCopyDelay, "codex://threads/\(task)"))
      } else {
        write("codex://threads/\(task)")
      }
    }
    copyPosts += 1
    if let move = focusMove, move.afterCopy == copyPosts { focus(move.to) }
    return true
  }
  func pasteboardChangeCount() -> Int { changeCount }
  func pasteboardOffersTaskText() -> Bool { pasteboardTypesAllowTaskRead(types) }
  func pasteboardString() -> String? { text }
  func preserveClipboard() {
    log.append("save")
    preserved = (text, changeCount)
  }
  func pasteboardHoldsTaskLink() -> Bool {
    pasteboardTypesAllowTaskRead(types) && text.flatMap { taskID(fromLink: $0) } != nil
  }
  func restoreClipboard(expectedChangeCount: Int) -> Bool {
    guard let preserved, changeCount == expectedChangeCount else { return false }
    write(preserved.text ?? "")
    log.append("restored")
    return true
  }
  func frame(_ window: Int) -> CGRect? { frames[window] }
  func setFrame(_ window: Int, _ frame: CGRect) -> Bool {
    log.append("frame \(window)")
    guard setFrameSucceeds else { return false }
    if !setFrameIgnored {
      frames[window] = frame
      frameHistory[window, default: []].append(frame)
    }
    return true
  }
  func newWindowItemAvailable() -> Bool { newWindowAvailable && clock >= newWindowItemFrom }
  func isFullScreen(_ window: Int) -> Bool { fullScreen.contains(window) }
  func frontmostApplication() -> Int32? { frontmost }
  func isDesktop(_ application: Int32) -> Bool { application == 0 }
  func activate(_ application: Int32) {
    activations.append(application)
    frontmost = application
  }
  func pressNewWindow() -> Bool {
    guard newWindowItemAvailable() else { return false }
    newWindowRequests += 1
    let delay = newWindowRequests == 2
      ? (secondNewWindowOpenDelay ?? newWindowOpenDelay) : newWindowOpenDelay
    if delay > 0 {
      log.append("new requested")
      pendingNewWindowAt = clock + delay
      return true
    }
    openNewWindow()
    return true
  }
  private func openNewWindow() {
    let window = addWindow(persistedFrame, task: nil)
    log.append("new \(window) after focus \(focused.map(String.init) ?? "none")")
    if newWindowOpensTwo { addWindow(persistedFrame.offsetBy(dx: 300, dy: 0), task: nil) }
    if newWindowsFocusedWithoutKey {
      axOnlyFocus = window
    } else if newWindowsAreKeyed {
      focus(window)
    }
  }
  func focusedWindow() -> Int? { axOnlyFocus ?? focused }
  func standardWindows() throws -> [Int] {
    if standardWindowsFailsAfterNewRequest && newWindowRequests > 0 {
      throw WindowTaskProbeFailure.windowAccessFailed
    }
    if let count = standardWindowsFailsAtCount, alive.count >= count {
      throw WindowTaskProbeFailure.windowAccessFailed
    }
    return alive
  }
  func openTaskLink(_ taskID: String) -> Bool {
    guard !openTaskLinkFails else { return false }
    linksSent.append((focused, taskID))
    if let delay = navigationDelay, let target = linksGoTo ?? lastActive {
      pending.append((clock + delay, target, taskID))
    }
    return true
  }
  func closeWindow(_ window: Int) -> Bool {
    log.append("close \(window)")
    guard closeSucceeds else { return false }
    alive.removeAll { $0 == window }
    if focused == window { focused = alive.last }
    return true
  }
  func isWindowAlive(_ window: Int) throws -> Bool {
    if unreadableLiveness.contains(window) { throw WindowTaskProbeFailure.windowAccessFailed }
    if staleLiveness.contains(window) { return false }
    return alive.contains(window)
  }

  /// Desktop shows and focuses the window it navigates.
  private func deliverLinks() {
    let dueWrites = lateWrites.filter { $0.at <= clock }
    lateWrites.removeAll { $0.at <= clock }
    for late in dueWrites { write(late.text) }
    let due = pending.filter { $0.at <= clock }
    pending.removeAll { $0.at <= clock }
    for link in due where alive.contains(link.window) {
      tasks[link.window] = link.task
      if !navigationKeepsFocus { focus(link.window) }
      if let crossTalk, crossTalk.window != link.window { tasks[crossTalk.window] = crossTalk.task }
    }
  }

  private func write(_ value: String) {
    changeCount += 1
    text = value
    types = ["public.utf8-plain-text"]
  }
}
