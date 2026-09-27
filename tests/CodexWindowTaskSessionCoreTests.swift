import CoreGraphics
import Foundation

@main
struct CodexWindowTaskSessionCoreTests {
  static let a = "01a00000-0000-4000-8000-00000000000a"
  static let b = "01a00000-0000-4000-8000-00000000000b"
  static let c = "01a00000-0000-4000-8000-00000000000c"
  static let x = "01a00000-0000-4000-8000-0000000000ee"
  static let r = "01a00000-0000-4000-8000-0000000000ff"
  static let f0 = CGRect(x: 0, y: 30, width: 900, height: 700)
  static let f1 = CGRect(x: 950, y: 30, width: 900, height: 700)
  static let f2 = CGRect(x: 0, y: 760, width: 900, height: 600)

  static func main() {
    snapshotRecordsEachWindowAndRestoresFocusAndClipboard()
    snapshotRefusesWindowsItCouldNotRecreate()
    aFailedSnapshotPutsFocusAndClipboardBack()
    aReplacedProcessGetsNoFurtherWindowChanges()
    aNewWindowWithoutKeyboardFocusIsNotALinkTarget()
    aSecondLinkIsNotSentAfterTheFirstMovedAnotherWindow()
    planLimitsAreInclusive()
    restoreAfterRelaunchCreatesOneWindowPerExtraEntry()
    restoreKeepsARelaunchedWindowThatAlreadyHasAPlannedFrame()
    restoreWaitsForNewWindowAfterARelaunch()
    mismatchedLayoutsAndInvalidPlansChangeNothing()
    missingOrBackgroundNewWindowsFailClosed()
    newWindowNeedsTheAnchorFocusedAndTheSameProcess()
    focusIsRetakenBeforeEveryLink()
    aWindowThatCannotBePlacedStillGetsItsTaskButIsNotVerified()
    aFailedRestoreGivesFocusBackToThePlannedWindow()
    aLinkLandingInAnotherWindowIsCaught()
    aTaskThatNeverMountsIsReportedUnverified()
    aLaterLinkThatMovedAnEarlierWindowIsCaught()
    recheckMovesBackOnlyWindowsThatRecoveryMoved()
    rehearsalOpensChecksAndClosesOnlyItsOwnWindows()
    rehearsalCleansUpAfterEveryFailure()
    rehearsalFramesStayDistinctAndUsable()
    print("Window task session sequencing passed")
  }

  static func plan(_ entries: [(String, CGRect)]) -> [PlannedWindowTask] {
    entries.map { PlannedWindowTask(taskID: $0.0, frame: $0.1) }
  }

  static func restore(
    _ desktop: FakeDesktop, _ entries: [(String, CGRect)], focus: Int? = nil,
    mode: WindowTaskRestoreMode = .relaunch
  ) -> Result<WindowTaskRestoreResult, WindowTaskProbeFailure> {
    do {
      return .success(try WindowTaskSession(system: desktop).restore(
        plan(entries), focusIndex: focus, mode: mode))
    } catch {
      return .failure(error as! WindowTaskProbeFailure)
    }
  }

