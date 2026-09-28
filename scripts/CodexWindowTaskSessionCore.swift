import CoreGraphics
import Foundation

/// Snapshot, restore, and rehearsal of the task shown in each ChatGPT window.
///
/// Desktop sends a task link to its most recently focused primary window and
/// opens File > New Window focused (inspected in ChatGPT 26.924.22138), so
/// each link is sent only while Accessibility shows the target window focused,
/// and the copied link of that window must equal the task before it counts.
final class WindowTaskSession<System: WindowTaskSessionSystem> {
  let system: System
  let reader: WindowTaskReader<System>

  init(system: System) {
    self.system = system
    reader = WindowTaskReader(system: system)
  }

  /// The task, frame, and focus of every window. Focus returns to the window
  /// that had it, and the clipboard is restored when nothing else wrote to it.
  func snapshot() throws -> (entries: [WindowTaskEntry], clipboardRestored: Bool) {
    let (ids, windows) = try reader.prepare()
    let frames = try frames(of: windows)
    try requireRestorable(windows)
    let focused = focusedIndex(in: windows)
    reader.beginVisibleChanges()
    do {
      let tasks = try reader.readTasks(ids: ids, windows: windows)
      refocus(focused.map { windows[$0] })
      let entries = ids.indices.map {
        WindowTaskEntry(windowID: ids[$0], frame: frames[$0], taskID: tasks[$0], focused: $0 == focused)
      }
      return (entries, reader.finishVisibleChanges())
    } catch {
      refocus(focused.map { windows[$0] })
      _ = reader.finishVisibleChanges()
      throw error
    }
  }

  /// Makes the open windows match `plan`. Open windows are paired with plan
  /// entries by frame (a recheck after recovery); a sole unmatched window, as
  /// after a relaunch, takes the first entry; New Window creates every entry
  /// left without a window. Each window is navigated only when its copied
  /// link differs from its planned task.
  func restore(
    _ plan: [PlannedWindowTask], focusIndex: Int?, mode: WindowTaskRestoreMode = .relaunch
  ) throws -> WindowTaskRestoreResult {
    guard planIsValid(plan, focusIndex: focusIndex) else {
      throw WindowTaskProbeFailure.restorePlanInvalid
    }
    if case .recheck(let recoveryTasks) = mode {
      return try recheck(plan, recoveryTasks: recoveryTasks)
    }
    let (_, windows) = try reader.prepare()
    var assigned = try assign(windows, to: plan)
    reader.beginVisibleChanges()
    do {
      var verified = Array(repeating: false, count: plan.count)
      for index in plan.indices {
        let window: System.Window
        let existed = assigned[index] != nil
        if let existing = assigned[index] {
          window = existing
        } else {
          window = try createWindow(anchor: assigned.values.first!, frame: nil)
          assigned[index] = window
        }
        // A window that cannot take its frame still gets its task, but it
        // does not count as restored.
        let placed = (try? place(window, plan[index].frame)) != nil
        let earlier = plan.indices.filter { $0 < index && verified[$0] }
          .map { (assigned[$0]!, plan[$0].taskID) }
        let shown = try show(
          plan[index].taskID, in: window, alreadyShowingIsPossible: existed, unchanged: earlier)
        verified[index] = placed && shown
      }
      // A later link must not have moved an earlier window off its task.
      for index in plan.indices where verified[index] {
        try reader.focus(assigned[index]!)
        verified[index] = (try? reader.copyTaskLink()) == plan[index].taskID
      }
      refocus(focusIndex.flatMap { assigned[$0] })
      return WindowTaskRestoreResult(
        verified: verified, clipboardRestored: reader.finishVisibleChanges())
    } catch {
      refocus(focusIndex.flatMap { assigned[$0] })
      _ = reader.finishVisibleChanges()
      throw error
    }
  }

