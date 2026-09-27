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
  var newWindowsAreKeyed = true
  var closeSucceeds = true
  var setFrameSucceeds = true
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
  func processBirthMatches() -> Bool { true }
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
    frames[window] = frame
    return true
  }
  func pressNewWindow() -> Bool {
    guard newWindowAvailable else { return false }
    let window = addWindow(persistedFrame, task: nil)
    log.append("new \(window)")
    if newWindowOpensTwo { addWindow(persistedFrame.offsetBy(dx: 300, dy: 0), task: nil) }
    if newWindowsAreKeyed { focus(window) }
    return true
  }
  func focusedWindow() -> Int? { focused }
  func standardWindows() throws -> [Int] { alive }
  func openTaskLink(_ taskID: String) -> Bool {
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
      focus(link.window)
      if let crossTalk, crossTalk.window != link.window { tasks[crossTalk.window] = crossTalk.task }
    }
  }

  private func write(_ value: String) {
    changeCount += 1
    text = value
    types = ["public.utf8-plain-text"]
  }
}

@main
struct CodexWindowTaskSessionCoreTests {
  static let a = "01a00000-0000-4000-8000-00000000000a"
  static let b = "01a00000-0000-4000-8000-00000000000b"
  static let c = "01a00000-0000-4000-8000-00000000000c"
  static let x = "01a00000-0000-4000-8000-0000000000ee"
  static let f0 = CGRect(x: 0, y: 30, width: 900, height: 700)
  static let f1 = CGRect(x: 950, y: 30, width: 900, height: 700)
  static let f2 = CGRect(x: 0, y: 760, width: 900, height: 600)

  static func main() {
    snapshotRecordsEachWindowAndRestoresFocusAndClipboard()
    restoreAfterRelaunchCreatesOneWindowPerExtraEntry()
    restoreKeepsARelaunchedWindowThatAlreadyHasAPlannedFrame()
    recheckAfterRecoveryNavigatesOnlyWindowsThatDrifted()
    mismatchedLayoutsAndInvalidPlansChangeNothing()
    missingOrBackgroundNewWindowsFailClosed()
    focusIsRetakenBeforeEveryLink()
    aLinkLandingInAnotherWindowIsCaught()
    aTaskThatNeverMountsIsReportedUnverified()
    aLaterLinkThatMovedAnEarlierWindowIsCaught()
    rehearsalOpensChecksAndClosesOnlyItsOwnWindows()
    rehearsalCleansUpAfterEveryFailure()
    rehearsalFramesStayDistinctAndUsable()
    print("Window task session sequencing passed")
  }

  static func plan(_ entries: [(String, CGRect)]) -> [PlannedWindowTask] {
    entries.map { PlannedWindowTask(taskID: $0.0, frame: $0.1) }
  }

  static func restore(
    _ desktop: FakeDesktop, _ entries: [(String, CGRect)], focus: Int? = nil
  ) -> Result<WindowTaskRestoreResult, WindowTaskProbeFailure> {
    do {
      return .success(try WindowTaskSession(system: desktop).restore(plan(entries), focusIndex: focus))
    } catch {
      return .failure(error as! WindowTaskProbeFailure)
    }
  }

  static func rehearse(
    _ desktop: FakeDesktop
  ) -> Result<(windowIDs: [UInt32], verified: Int, clipboardRestored: Bool), WindowTaskProbeFailure> {
    do { return .success(try WindowTaskSession(system: desktop).rehearse()) } catch {
      return .failure(error as! WindowTaskProbeFailure)
    }
  }

  static func failure<T>(_ result: Result<T, WindowTaskProbeFailure>) -> WindowTaskProbeFailure? {
    if case .failure(let failure) = result { return failure }
    return nil
  }

  /// A single window on the home page, as Desktop starts after a relaunch.
  static func relaunched(at frame: CGRect = CGRect(x: 10, y: 40, width: 1200, height: 800)) -> FakeDesktop {
    let desktop = FakeDesktop()
    desktop.persistedFrame = frame
    desktop.focus(desktop.addWindow(frame, task: nil))
    return desktop
  }

  static func snapshotRecordsEachWindowAndRestoresFocusAndClipboard() {
    let desktop = FakeDesktop()
    desktop.addWindow(f0, task: a)
    desktop.focus(desktop.addWindow(f1, task: b))
    let result = try! WindowTaskSession(system: desktop).snapshot()
    precondition(result.entries.map { $0.windowID } == [100, 101])
    precondition(result.entries.map { $0.taskID } == [a, b])
    precondition(result.entries.map { $0.frame } == [f0, f1])
    precondition(result.entries.map { $0.focused } == [false, true])
    precondition(desktop.focused == 1 && desktop.linksSent.isEmpty)
    precondition(result.clipboardRestored && desktop.text == "user clipboard")
    precondition(!desktop.log.contains { $0.hasPrefix("new") || $0.hasPrefix("frame") })
  }

