import CoreGraphics
import Foundation

/// The helper's JSON must match what `cxi` sends and validates. Rust tests
/// read the same fixtures, so a field renamed on either side fails a test
/// instead of a restart.
@main
struct CodexWindowTaskRecordsTests {
  static let a = "01a00000-0000-4000-8000-00000000000a"
  static let b = "01a00000-0000-4000-8000-00000000000b"
  static let process = ProcessRecord(pid: 4242, birth_id: "1726789012:000007")

  static func main() {
    theRestorePlanRustSendsDecodes()
    everyHelperRecordEncodesAsRustExpects()
    malformedPlansAreRefused()
    print("Window task records passed")
  }

  static func fixture(_ name: String) -> Data {
    let url = URL(fileURLWithPath: "tests/fixtures/window-tasks/\(name)")
    guard let data = try? Data(contentsOf: url) else { fatalError("missing fixture \(name)") }
    return data
  }

  static func sameJSON<T: Encodable>(_ record: T, _ name: String) -> Bool {
    let encoded = try! JSONSerialization.jsonObject(with: JSONEncoder().encode(record)) as! NSDictionary
    let expected = try! JSONSerialization.jsonObject(with: fixture(name)) as! NSDictionary
    return encoded.isEqual(to: expected as! [AnyHashable: Any])
  }

  static func theRestorePlanRustSendsDecodes() {
    let plan = try! JSONDecoder().decode(WindowTaskRestorePlanRecord.self, from: fixture("restore-plan.json"))
    precondition(plan.windows.map { $0.task_id } == [a, b] && plan.focus_index == 1)
    precondition(plan.windows[1].frame.rect == CGRect(x: 950, y: 30, width: 900, height: 700))
    guard case .recheck(let tasks)? = restoreMode(plan) else { fatalError("recheck plan rejected") }
    precondition(tasks == [b])
  }

  static func everyHelperRecordEncodesAsRustExpects() {
    let left = CGRect(x: 0, y: 30, width: 900, height: 700)
    let right = CGRect(x: 950, y: 30, width: 900, height: 700)
    precondition(sameJSON(WindowTaskSnapshotRecord(
      process: process,
      windows: [
        WindowTaskEntryRecord(window_id: 31, frame: FrameRecord(left), task_id: a, focused: false),
        WindowTaskEntryRecord(window_id: 32, frame: FrameRecord(right), task_id: b, focused: true),
      ],
      clipboard_restored: true), "snapshot.json"))
    precondition(sameJSON(WindowTaskRestoreRecord(
      process: process, verified: [true, false], clipboard_restored: true), "restore-result.json"))
    precondition(sameJSON(WindowTaskRehearsalRecord(
      process: process, window_ids: [31, 32], verified_count: 2, clipboard_restored: false),
      "rehearsal-result.json"))
    precondition(sameJSON(WindowTaskProbeRecord(
      process: process, window_ids: [31, 32], observed_task_count: 2, clipboard_restored: true),
      "probe-result.json"))
  }

  static func malformedPlansAreRefused() {
    func plan(_ mode: String, _ recovery: [String]) -> WindowTaskRestorePlanRecord {
      WindowTaskRestorePlanRecord(windows: [], focus_index: nil, mode: mode, recovery_task_ids: recovery)
    }
    guard case .relaunch? = restoreMode(plan("relaunch", [])) else { fatalError("relaunch rejected") }
    precondition(restoreMode(plan("relaunch", [a])) == nil)
    precondition(restoreMode(plan("rebuild", [])) == nil)
    precondition(restoreMode(plan("recheck", [a, a])) == nil)
    precondition(restoreMode(plan("recheck", [a.uppercased()])) == nil)
    guard case .recheck(let none)? = restoreMode(plan("recheck", [])), none.isEmpty else {
      fatalError("an empty recheck was rejected")
    }
  }
}