  /// After recovery, which may have sent its own task link to the most
  /// recently focused window. Only a planned window that now shows one of
  /// recovery's tasks instead of its own is navigated back; nothing is
  /// created, moved, or closed. A window without a unique planned frame, or
  /// whose link cannot be read is left as it is and reported unverified.
  /// Afterwards the app that was frontmost before the
  /// recheck is activated again, or ChatGPT's focused window if it was.
  private func recheck(
    _ plan: [PlannedWindowTask], recoveryTasks: Set<String>
  ) throws -> WindowTaskRestoreResult {
    let (_, windows) = try reader.prepare()
    let frames = try frames(of: windows)
    let current = system.focusedWindow()
    let frontmost = system.frontmostApplication()
    var verified = Array(repeating: false, count: plan.count)
    var checked: [(window: System.Window, taskID: String)] = []
    var navigationAttempted = false
    reader.beginVisibleChanges()
    defer {
      if let frontmost, !system.isDesktop(frontmost) {
        system.activate(frontmost)
      } else {
        refocus(current)
      }
    }
    do {
      for index in plan.indices {
        let matches = windows.indices.filter { framesMatch(frames[$0], plan[index].frame) }
        guard matches.count == 1 else { continue }
        let window = windows[matches[0]]
        try reader.focus(window)
        let shown = try? reader.copyTaskLink()
        if shown == plan[index].taskID {
          verified[index] = true
        } else if let shown, recoveryTasks.contains(shown) {
          navigationAttempted = true
          verified[index] = try show(
            plan[index].taskID, in: window, alreadyShowingIsPossible: false, unchanged: checked)
        }
        if verified[index] {
          checked.append((window, plan[index].taskID))
        }
      }
      // Even a failed last attempt may have changed an earlier window.
      if navigationAttempted {
        for (window, task) in checked {
          guard let index = plan.firstIndex(where: { $0.taskID == task }) else { continue }
          try reader.focus(window)
          verified[index] = (try? reader.copyTaskLink()) == task
        }
      }
      return WindowTaskRestoreResult(
        verified: verified, clipboardRestored: reader.finishVisibleChanges())
    } catch {
      _ = reader.finishVisibleChanges()
      throw error
    }
  }

  /// Proves the restore steps without a restart: opens one new window per
  /// original, sends it the original's task, checks it, checks every original
  /// still shows its own task, then closes only the windows it opened.
  func rehearse() throws -> (windowIDs: [UInt32], verified: Int, clipboardRestored: Bool) {
    let (ids, windows) = try reader.prepare()
    let frames = try frames(of: windows)
    let focused = focusedIndex(in: windows)
    reader.beginVisibleChanges()
    var created: [(window: System.Window, frame: CGRect?)] = []
    var occupied = frames
    var verified = 0
    var failure: Error?
    do {
      let tasks = try reader.readTasks(ids: ids, windows: windows)
      for index in windows.indices {
        let frame = rehearsalFrame(for: frames[index], avoiding: occupied)
        occupied.append(frame)
        let window = try createWindow(anchor: windows[index], frame: frame, created: &created)
        guard try show(
          tasks[index], in: window, alreadyShowingIsPossible: false,
          unchanged: Array(zip(windows, tasks))) else {
          throw WindowTaskProbeFailure.taskNavigationFailed
        }
        verified += 1
      }
      for (window, task) in zip(windows, tasks) {
        try reader.focus(window)
        guard try reader.copyTaskLink() == task else {
          // Put the user's window back on its task before reporting.
          _ = try? show(task, in: window, alreadyShowingIsPossible: false)
          throw WindowTaskProbeFailure.originalWindowChanged
        }
      }
    } catch {
      failure = error
    }
    let closed = close(created)
    // An AX element becoming invalid does not prove the OS window closed.
    // The exact original WindowServer/Accessibility inventory must return,
    // including when an opened window could not be observed for cleanup.
    let originalInventoryRestored = (try? system.windowIDs()) == ids
    refocus(focused.map { windows[$0] })
    let clipboardRestored = reader.finishVisibleChanges()
    guard closed && originalInventoryRestored else {
      throw WindowTaskProbeFailure.rehearsalWindowLeftOpen
    }
    if let failure { throw failure }
    return (ids, verified, clipboardRestored)
  }