  static func restoreAfterRelaunchCreatesOneWindowPerExtraEntry() {
    let desktop = relaunched()
    guard case .success(let result) = restore(desktop, [(a, f0), (b, f1), (c, f2)], focus: 1) else {
      fatalError("restore failed")
    }
    precondition(result.verified == [true, true, true] && result.windowIDs == [100, 101, 102])
    precondition(desktop.alive == [0, 1, 2])
    precondition(desktop.tasks[0] == a && desktop.tasks[1] == b && desktop.tasks[2] == c)
    precondition(desktop.frames[0] == f0 && desktop.frames[1] == f1 && desktop.frames[2] == f2)
    // Each link was sent while its own target window had keyboard focus.
    precondition(desktop.linksSent.map { $0.focused } == [0, 1, 2])
    precondition(desktop.linksSent.map { $0.task } == [a, b, c])
    precondition(desktop.focused == 1)
    precondition(result.clipboardRestored && desktop.text == "user clipboard")
  }

  static func restoreKeepsARelaunchedWindowThatAlreadyHasAPlannedFrame() {
    // Desktop reopens at the frame of the window it closed last.
    let desktop = relaunched(at: f2)
    guard case .success(let result) = restore(desktop, [(a, f0), (b, f1), (c, f2)]) else {
      fatalError("restore failed")
    }
    precondition(result.verified == [true, true, true])
    precondition(desktop.tasks[0] == c && desktop.frames[0] == f2)
    precondition(result.windowIDs == [101, 102, 100])
  }

  static func recheckAfterRecoveryNavigatesOnlyWindowsThatDrifted() {
    let desktop = FakeDesktop()
    desktop.addWindow(f0, task: a)
    desktop.addWindow(f1, task: x)
    desktop.focus(desktop.addWindow(f2, task: c))
    guard case .success(let result) = restore(desktop, [(a, f0), (b, f1), (c, f2)], focus: 2) else {
      fatalError("recheck failed")
    }
    precondition(result.verified == [true, true, true])
    precondition(desktop.linksSent.count == 1 && desktop.linksSent[0].focused == 1)
    precondition(desktop.tasks[1] == b && desktop.focused == 2)
    precondition(!desktop.log.contains { $0.hasPrefix("new") || $0.hasPrefix("frame") })
  }

  static func mismatchedLayoutsAndInvalidPlansChangeNothing() {
    let strangers = FakeDesktop()
    strangers.addWindow(CGRect(x: 5, y: 40, width: 800, height: 600), task: a)
    strangers.addWindow(CGRect(x: 900, y: 40, width: 800, height: 600), task: b)
    let partly = FakeDesktop()
    partly.addWindow(f0, task: a)
    partly.addWindow(CGRect(x: 900, y: 40, width: 800, height: 600), task: b)
    for desktop in [strangers, partly] {
      precondition(failure(restore(desktop, [(a, f0), (b, f1)])) == .restoreLayoutMismatch)
      precondition(!desktop.log.contains("visible") && desktop.linksSent.isEmpty)
    }
    let small = CGRect(x: 0, y: 30, width: 200, height: 700)
    let invalid: [([(String, CGRect)], Int?)] = [
      ([], nil), ([(a, f0), (a, f1)], nil), ([(a, f0), (b, f0.offsetBy(dx: 1, dy: 1))], nil),
      ([(a.uppercased(), f0)], nil), ([("new", f0)], nil), ([(a, small)], nil),
      ([(a, f0), (b, f1)], 2), ([(a, f0)], -1),
      ((0..<65).map { index in (a, f0.offsetBy(dx: CGFloat(index * 10), dy: 0)) }, nil),
    ]
    for (entries, focus) in invalid {
      let desktop = relaunched()
      precondition(failure(restore(desktop, entries, focus: focus)) == .restorePlanInvalid)
      precondition(!desktop.log.contains("visible") && desktop.linksSent.isEmpty)
    }
  }

  static func missingOrBackgroundNewWindowsFailClosed() {
    let missing = relaunched()
    missing.newWindowAvailable = false
    precondition(failure(restore(missing, [(a, f0), (b, f1)])) == .newWindowUnavailable)
    precondition(missing.tasks[0] == a && missing.linksSent.count == 1)
    precondition(missing.log.contains("restored"))
    // A window opened in the background is not Desktop's link target.
    let background = relaunched()
    background.newWindowsAreKeyed = false
    precondition(failure(restore(background, [(a, f0), (b, f1)])) == .newWindowFailed)
    precondition(background.linksSent.count == 1 && background.tasks[1] == nil)
    // Two new windows make the new one ambiguous.
    let doubled = relaunched()
    doubled.newWindowOpensTwo = true
    precondition(failure(restore(doubled, [(a, f0), (b, f1)])) == .newWindowFailed)
    precondition(doubled.linksSent.count == 1)
  }

