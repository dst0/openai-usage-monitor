import CoreGraphics
import Foundation

/// The JSON the window helper exchanges with `cxi`. Field names are the
/// Rust side's contract; `tests/fixtures/window-tasks/` pins them for both.
struct ProcessRecord: Codable {
  let pid: Int32
  let birth_id: String
}

struct WindowTaskProbeRecord: Codable {
  let process: ProcessRecord
  let window_ids: [UInt32]
  let observed_task_count: Int
  let clipboard_restored: Bool
}

struct FrameRecord: Codable {
  let x: CGFloat
  let y: CGFloat
  let width: CGFloat
  let height: CGFloat

  init(_ frame: CGRect) {
    x = frame.minX
    y = frame.minY
    width = frame.width
    height = frame.height
  }

  var rect: CGRect { CGRect(x: x, y: y, width: width, height: height) }
}

struct WindowTaskEntryRecord: Codable {
  let window_id: UInt32
  let frame: FrameRecord
  let task_id: String
  let focused: Bool
}

struct WindowTaskSnapshotRecord: Codable {
  let process: ProcessRecord
  let windows: [WindowTaskEntryRecord]
  let clipboard_restored: Bool
}

struct PlannedWindowTaskRecord: Codable {
  let task_id: String
  let frame: FrameRecord
}

/// `mode` is `relaunch` or `recheck`; a recheck names recovery's tasks.
struct WindowTaskRestorePlanRecord: Codable {
  let windows: [PlannedWindowTaskRecord]
  let focus_index: Int?
  let mode: String
  let recovery_task_ids: [String]
}

struct WindowTaskRestoreRecord: Codable {
  let process: ProcessRecord
  let verified: [Bool]
  let clipboard_restored: Bool
}

struct WindowTaskRehearsalRecord: Codable {
  let process: ProcessRecord
  let window_ids: [UInt32]
  let verified_count: Int
  let clipboard_restored: Bool
}

func restoreMode(_ plan: WindowTaskRestorePlanRecord) -> WindowTaskRestoreMode? {
  let recovery = Set(plan.recovery_task_ids)
  guard recovery.count == plan.recovery_task_ids.count,
    recovery.allSatisfy({ canonicalTaskID($0) == $0 }) else { return nil }
  switch plan.mode {
  case "relaunch": return recovery.isEmpty ? .relaunch : nil
  case "recheck": return .recheck(recoveryTaskIDs: recovery)
  default: return nil
  }
}
