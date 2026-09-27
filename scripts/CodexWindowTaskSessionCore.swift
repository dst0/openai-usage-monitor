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
  func restore(_ plan: [PlannedWindowTask], focusIndex: Int?) throws -> WindowTaskRestoreResult {
    guard planIsValid(plan, focusIndex: focusIndex) else {
      throw WindowTaskProbeFailure.restorePlanInvalid
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
          try place(window, plan[index].frame)
        } else {
          window = try createWindow(anchor: assigned.values.first!, frame: plan[index].frame)
          assigned[index] = window
        }
        verified[index] = try show(plan[index].taskID, in: window, alreadyShowingIsPossible: existed)
      }
      // A later link must not have moved an earlier window off its task.
      for index in plan.indices where verified[index] {
        try reader.focus(assigned[index]!)
        verified[index] = (try? reader.copyTaskLink()) == plan[index].taskID
      }
      refocus(focusIndex.flatMap { assigned[$0] })
      let ids = try windowIDs(in: plan.indices.map { assigned[$0]! })
      return WindowTaskRestoreResult(
        windowIDs: ids, verified: verified, clipboardRestored: reader.finishVisibleChanges())
    } catch {
      refocus(focusIndex.flatMap { assigned[$0] })
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
    var created: [System.Window] = []
    var occupied = frames
    var verified = 0
    var failure: Error?
    do {
      let tasks = try reader.readTasks(ids: ids, windows: windows)
      for index in windows.indices {
        let frame = rehearsalFrame(for: frames[index], avoiding: occupied)
        occupied.append(frame)
        let window = try createWindow(anchor: windows[index], frame: frame, created: &created)
        guard try show(tasks[index], in: window, alreadyShowingIsPossible: false) else {
          throw WindowTaskProbeFailure.taskNavigationFailed
        }
        verified += 1
      }
      for (window, task) in zip(windows, tasks) {
        try reader.focus(window)
        guard try reader.copyTaskLink() == task else {
          throw WindowTaskProbeFailure.originalWindowChanged
        }
      }
    } catch {
      failure = error
    }
    let closed = close(created)
    refocus(focused.map { windows[$0] })
    let clipboardRestored = reader.finishVisibleChanges()
    guard closed else { throw WindowTaskProbeFailure.rehearsalWindowLeftOpen }
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

  private func createWindow(anchor: System.Window, frame: CGRect) throws -> System.Window {
    var created: [System.Window] = []
    return try createWindow(anchor: anchor, frame: frame, created: &created)
  }

  /// An active app keys its new window, which is then Desktop's most recently
  /// focused window; a window opened in the background might not be.
  private func createWindow(
    anchor: System.Window, frame: CGRect, created: inout [System.Window]
  ) throws -> System.Window {
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
    // Every window that appeared is recorded, focused or not, so a rehearsal
    // can close it again.
    let fresh = ((try? system.standardWindows()) ?? []).filter { window in
      !before.contains { system.sameWindow($0, window) }
    }
    for window in fresh where !created.contains(where: { system.sameWindow($0, window) }) {
      created.append(window)
    }
    guard appeared, let window = opened, fresh.count == 1,
      system.sameWindow(fresh[0], window) else { throw WindowTaskProbeFailure.newWindowFailed }
    try place(window, frame)
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
  /// A window that may already show the task is checked first.
  private func show(
    _ taskID: String, in window: System.Window, alreadyShowingIsPossible: Bool
  ) throws -> Bool {
    if alreadyShowingIsPossible {
      try reader.focus(window)
      if (try? reader.copyTaskLink()) == taskID { return true }
    }
    for _ in 0..<taskNavigationAttempts {
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

  private func close(_ windows: [System.Window]) -> Bool {
    var closed = true
    for window in windows.reversed() where system.isWindowAlive(window) {
      guard system.processBirthMatches(), system.closeWindow(window),
        reader.waitFor(windowCloseTimeout, { !system.isWindowAlive(window) }) else {
        closed = false
        continue
      }
    }
    return closed
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

  /// WindowServer IDs for `windows`, in order, through the same frame-based
  /// mapping the snapshot used.
  private func windowIDs(in windows: [System.Window]) throws -> [UInt32] {
    let ids = try system.windowIDs()
    let mapped = try system.mappedWindows(ids)
    return try windows.map { window in
      guard let index = mapped.firstIndex(where: { system.sameWindow($0, window) }) else {
        throw WindowTaskProbeFailure.windowMappingChanged
      }
      return ids[index]
    }
  }
}