  private func planIsValid(_ plan: [PlannedWindowTask], focusIndex: Int?) -> Bool {
    guard !plan.isEmpty, plan.count <= maximumProbedWindows else { return false }
    if let focusIndex, !plan.indices.contains(focusIndex) { return false }
    for (index, entry) in plan.enumerated() {
      guard canonicalTaskID(entry.taskID) == entry.taskID, frameIsUsable(entry.frame) else {
        return false
      }
      for other in plan[..<index] where framesMatch(other.frame, entry.frame) || other.taskID == entry.taskID {
        return false
      }
    }
    return true
  }

  /// Pairs open windows with plan entries by frame. One unmatched window is
  /// accepted only when it is the sole window, as after a relaunch.
  private func assign(_ windows: [System.Window], to plan: [PlannedWindowTask]) throws -> [Int: System.Window] {
    let frames = try frames(of: windows)
    var assigned: [Int: System.Window] = [:]
    var unmatched: [System.Window] = []
    for (window, frame) in zip(windows, frames) {
      let matches = plan.indices.filter { framesMatch(plan[$0].frame, frame) }
      if matches.count == 1, assigned[matches[0]] == nil {
        assigned[matches[0]] = window
      } else {
        unmatched.append(window)
      }
    }
    if unmatched.count == 1, assigned.isEmpty {
      assigned[0] = unmatched[0]
    } else if !unmatched.isEmpty {
      throw WindowTaskProbeFailure.restoreLayoutMismatch
    }
    return assigned
  }

  private func createWindow(anchor: System.Window, frame: CGRect?) throws -> System.Window {
    var created: [(window: System.Window, frame: CGRect?)] = []
    return try createWindow(anchor: anchor, frame: frame, created: &created)
  }

  /// An active app keys its new window, which is then Desktop's most recently
  /// focused window; a window opened in the background might not be. Each
  /// window that appears is recorded with the frame Desktop gave it.
  private func createWindow(
    anchor: System.Window, frame: CGRect?, created: inout [(window: System.Window, frame: CGRect?)]
  ) throws -> System.Window {
    // After a relaunch the item appears only once the renderer is ready.
    guard reader.waitFor(newWindowItemTimeout, { system.newWindowItemAvailable() }) else {
      throw WindowTaskProbeFailure.newWindowUnavailable
    }
    try reader.focus(anchor)
    let before = try system.standardWindows()
    guard system.processBirthMatches() else { throw WindowTaskProbeFailure.processIdentityRejected }
    guard system.pressNewWindow() else { throw WindowTaskProbeFailure.newWindowUnavailable }
    var opened: System.Window?
    let appeared = reader.waitFor(newWindowTimeout) {
      guard let focused = system.focusedWindow(),
        !before.contains(where: { system.sameWindow($0, focused) }) else { return false }
      opened = focused
      return system.hasKeyboardFocus(focused)
    }
    // Remember the focused new window before the fallible full inventory read;
    // then record any other new windows that the inventory exposes.
    if appeared, let window = opened,
      !created.contains(where: { system.sameWindow($0.window, window) }) {
      created.append((window, system.frame(window)))
    }
    let fresh = try system.standardWindows().filter { window in
      !before.contains { system.sameWindow($0, window) }
    }
    for window in fresh where !created.contains(where: { system.sameWindow($0.window, window) }) {
      created.append((window, system.frame(window)))
    }
    guard appeared, let window = opened, fresh.count == 1,
      system.sameWindow(fresh[0], window) else { throw WindowTaskProbeFailure.newWindowFailed }
    if let frame { try place(window, frame) }
    return window
  }

