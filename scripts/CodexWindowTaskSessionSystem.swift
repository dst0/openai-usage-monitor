import CoreGraphics
import Foundation

/// What restoring windows needs beyond the probe: frames, File > New Window,
/// task links, and closing the windows a rehearsal opened.
protocol WindowTaskSessionSystem: WindowTaskProbeSystem {
  func frame(_ window: Window) -> CGRect?
  /// Moves and resizes the window; the caller reads the frame back.
  func setFrame(_ window: Window, _ frame: CGRect) -> Bool
  /// Presses File > New Window in the verified process. False when the item
  /// is missing or disabled.
  func pressNewWindow() -> Bool
  /// The verified process's focused standard window, if any.
  func focusedWindow() -> Window?
  /// Every standard window of the verified process, without the WindowServer
  /// cross-check, so windows with equal frames are still listed.
  func standardWindows() throws -> [Window]
  /// Opens `codex://threads/<taskID>` with the running ChatGPT bundle.
  func openTaskLink(_ taskID: String) -> Bool
  func closeWindow(_ window: Window) -> Bool
  func isWindowAlive(_ window: Window) -> Bool
}

/// One window captured before a restart. Frames use Accessibility's
/// top-left-origin screen coordinates.
struct WindowTaskEntry {
  let windowID: UInt32
  let frame: CGRect
  let taskID: String
  let focused: Bool
}

struct PlannedWindowTask {
  let taskID: String
  let frame: CGRect
}

struct WindowTaskRestoreResult {
  let windowIDs: [UInt32]
  let verified: [Bool]
  let clipboardRestored: Bool
}

let newWindowTimeout: TimeInterval = 5
/// Desktop reads the thread before it navigates; a cold task after a
/// relaunch can take several seconds.
let taskNavigationTimeout: TimeInterval = 12
let taskNavigationAttempts = 2
let taskNavigationPollInterval: TimeInterval = 0.5
let windowCloseTimeout: TimeInterval = 3
/// A rehearsal window is offset from its original so frames stay distinct.
let rehearsalFrameOffset: CGFloat = 40

/// Offsets a rehearsal window from its original and from every other open
/// window, so frame-based mapping never confuses them.
func rehearsalFrame(for frame: CGRect, avoiding frames: [CGRect]) -> CGRect {
  for offset in [rehearsalFrameOffset, -rehearsalFrameOffset, 2 * rehearsalFrameOffset] {
    let candidate = frame.offsetBy(dx: offset, dy: offset)
    if !frames.contains(where: { framesMatch($0, candidate) }) { return candidate }
  }
  return frame.offsetBy(dx: 3 * rehearsalFrameOffset, dy: 3 * rehearsalFrameOffset)
}

/// The same minimum the window inventory applies to a user window.
func frameIsUsable(_ frame: CGRect) -> Bool {
  frame.origin.x.isFinite && frame.origin.y.isFinite && frame.width.isFinite
    && frame.height.isFinite && frame.width >= 300 && frame.height >= 250
}