  static func snapshot(
    _ desktop: FakeDesktop
  ) -> Result<(entries: [WindowTaskEntry], clipboardRestored: Bool), WindowTaskProbeFailure> {
    do { return .success(try WindowTaskSession(system: desktop).snapshot()) } catch {
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
    // Focus starts on the first window, so reading the last one moves it.
    let desktop = FakeDesktop()
    desktop.focus(desktop.addWindow(f0, task: a))
    desktop.addWindow(f1, task: b)
    guard case .success(let result) = snapshot(desktop) else { fatalError("snapshot failed") }
    precondition(result.entries.map { $0.windowID } == [100, 101])
    precondition(result.entries.map { $0.taskID } == [a, b])
    precondition(result.entries.map { $0.frame } == [f0, f1])
    precondition(result.entries.map { $0.focused } == [true, false])
    precondition(desktop.focused == 0 && desktop.linksSent.isEmpty)
    precondition(result.clipboardRestored && desktop.text == "user clipboard")
    precondition(!desktop.log.contains { $0.hasPrefix("new") || $0.hasPrefix("frame") })
  }

  static func snapshotRefusesWindowsItCouldNotRecreate() {
    let full = FakeDesktop()
    full.addWindow(f0, task: a)
    full.addWindow(f1, task: b)
    full.fullScreen = [1]
    precondition(failure(snapshot(full)) == .windowFullScreen)
    let noNewWindow = FakeDesktop()
    noNewWindow.addWindow(f0, task: a)
    noNewWindow.addWindow(f1, task: b)
    noNewWindow.newWindowAvailable = false
    precondition(failure(snapshot(noNewWindow)) == .newWindowUnavailable)
    for desktop in [full, noNewWindow] {
      precondition(!desktop.log.contains("visible") && desktop.copyPosts == 0)
    }
    // One window needs no New Window after the relaunch.
    let single = FakeDesktop()
    single.addWindow(f0, task: a)
    single.newWindowAvailable = false
    guard case .success = snapshot(single) else { fatalError("single-window snapshot failed") }
  }

  static func aFailedSnapshotPutsFocusAndClipboardBack() {
    // The second window is on the home page, so it copies no link.
    let desktop = FakeDesktop()
    desktop.focus(desktop.addWindow(f0, task: a))
    desktop.addWindow(f1, task: nil)
    precondition(failure(snapshot(desktop)) == .copyLinkMissing)
    precondition(desktop.focused == 0, "focus must return to the window that had it")
    precondition(desktop.text == "user clipboard" && desktop.log.contains("restored"))
    precondition(!desktop.log.contains { $0.hasPrefix("new") || $0.hasPrefix("frame") })
    // Two windows on one task could hide a copy from the wrong window.
    let twins = FakeDesktop()
    twins.addWindow(f0, task: a)
    twins.addWindow(f1, task: a)
    precondition(failure(snapshot(twins)) == .taskLinkDuplicate)
  }

  static func aReplacedProcessGetsNoFurtherWindowChanges() {
    let desktop = FakeDesktop()
    desktop.focus(desktop.addWindow(f0, task: a))
    desktop.birthFailsAfterLog = "new"
    precondition(failure(rehearse(desktop)) == .rehearsalWindowLeftOpen)
    let opened = desktop.log.firstIndex { $0.hasPrefix("new") }!
    let after = desktop.log[(opened + 1)...]
    precondition(!after.contains { $0.hasPrefix("frame") || $0.hasPrefix("close") || $0.hasPrefix("focus") })
    precondition(desktop.linksSent.isEmpty)
  }

  static func aNewWindowWithoutKeyboardFocusIsNotALinkTarget() {
    let desktop = relaunched()
    desktop.newWindowsFocusedWithoutKey = true
    precondition(failure(restore(desktop, [(a, f0), (b, f1)])) == .newWindowFailed)
    precondition(desktop.linksSent.count == 1 && desktop.tasks[1] == nil)
  }

  static func aSecondLinkIsNotSentAfterTheFirstMovedAnotherWindow() {
    // Desktop sends the link to window 0 and leaves focus where it was.
    let desktop = FakeDesktop()
    desktop.addWindow(f0, task: a)
    desktop.addWindow(f1, task: x)
    desktop.linksGoTo = 0
    desktop.navigationKeepsFocus = true
    precondition(failure(restore(desktop, [(a, f0), (b, f1)])) == .navigationTargetChanged)
    precondition(desktop.linksSent.count == 1, "no second link after a misrouted first one")
  }

  static func planLimitsAreInclusive() {
    let windows = (0..<64).map { index -> (String, CGRect) in
      (String(format: "01a00000-0000-4000-8000-%012x", index),
       CGRect(x: CGFloat(index * 10), y: 30, width: 300, height: 250))
    }
    let desktop = FakeDesktop()
    desktop.addWindow(CGRect(x: 2000, y: 900, width: 900, height: 700), task: a)
    guard case .success(let result) = restore(desktop, windows, mode: .recheck(recoveryTaskIDs: []))
    else { fatalError("a 64-window plan was refused") }
    precondition(result.verified.count == 64)
    precondition(frameIsUsable(CGRect(x: 0, y: 0, width: 300, height: 250)))
  }

  static func restoreAfterRelaunchCreatesOneWindowPerExtraEntry() {
    let desktop = relaunched()
    guard case .success(let result) = restore(desktop, [(a, f0), (b, f1), (c, f2)], focus: 1) else {
      fatalError("restore failed")
    }
    precondition(result.verified == [true, true, true])
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
    // Desktop reopens at the frame of the window it saved last.
    let desktop = relaunched(at: f2)
    guard case .success(let result) = restore(desktop, [(a, f0), (b, f1), (c, f2)]) else {
      fatalError("restore failed")
    }
    precondition(result.verified == [true, true, true])
    precondition(desktop.tasks[0] == c && desktop.frames[0] == f2)
    precondition(desktop.tasks[1] == a && desktop.frames[1] == f0)
    precondition(desktop.frameHistory[0] == nil, "a window already on its frame is not moved")
  }

  static func restoreWaitsForNewWindowAfterARelaunch() {
    let late = relaunched()
    late.newWindowItemFrom = 8
    guard case .success(let result) = restore(late, [(a, f0), (b, f1)]) else {
      fatalError("restore did not wait for New Window")
    }
    precondition(result.verified == [true, true] && late.alive.count == 2)
    let never = relaunched()
    never.newWindowItemFrom = .infinity
    precondition(failure(restore(never, [(a, f0), (b, f1)])) == .newWindowUnavailable)
    precondition(never.clock >= newWindowItemTimeout && never.alive == [0])
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
    precondition(failure(restore(missing, [(a, f0), (b, f1)], focus: 0)) == .newWindowUnavailable)
    precondition(missing.tasks[0] == a && missing.linksSent.count == 1)
    precondition(missing.log.contains("restored") && missing.focused == 0)
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

  static func newWindowNeedsTheAnchorFocusedAndTheSameProcess() {
    let desktop = FakeDesktop()
    desktop.addWindow(f0, task: a)
    desktop.focus(desktop.addWindow(f1, task: b))
    guard case .success = rehearse(desktop) else { fatalError("rehearsal failed") }
    // Each window opens while its original has focus, never another window.
    precondition(desktop.log.filter { $0.hasPrefix("new") } == ["new 2 after focus 0", "new 3 after focus 1"])
    // A process replaced after the anchor was focused gets no New Window
    // press. Birth checks: plan preparation (1), window 0's focus and
    // recheck (2, 3), the anchor's focus and recheck (4, 5), then the check
    // immediately before the press (6).
    let recycled = FakeDesktop()
    recycled.focus(recycled.addWindow(f0, task: a))
    recycled.birthFailsFromCheck = 6
    precondition(failure(restore(recycled, [(a, f0), (b, f1)])) == .processIdentityRejected)
    precondition(!recycled.log.contains { $0.hasPrefix("new") })
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

  static func aWindowThatCannotBePlacedStillGetsItsTaskButIsNotVerified() {
    for ignored in [false, true] {
      let desktop = relaunched()
      desktop.setFrameSucceeds = ignored
      desktop.setFrameIgnored = ignored
      guard case .success(let result) = restore(desktop, [(a, f0), (b, f1)]) else {
        fatalError("restore threw")
      }
      precondition(result.verified == [false, false], "ignored=\(ignored)")
      precondition(desktop.tasks[0] == a && desktop.tasks[1] == b, "tasks still restored")
    }
    // A window already on its planned frame is never moved.
    let placed = FakeDesktop()
    placed.focus(placed.addWindow(f0, task: a))
    placed.setFrameSucceeds = false
    guard case .success(let result) = restore(placed, [(a, f0)]) else { fatalError("restore failed") }
    precondition(result.verified == [true] && !placed.log.contains("frame 0"))
  }

  static func aFailedRestoreGivesFocusBackToThePlannedWindow() {
    let desktop = FakeDesktop()
    desktop.addWindow(f0, task: a)
    desktop.addWindow(f1, task: x)
    desktop.openTaskLinkFails = true
    precondition(failure(restore(desktop, [(a, f0), (b, f1)], focus: 0)) == .taskLinkOpenFailed)
    precondition(desktop.focused == 0 && desktop.text == "user clipboard")
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

  static func recheckMovesBackOnlyWindowsThatRecoveryMoved() {
    // Window 0 kept its task, recovery sent its task link to window 1, the
    // user moved window 2 elsewhere and opened window 3; entry 3's window
    // was closed.
    let desktop = FakeDesktop()
    desktop.focus(desktop.addWindow(f0, task: a))
    desktop.addWindow(f1, task: r)
    desktop.addWindow(f2, task: x)
    desktop.addWindow(CGRect(x: 1900, y: 30, width: 900, height: 700), task: nil)
    let f3 = CGRect(x: 950, y: 760, width: 900, height: 600)
    let d = "01a00000-0000-4000-8000-00000000000d"
    guard case .success(let result) = restore(
      desktop, [(a, f0), (b, f1), (c, f2), (d, f3)], focus: 0,
      mode: .recheck(recoveryTaskIDs: [r])) else { fatalError("recheck failed") }
    precondition(result.verified == [true, true, true, true])
    precondition(desktop.tasks[1] == b && desktop.tasks[2] == x && desktop.tasks[3] == nil)
    precondition(desktop.linksSent.count == 1 && desktop.linksSent[0].focused == 1)
    precondition(!desktop.log.contains { $0.hasPrefix("new") || $0.hasPrefix("frame") })
    // Focus returns to the window the user had, not the plan's.
    precondition(desktop.focused == 0 && desktop.text == "user clipboard")
    // A window recovery moved that cannot be brought back is reported.
    let stuck = FakeDesktop()
    stuck.addWindow(f0, task: r)
    stuck.navigationDelay = nil
    guard case .success(let failed) = restore(
      stuck, [(a, f0)], mode: .recheck(recoveryTaskIDs: [r])) else { fatalError("recheck threw") }
    precondition(failed.verified == [false])
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
    // Each rehearsal window got its opening frame back before it closed,
    // so Desktop saves the same bounds it would have saved anyway.
    for window in [2, 3] {
      precondition(desktop.frameHistory[window]?.last == desktop.persistedFrame, "\(window)")
    }
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
    // A link that also moved an original is reported, and the original is
    // navigated back to its own task first.
    let changed = FakeDesktop()
    changed.addWindow(f0, task: a)
    changed.addWindow(f1, task: b)
    changed.crossTalk = (0, x)
    precondition(failure(rehearse(changed)) == .originalWindowChanged)
    precondition(changed.alive == [0, 1] && changed.tasks[0] == a)
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
    // Three originals whose first choices collide: A's is taken by C, so A
    // moves back, onto the frame B would pick first.
    let origin = CGRect(x: 100, y: 100, width: 900, height: 700)
    let desktop = FakeDesktop()
    desktop.addWindow(origin, task: a)
    desktop.addWindow(origin.offsetBy(dx: -80, dy: -80), task: b)
    desktop.addWindow(origin.offsetBy(dx: 40, dy: 40), task: c)
    guard case .success = rehearse(desktop) else { fatalError("rehearsal failed") }
    let placed = [3, 4, 5].compactMap { desktop.frameHistory[$0]?.first }
    precondition(placed.count == 3)
    for (index, frame) in placed.enumerated() {
      precondition(!placed[..<index].contains { framesMatch($0, frame) }, "rehearsal frames collide")
      precondition(![0, 1, 2].contains { framesMatch(desktop.frames[$0]!, frame) })
    }
    precondition(frameIsUsable(f0))
    precondition(!frameIsUsable(CGRect(x: 0, y: 0, width: 299, height: 700)))
    precondition(!frameIsUsable(CGRect(x: 0, y: 0, width: 900, height: 249)))
    precondition(!frameIsUsable(CGRect(x: CGFloat.nan, y: 0, width: 900, height: 700)))
  }
}
