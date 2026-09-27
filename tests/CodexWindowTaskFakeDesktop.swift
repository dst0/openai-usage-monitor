import CoreGraphics
import Foundation

/// A scripted Desktop for `WindowTaskSession`: windows with frames and
/// selected tasks, keyboard focus, Desktop's most recently focused window
/// (where a task link lands), File > New Window, and a pasteboard. It models
/// the routing inspected in ChatGPT 26.924.22138, not a live app.
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
  var closeSucceeds = true
  var setFrameSucceeds = true
  /// Accepts a frame change without moving the window.
  var setFrameIgnored = false
  var fullScreen: Set<Int> = []
  var openTaskLinkFails = false
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
    axOnlyFocus = nil
    focus(window)
  }
  func hasKeyboardFocus(_ window: Int) -> Bool { focused == window }
  func copyShortcutKeyIsExpected() -> Bool { true }
  func postCopyShortcut() -> Bool {
    if let focused, let task = tasks[focused] { write("codex://threads/\(task)") }
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
  func pressNewWindow() -> Bool {
    guard newWindowItemAvailable() else { return false }
    let window = addWindow(persistedFrame, task: nil)
    log.append("new \(window) after focus \(focused.map(String.init) ?? "none")")
    if newWindowOpensTwo { addWindow(persistedFrame.offsetBy(dx: 300, dy: 0), task: nil) }
    if newWindowsFocusedWithoutKey {
      axOnlyFocus = window
    } else if newWindowsAreKeyed {
      focus(window)
    }
    return true
  }
  func focusedWindow() -> Int? { axOnlyFocus ?? focused }
  func standardWindows() throws -> [Int] { alive }
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
  func isWindowAlive(_ window: Int) -> Bool { alive.contains(window) }

  /// Desktop shows and focuses the window it navigates.
  private func deliverLinks() {
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
