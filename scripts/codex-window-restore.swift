import ApplicationServices
import Cocoa
import Darwin

struct ProcessRecord: Codable {
  let pid: Int32
  let birth_id: String
}

struct Rect: Codable {
  let x: CGFloat
  let y: CGFloat
  let width: CGFloat
  let height: CGFloat
}

struct ScreenRecord: Codable {
  let display_id: UInt32
  let frame: Rect
}

struct CaptureRecord: Codable {
  let process: ProcessRecord
  let frame: Rect
  let screen: ScreenRecord
}

func argument(_ name: String) -> String? {
  guard let index = CommandLine.arguments.firstIndex(of: name), index + 1 < CommandLine.arguments.count else { return nil }
  return CommandLine.arguments[index + 1]
}

func fail(_ message: String) -> Never {
  fputs("\(message)\n", stderr)
  exit(1)
}

func processBirth(_ pid: pid_t) -> String? {
  var info = proc_bsdinfo()
  let size = Int32(MemoryLayout<proc_bsdinfo>.size)
  let written = withUnsafeMutablePointer(to: &info) {
    proc_pidinfo(pid, PROC_PIDTBSDINFO, 0, $0, size)
  }
  guard written == size, info.pbi_status != UInt32(SZOMB) else { return nil }
  return "\(info.pbi_start_tvsec):\(String(format: "%06llu", info.pbi_start_tvusec))"
}

func expectedProcess(requireBirth: Bool = true) -> (pid: pid_t, birth: String) {
  guard let rawPID = argument("--expected-pid"), let value = Int32(rawPID), value > 1,
    let birth = processBirth(pid_t(value)) else {
    fail("PROCESS_IDENTITY_REJECTED")
  }
  if requireBirth {
    guard let rawBirth = argument("--expected-birth"), !rawBirth.isEmpty, birth == rawBirth else {
      fail("PROCESS_IDENTITY_REJECTED")
    }
  } else if let rawBirth = argument("--expected-birth"), rawBirth != birth {
    fail("PROCESS_IDENTITY_REJECTED")
  }
  return (pid_t(value), birth)
}

func axValue<T>(_ element: AXUIElement, _ attribute: String, _ type: AXValueType, _ value: inout T) -> Bool {
  var raw: AnyObject?
  guard AXUIElementCopyAttributeValue(element, attribute as CFString, &raw) == .success,
    let raw else { return false }
  let rawValue = raw as! AXValue
  return AXValueGetValue(rawValue, type, &value)
}

func mainWindow(_ pid: pid_t) -> (element: AXUIElement, frame: CGRect)? {
  let app = AXUIElementCreateApplication(pid)
  var rawWindows: AnyObject?
  let status = AXUIElementCopyAttributeValue(app, kAXWindowsAttribute as CFString, &rawWindows)
  guard status == .success else { fail("WINDOW_ACCESS_FAILED") }
  guard let windows = rawWindows as? [AXUIElement] else { fail("WINDOW_ACCESS_FAILED") }
  var best: (element: AXUIElement, frame: CGRect, area: CGFloat)?
  for window in windows {
    var rawSubrole: AnyObject?
    let subroleStatus = AXUIElementCopyAttributeValue(window, kAXSubroleAttribute as CFString, &rawSubrole)
    guard subroleStatus == .success else { fail("WINDOW_ACCESS_FAILED") }
    guard let subrole = rawSubrole as? String else { fail("WINDOW_ACCESS_FAILED") }
    guard subrole == kAXStandardWindowSubrole as String else { continue }
    var minimizedValue: AnyObject?
    if AXUIElementCopyAttributeValue(window, kAXMinimizedAttribute as CFString, &minimizedValue) == .success,
      (minimizedValue as? Bool) == true { continue }
    var position = CGPoint.zero
    var size = CGSize.zero
    guard axValue(window, kAXPositionAttribute as String, .cgPoint, &position),
      axValue(window, kAXSizeAttribute as String, .cgSize, &size) else {
      fail("WINDOW_GEOMETRY_FAILED")
    }
    guard size.width >= 300, size.height >= 250 else { continue }
    let frame = CGRect(origin: position, size: size)
    let area = size.width * size.height
    if best == nil || area > best!.area { best = (window, frame, area) }
  }
  return best.map { ($0.element, $0.frame) }
}

