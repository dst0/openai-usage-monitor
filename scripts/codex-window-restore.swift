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

struct WindowInventoryRecord: Codable {
  let process: ProcessRecord
  let window_ids: [UInt32]
  let ax_standard_count: Int
  let ambiguous_count: Int
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

func copyAXValue(_ element: AXUIElement, _ attribute: String) -> AnyObject? {
  var raw: AnyObject?
  guard AXUIElementCopyAttributeValue(element, attribute as CFString, &raw) == .success else {
    return nil
  }
  return raw
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
    guard let position = decodeAXPoint(copyAXValue(window, kAXPositionAttribute as String)),
      let size = decodeAXSize(copyAXValue(window, kAXSizeAttribute as String)) else {
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
  var best: (id: UInt32, screen: NSScreen, area: CGFloat)?
  for screen in NSScreen.screens {
    guard let number = screen.deviceDescription[NSDeviceDescriptionKey("NSScreenNumber")] as? NSNumber
    else { continue }
    let visible = frame.intersection(CGDisplayBounds(number.uint32Value))
    guard !visible.isNull, visible.width > 0, visible.height > 0 else { continue }
    let area = visible.width * visible.height
    if best == nil || area > best!.area { best = (number.uint32Value, screen, area) }
  }
  guard let best else { return nil }
  let r = best.screen.frame
  return ScreenRecord(
    display_id: best.id,
    frame: Rect(x: r.origin.x, y: r.origin.y, width: r.width, height: r.height)
  )
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

/// WindowServer geometry is readable by the user LaunchAgent even when its
/// Accessibility window query is denied. This path is only for banner
/// placement; it must never be used to authorize a geometry restore.
func captureBannerWindow(_ process: (pid: pid_t, birth: String)) -> CaptureRecord {
  guard let windows = CGWindowListCopyWindowInfo(
    [.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID
  ) as? [[String: Any]] else { fail("WINDOW_ACCESS_FAILED") }
  var best: (frame: CGRect, area: CGFloat, named: Bool)?
  for info in windows {
    let frame: CGRect
    switch bannerWindowFrame(info, expectedPID: process.pid) {
    case .notCandidate: continue
    case .invalidGeometry: fail("WINDOW_GEOMETRY_FAILED")
    case .frame(let candidate): frame = candidate
    }
    let named = (info[kCGWindowName as String] as? String) == "ChatGPT"
    let area = frame.width * frame.height
    if best == nil || (named && !best!.named) || (named == best!.named && area > best!.area) {
      best = (frame, area, named)
    }
  }
  guard processBirth(process.pid) == process.birth else { fail("PROCESS_IDENTITY_REJECTED") }
  guard let frame = best?.frame else { fail("WINDOW_NOT_FOUND") }
  guard let screen = screenRecord(for: frame) else { fail("WINDOW_GEOMETRY_FAILED") }
  return CaptureRecord(
    process: ProcessRecord(pid: process.pid, birth_id: process.birth),
    frame: Rect(x: frame.minX, y: frame.minY, width: frame.width, height: frame.height),
    screen: screen
  )
}

func standardWindowFrames(_ pid: pid_t) -> [CGRect] {
  let app = AXUIElementCreateApplication(pid)
  var rawWindows: AnyObject?
  guard AXUIElementCopyAttributeValue(app, kAXWindowsAttribute as CFString, &rawWindows) == .success,
    let windows = rawWindows as? [AXUIElement] else { fail("WINDOW_ACCESS_FAILED") }
  var frames: [CGRect] = []
  for window in windows {
    var rawSubrole: AnyObject?
    guard AXUIElementCopyAttributeValue(window, kAXSubroleAttribute as CFString, &rawSubrole) == .success,
      let subrole = rawSubrole as? String else { fail("WINDOW_ACCESS_FAILED") }
    guard subrole == kAXStandardWindowSubrole as String else { continue }
    guard let position = decodeAXPoint(copyAXValue(window, kAXPositionAttribute as String)),
      let size = decodeAXSize(copyAXValue(window, kAXSizeAttribute as String)),
      size.width >= 300, size.height >= 250 else { fail("WINDOW_GEOMETRY_FAILED") }
    frames.append(CGRect(origin: position, size: size))
  }
  return frames
}

/// Reads every ChatGPT window in this process, including windows on another
/// Space. An unidentified visible layer-0 window is ambiguous; it must not be
/// treated as proof that the user had only one window open.
func countStandardWindows(_ process: (pid: pid_t, birth: String)) -> WindowInventoryRecord {
  let axFrames = standardWindowFrames(process.pid)
  guard let windows = CGWindowListCopyWindowInfo(
    [.optionAll, .excludeDesktopElements], kCGNullWindowID
  ) as? [[String: Any]] else { fail("WINDOW_ACCESS_FAILED") }
  var ids = Set<UInt32>()
  var cgFrames: [CGRect] = []
  var ambiguous = 0
  for info in windows {
    guard (info[kCGWindowOwnerPID as String] as? NSNumber)?.int32Value == process.pid,
      (info[kCGWindowLayer as String] as? NSNumber)?.intValue == 0 else { continue }
    guard let bounds = info[kCGWindowBounds as String] as? [String: Any],
      let x = (bounds["X"] as? NSNumber)?.doubleValue,
      let y = (bounds["Y"] as? NSNumber)?.doubleValue,
      let width = (bounds["Width"] as? NSNumber)?.doubleValue,
      let height = (bounds["Height"] as? NSNumber)?.doubleValue,
      x.isFinite, y.isFinite, width.isFinite, height.isFinite else {
      ambiguous += 1
      continue
    }
    guard width >= 300, height >= 250 else { continue }
    let name = info[kCGWindowName as String] as? String ?? ""
    guard (info[kCGWindowAlpha as String] as? NSNumber)?.doubleValue ?? 0 > 0 else {
      ambiguous += 1
      continue
    }
    guard name == "ChatGPT" else {
      if unidentifiedWindowIsAmbiguous(
        info, frame: CGRect(x: x, y: y, width: width, height: height), standardFrames: axFrames
      ) { ambiguous += 1 }
      continue
    }
    guard let number = info[kCGWindowNumber as String] as? NSNumber,
      number.uint64Value > 0, number.uint64Value <= UInt32.max else {
      ambiguous += 1
      continue
    }
    if !ids.insert(number.uint32Value).inserted { ambiguous += 1 }
    cgFrames.append(CGRect(x: x, y: y, width: width, height: height))
  }
  guard processBirth(process.pid) == process.birth else { fail("PROCESS_IDENTITY_REJECTED") }
  var unmatched = cgFrames
  for frame in axFrames {
    guard let index = unmatched.firstIndex(where: { framesMatch($0, frame) }) else {
      fail("WINDOW_INVENTORY_MISMATCH")
    }
    unmatched.remove(at: index)
  }
  guard unmatched.isEmpty, ambiguous == 0 else { fail("WINDOW_INVENTORY_MISMATCH") }
  return WindowInventoryRecord(
    process: ProcessRecord(pid: process.pid, birth_id: process.birth),
    window_ids: ids.sorted(), ax_standard_count: axFrames.count, ambiguous_count: ambiguous
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
  guard let value = AXValueCreate(.cgPoint, &point) else {
    fail("POSITION_WRITE_FAILED")
  }
  guard processIdentityMatches(process.pid, expectedBirth: process.birth, birthReader: processBirth) else {
    fail("PROCESS_IDENTITY_REJECTED")
  }
  guard AXUIElementSetAttributeValue(window, kAXPositionAttribute as CFString, value) == .success else {
    fail("POSITION_WRITE_FAILED")
  }
  print("{}")
}

func setSize(_ process: (pid: pid_t, birth: String), _ window: AXUIElement) {
  guard let width = argument("--width").flatMap(Double.init), let height = argument("--height").flatMap(Double.init),
    width >= 300, height >= 250 else { fail("SIZE_ARGUMENTS_INVALID") }
  var size = CGSize(width: width, height: height)
  guard let value = AXValueCreate(.cgSize, &size) else {
    fail("SIZE_WRITE_FAILED")
  }
  guard processIdentityMatches(process.pid, expectedBirth: process.birth, birthReader: processBirth) else {
    fail("PROCESS_IDENTITY_REJECTED")
  }
  guard AXUIElementSetAttributeValue(window, kAXSizeAttribute as CFString, value) == .success else {
    fail("SIZE_WRITE_FAILED")
  }
  print("{}")
}

@main
struct CodexWindowRestoreMain {
  static func main() {
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
    case "capture-banner-window":
      let process = expectedProcess()
      let data = try! JSONEncoder().encode(captureBannerWindow(process))
      FileHandle.standardOutput.write(data)
    case "count-standard-windows":
      let process = expectedProcess()
      let data = try! JSONEncoder().encode(countStandardWindows(process))
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
  }
}