  static func focusIsRetakenBeforeEveryLink() {
    // A click moved focus after the window was checked: the link must still
    // go to the window being restored, never to the one clicked.
    let desktop = FakeDesktop()
    desktop.addWindow(f0, task: a)
    desktop.addWindow(f1, task: x)
    // Copy 1 checks window 0, copy 2 checks window 1; then the user clicks 0.
    desktop.focusMove = (afterCopy: 2, to: 0)
    guard case .success(let result) = restore(desktop, [(a, f0), (b, f1)]) else {
      fatalError("restore failed")
    }
    precondition(result.verified == [true, true] && desktop.tasks[0] == a && desktop.tasks[1] == b)
  }

  static func aLinkLandingInAnotherWindowIsCaught() {
    let desktop = FakeDesktop()
    desktop.addWindow(f0, task: a)
    desktop.addWindow(f1, task: x)
    desktop.linksGoTo = 0
    precondition(failure(restore(desktop, [(a, f0), (b, f1)])) == .navigationTargetChanged)
    precondition(desktop.linksSent.count == 1 && desktop.log.contains("restored"))
  }

  static func aTaskThatNeverMountsIsReportedUnverified() {
    let desktop = relaunched()
    desktop.navigationDelay = nil
    guard case .success(let result) = restore(desktop, [(a, f0)]) else { fatalError("restore threw") }
    precondition(result.verified == [false])
    precondition(desktop.linksSent.count == taskNavigationAttempts)
    precondition(desktop.clock >= Double(taskNavigationAttempts) * taskNavigationTimeout)
  }

  static func aLaterLinkThatMovedAnEarlierWindowIsCaught() {
    let desktop = relaunched()
    desktop.crossTalk = (0, x)
    guard case .success(let result) = restore(desktop, [(a, f0), (b, f1)]) else {
      fatalError("restore threw")
    }
    precondition(result.verified == [false, true])
  }

  static func rehearsalOpensChecksAndClosesOnlyItsOwnWindows() {
    let desktop = FakeDesktop()
    desktop.focus(desktop.addWindow(f0, task: a))
    desktop.addWindow(f1, task: b)
    guard case .success(let result) = rehearse(desktop) else { fatalError("rehearsal failed") }
    precondition(result.windowIDs == [100, 101] && result.verified == 2 && result.clipboardRestored)
    precondition(desktop.alive == [0, 1] && desktop.tasks[0] == a && desktop.tasks[1] == b)
    precondition(desktop.linksSent.map { $0.focused } == [2, 3])
    precondition(desktop.log.contains("close 3") && desktop.log.contains("close 2"))
    precondition(desktop.focused == 0 && desktop.text == "user clipboard")
  }

  static func rehearsalCleansUpAfterEveryFailure() {
    let unmounted = FakeDesktop()
    unmounted.focus(unmounted.addWindow(f0, task: a))
    unmounted.addWindow(f1, task: b)
    unmounted.navigationDelay = nil
    precondition(failure(rehearse(unmounted)) == .taskNavigationFailed)
    precondition(unmounted.alive == [0, 1] && unmounted.text == "user clipboard")
    let stuck = FakeDesktop()
    stuck.addWindow(f0, task: a)
    stuck.closeSucceeds = false
    precondition(failure(rehearse(stuck)) == .rehearsalWindowLeftOpen)
    let changed = FakeDesktop()
    changed.addWindow(f0, task: a)
    changed.addWindow(f1, task: b)
    changed.crossTalk = (0, x)
    precondition(failure(rehearse(changed)) == .originalWindowChanged)
    precondition(changed.alive == [0, 1])
    let background = FakeDesktop()
    background.addWindow(f0, task: a)
    background.newWindowsAreKeyed = false
    precondition(failure(rehearse(background)) == .newWindowFailed)
    precondition(background.alive == [0] && background.linksSent.isEmpty)
  }

  static func rehearsalFramesStayDistinctAndUsable() {
    let shifted = f0.offsetBy(dx: rehearsalFrameOffset, dy: rehearsalFrameOffset)
    precondition(rehearsalFrame(for: f0, avoiding: [f0]) == shifted)
    precondition(rehearsalFrame(for: f0, avoiding: [f0, shifted])
      == f0.offsetBy(dx: -rehearsalFrameOffset, dy: -rehearsalFrameOffset))
    precondition(frameIsUsable(f0))
    precondition(!frameIsUsable(CGRect(x: 0, y: 0, width: 299, height: 700)))
    precondition(!frameIsUsable(CGRect(x: 0, y: 0, width: 900, height: 249)))
    precondition(!frameIsUsable(CGRect(x: CGFloat.nan, y: 0, width: 900, height: 700)))
  }
}