func screenRecord(for frame: CGRect) -> ScreenRecord? {
  let screens = NSScreen.screens
  let center = CGPoint(x: frame.midX, y: frame.midY)
  let screen = screens.first(where: { $0.frame.contains(center) })
    ?? screens.min(by: { distance($0.frame, center) < distance($1.frame, center) })
  guard let screen,
    let number = screen.deviceDescription[NSDeviceDescriptionKey("NSScreenNumber")] as? NSNumber else { return nil }
  let r = screen.frame
  return ScreenRecord(
    display_id: number.uint32Value,
    frame: Rect(x: r.origin.x, y: r.origin.y, width: r.width, height: r.height)
  )
}

func distance(_ rect: CGRect, _ point: CGPoint) -> CGFloat {
  let dx = max(rect.minX - point.x, 0, point.x - rect.maxX)
  let dy = max(rect.minY - point.y, 0, point.y - rect.maxY)
  return dx * dx + dy * dy
}

func capture(_ process: (pid: pid_t, birth: String)) -> CaptureRecord {
  guard let window = mainWindow(process.pid) else { fail("WINDOW_NOT_FOUND") }
  guard let screen = screenRecord(for: window.frame) else { fail("WINDOW_GEOMETRY_FAILED") }
  let frame = window.frame
  return CaptureRecord(
    process: ProcessRecord(pid: process.pid, birth_id: process.birth),
    frame: Rect(x: frame.origin.x, y: frame.origin.y, width: frame.width, height: frame.height),
    screen: screen
  )
}

func verifyAndWindow() -> ((pid: pid_t, birth: String), AXUIElement) {
  let process = expectedProcess()
  guard let window = mainWindow(process.pid) else { fail("WINDOW_NOT_FOUND") }
  return (process, window.element)
}

func setPosition(_ process: (pid: pid_t, birth: String), _ window: AXUIElement) {
  guard let x = argument("--x").flatMap(Double.init), let y = argument("--y").flatMap(Double.init) else {
    fail("POSITION_ARGUMENTS_INVALID")
  }
  var point = CGPoint(x: x, y: y)
  guard let value = AXValueCreate(.cgPoint, &point),
    AXUIElementSetAttributeValue(window, kAXPositionAttribute as CFString, value) == .success else {
    fail("POSITION_WRITE_FAILED")
  }
  print("{}")
}

func setSize(_ process: (pid: pid_t, birth: String), _ window: AXUIElement) {
  guard let width = argument("--width").flatMap(Double.init), let height = argument("--height").flatMap(Double.init),
    width >= 300, height >= 250 else { fail("SIZE_ARGUMENTS_INVALID") }
  var size = CGSize(width: width, height: height)
  guard let value = AXValueCreate(.cgSize, &size),
    AXUIElementSetAttributeValue(window, kAXSizeAttribute as CFString, value) == .success else {
    fail("SIZE_WRITE_FAILED")
  }
  print("{}")
}

let command = CommandLine.arguments.dropFirst().first ?? ""
switch command {
case "inspect-process":
  let process = expectedProcess(requireBirth: false)
  let data = try! JSONEncoder().encode(ProcessRecord(pid: process.pid, birth_id: process.birth))
  FileHandle.standardOutput.write(data)
case "capture-window", "read-window":
  let process = expectedProcess()
  let record = capture(process)
  let data = try! JSONEncoder().encode(record)
  FileHandle.standardOutput.write(data)
case "set-position":
  let (process, window) = verifyAndWindow()
  setPosition(process, window)
case "set-size":
  let (process, window) = verifyAndWindow()
  setSize(process, window)
default:
  fail("COMMAND_REJECTED")
}