  private func place(_ window: System.Window, _ frame: CGRect) throws {
    if let current = system.frame(window), framesMatch(current, frame) { return }
    guard system.processBirthMatches() else { throw WindowTaskProbeFailure.processIdentityRejected }
    guard system.setFrame(window, frame), let actual = system.frame(window),
      framesMatch(actual, frame) else { throw WindowTaskProbeFailure.windowFrameFailed }
  }

  /// True once `window` copies `taskID`. A link is sent only while the window
  /// has keyboard focus, and focus must stay there while Desktop navigates.
  /// A window that may already show the task is checked first. Before a
  /// second link, the windows in `unchanged` must still show their tasks: a
  /// first link that landed elsewhere without moving focus stops here
  /// instead of moving another window too.
  private func show(
    _ taskID: String, in window: System.Window, alreadyShowingIsPossible: Bool,
    unchanged: [(window: System.Window, taskID: String)] = []
  ) throws -> Bool {
    if alreadyShowingIsPossible {
      try reader.focus(window)
      if (try? reader.copyTaskLink()) == taskID { return true }
    }
    for attempt in 0..<taskNavigationAttempts {
      if attempt > 0 {
        for other in unchanged {
          try reader.focus(other.window)
          guard (try? reader.copyTaskLink()) == other.taskID else {
            throw WindowTaskProbeFailure.navigationTargetChanged
          }
        }
      }
      try reader.focus(window)
      guard system.openTaskLink(taskID) else { throw WindowTaskProbeFailure.taskLinkOpenFailed }
      let deadline = system.now() + taskNavigationTimeout
      while system.now() < deadline {
        system.pause(taskNavigationPollInterval)
        try reader.recheckBeforeShortcut(window, lostFocus: .navigationTargetChanged)
        if (try? reader.copyTaskLink()) == taskID { return true }
      }
    }
    return false
  }

  /// Desktop saves a primary window's frame when it closes; putting back
  /// the frame it opened with keeps a rehearsal from changing where the next
  /// window opens.
  private func close(_ windows: [(window: System.Window, frame: CGRect?)]) -> Bool {
    var closed = true
    for (window, frame) in windows.reversed() {
      let alive: Bool
      do { alive = try system.isWindowAlive(window) } catch {
        closed = false
        continue
      }
      if !alive { continue }
      if let frame, system.processBirthMatches() { _ = system.setFrame(window, frame) }
      guard system.processBirthMatches(), system.closeWindow(window) else {
        closed = false
        continue
      }
      var readFailed = false
      let disappeared = reader.waitFor(windowCloseTimeout) {
        do { return try !system.isWindowAlive(window) } catch {
          readFailed = true
          return true
        }
      }
      if readFailed || !disappeared { closed = false }
    }
    return closed
  }

  /// Before any visible change: a window that could not be recreated
  /// refuses the snapshot, so the restart never starts.
  private func requireRestorable(_ windows: [System.Window]) throws {
    guard !windows.contains(where: { system.isFullScreen($0) }) else {
      throw WindowTaskProbeFailure.windowFullScreen
    }
    guard windows.count == 1 || system.newWindowItemAvailable() else {
      throw WindowTaskProbeFailure.newWindowUnavailable
    }
  }

  private func refocus(_ window: System.Window?) {
    guard let window, system.processBirthMatches() else { return }
    system.requestFocus(window)
    _ = reader.waitFor(probeFocusTimeout) { system.hasKeyboardFocus(window) }
  }

  private func frames(of windows: [System.Window]) throws -> [CGRect] {
    try windows.map {
      guard let frame = system.frame($0), frameIsUsable(frame) else {
        throw WindowTaskProbeFailure.windowGeometryFailed
      }
      return frame
    }
  }

  private func focusedIndex(in windows: [System.Window]) -> Int? {
    guard let focused = system.focusedWindow() else { return nil }
    return windows.firstIndex { system.sameWindow($0, focused) }
  }
}
