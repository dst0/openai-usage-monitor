import ApplicationServices
import Cocoa
import Darwin

func argumentValue(after flag: String) -> String? {
  guard let index = CommandLine.arguments.firstIndex(of: flag),
    index + 1 < CommandLine.arguments.count
  else { return nil }
  return CommandLine.arguments[index + 1]
}

func verifyVisibleCodex() -> Never {
  guard let rawPID = argumentValue(after: "--expected-pid"),
    let expectedPID = Int32(rawPID), expectedPID > 1,
    let app = NSRunningApplication(processIdentifier: pid_t(expectedPID)),
    ["com.openai.codex", "com.openai.chat"].contains(app.bundleIdentifier ?? "")
  else {
    print("APP_NOT_FOUND")
    exit(1)
  }

  // Purely passive window verification: NEVER activate, unhide, or raise the app.
  // The user must remain completely uninterrupted in their active application.
  let deadline = Date().addingTimeInterval(15)
  repeat {
    let windowInfo =
      CGWindowListCopyWindowInfo(
        [.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID)
      as? [[String: Any]] ?? []
    let hasOnScreenWindow = windowInfo.contains { info in
      let ownerPID = (info[kCGWindowOwnerPID as String] as? NSNumber)?.int32Value
      let layer = (info[kCGWindowLayer as String] as? NSNumber)?.intValue
      let alpha = (info[kCGWindowAlpha as String] as? NSNumber)?.doubleValue ?? 0
      guard ownerPID == expectedPID, layer == 0, alpha > 0,
        let boundsValue = info[kCGWindowBounds as String],
        let frame = CGRect(dictionaryRepresentation: boundsValue as! CFDictionary)
      else { return false }
      return frame.width >= 300 && frame.height >= 300
    }

    if hasOnScreenWindow && !app.isTerminated {
      print("APP_VISIBLE pid=\(expectedPID)")
      exit(0)
    }
    _ = RunLoop.current.run(
      mode: .default,
      before: Date().addingTimeInterval(0.2)
    )
  } while Date() < deadline && !app.isTerminated

  print("APP_NOT_VISIBLE pid=\(expectedPID)")
  exit(1)
}

func codexHomeURL() -> URL {
  if let home = ProcessInfo.processInfo.environment["CODEX_HOME"], !home.isEmpty {
    return URL(fileURLWithPath: home)
  }
  return FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent(".codex")
}

func defaultWindowBoundsURL() -> URL {
  if let custom = argumentValue(after: "--bounds-file"), !custom.isEmpty {
    return URL(fileURLWithPath: custom)
  }
  return codexHomeURL().appendingPathComponent("desktop-window.json")
}

func findCodexTargetPID() -> pid_t? {
  if let rawPID = argumentValue(after: "--expected-pid"),
    let expectedPID = Int32(rawPID), expectedPID > 1
  {
    if Darwin.kill(pid_t(expectedPID), 0) == 0 {
      return pid_t(expectedPID)
    }
  }
  if let app = NSRunningApplication.runningApplications(withBundleIdentifier: "com.openai.codex").first
    ?? NSRunningApplication.runningApplications(withBundleIdentifier: "com.openai.chat").first
  {
    return app.processIdentifier
  }
  return nil
}

func getWindowViaOsascript(pid: pid_t?) -> (pid: pid_t, frame: CGRect)? {
  let pidClause = pid != nil ? "try\nset targetProc to (first process whose unix id is \(pid!))\nend try" : ""
  let script = """
  tell application "System Events"
    set targetProc to missing value
    \(pidClause)
    if targetProc is not missing value then
      tell targetProc
        set matched to missing value
        repeat with w in windows
          try
            set subr to subrole of w
            set sz to size of w
            if subr is "AXStandardWindow" and (item 1 of sz >= 300 and item 2 of sz >= 250) then
              set matched to w
              exit repeat
            end if
          end try
        end repeat
        if matched is not missing value then
          set pos to position of matched
          set sz to size of matched
          set actualPid to unix id
          return ((item 1 of pos as integer) as text) & " " & ((item 2 of pos as integer) as text) & " " & ((item 1 of sz as integer) as text) & " " & ((item 2 of sz as integer) as text) & " " & (actualPid as text)
        end if
      end tell
    end if
    error "NO_WINDOW_FOUND"
  end tell
  """

  let proc = Process()
  proc.executableURL = URL(fileURLWithPath: "/usr/bin/osascript")
  proc.arguments = ["-e", script]
  let pipe = Pipe()
  proc.standardOutput = pipe
  proc.standardError = Pipe()
  do {
    try proc.run()
    proc.waitUntilExit()
    guard proc.terminationStatus == 0 else { return nil }
    let data = pipe.fileHandleForReading.readDataToEndOfFile()
    guard let out = String(data: data, encoding: .utf8)?.trimmingCharacters(in: .whitespacesAndNewlines) else { return nil }
    let parts = out.split(separator: " ").compactMap { Int($0) }
    if parts.count == 5 {
      return (pid_t(parts[4]), CGRect(x: parts[0], y: parts[1], width: parts[2], height: parts[3]))
    }
  } catch {}
  return nil
}

func restoreViaOsascript(pid: pid_t?, targetFrame: CGRect, deadline: Date) -> (CGPoint, CGSize, pid_t)? {
  let targetX = Int(targetFrame.origin.x.rounded())
  let targetY = Int(targetFrame.origin.y.rounded())
  let targetW = Int(targetFrame.size.width.rounded())
  let targetH = Int(targetFrame.size.height.rounded())
  let pidClause = pid != nil ? "try\nset targetProc to (first process whose unix id is \(pid!))\nend try" : ""
  let script = """
  tell application "System Events"
    set targetProc to missing value
    \(pidClause)
    if targetProc is not missing value then
      tell targetProc
        set matched to missing value
        repeat with w in windows
          try
            set subr to subrole of w
            set sz to size of w
            if subr is "AXStandardWindow" and (item 1 of sz >= 300 and item 2 of sz >= 250) then
              set matched to w
              exit repeat
            end if
          end try
        end repeat
        if matched is not missing value then
          set position of matched to {\(targetX), \(targetY)}
          delay 0.15
          set size of matched to {\(targetW), \(targetH)}
          delay 0.1
          set position of matched to {\(targetX), \(targetY)}
          set pos to position of matched
          set sz to size of matched
          set actualPid to unix id
          return ((item 1 of pos as integer) as text) & " " & ((item 2 of pos as integer) as text) & " " & ((item 1 of sz as integer) as text) & " " & ((item 2 of sz as integer) as text) & " " & (actualPid as text)
        end if
      end tell
    end if
    error "NO_WINDOW_FOUND"
  end tell
  """

  while Date() < deadline {
    let proc = Process()
    proc.executableURL = URL(fileURLWithPath: "/usr/bin/osascript")
    proc.arguments = ["-e", script]
    let pipe = Pipe()
    proc.standardOutput = pipe
    proc.standardError = Pipe()
    do {
      try proc.run()
      proc.waitUntilExit()
      if proc.terminationStatus == 0 {
        let data = pipe.fileHandleForReading.readDataToEndOfFile()
        if let out = String(data: data, encoding: .utf8)?.trimmingCharacters(in: .whitespacesAndNewlines) {
          let parts = out.split(separator: " ").compactMap { Int($0) }
          if parts.count == 5 {
            return (CGPoint(x: parts[0], y: parts[1]), CGSize(width: parts[2], height: parts[3]), pid_t(parts[4]))
          }
        }
      }
    } catch {}
    _ = RunLoop.current.run(mode: .default, before: Date().addingTimeInterval(0.5))
  }
  return nil
}

func findCodexWindowFrame(targetPID: pid_t? = nil) -> (pid: pid_t, frame: CGRect)? {
  let pid = targetPID ?? findCodexTargetPID()
  if let pid = pid {
    let axApp = AXUIElementCreateApplication(pid)
    _ = AXUIElementSetAttributeValue(axApp, "AXManualAccessibility" as CFString, true as CFTypeRef)
    var windowsVal: AnyObject?
    if AXUIElementCopyAttributeValue(axApp, kAXWindowsAttribute as CFString, &windowsVal) == .success,
      let winList = windowsVal as? [AXUIElement], !winList.isEmpty
    {
      var bestWin: (frame: CGRect, area: CGFloat)? = nil
      for win in winList {
        var minVal: AnyObject?
        if AXUIElementCopyAttributeValue(win, kAXMinimizedAttribute as CFString, &minVal) == .success,
          (minVal as? Bool) == true
        {
          continue
        }
        var subroleVal: AnyObject?
        AXUIElementCopyAttributeValue(win, kAXSubroleAttribute as CFString, &subroleVal)
        let subrole = subroleVal as? String ?? ""
        guard subrole == (kAXStandardWindowSubrole as String) || subrole.isEmpty else { continue }

        var posVal: AnyObject?
        var szVal: AnyObject?
        AXUIElementCopyAttributeValue(win, kAXPositionAttribute as CFString, &posVal)
        AXUIElementCopyAttributeValue(win, kAXSizeAttribute as CFString, &szVal)
        var pt = CGPoint.zero
        var sz = CGSize.zero
        if let pv = posVal { AXValueGetValue(pv as! AXValue, .cgPoint, &pt) }
        if let sv = szVal { AXValueGetValue(sv as! AXValue, .cgSize, &sz) }
        guard sz.width >= 400 && sz.height >= 300 else { continue }
        let area = sz.width * sz.height
        if bestWin == nil || area > bestWin!.area {
          bestWin = (CGRect(origin: pt, size: sz), area)
        }
      }
      if let best = bestWin {
        return (pid, best.frame)
      }
    }
  }

  let windowInfo =
    CGWindowListCopyWindowInfo(
      [.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID)
    as? [[String: Any]] ?? []

  var bestCandidate: (pid: pid_t, frame: CGRect, area: CGFloat, isTitleMatch: Bool)? = nil

  for info in windowInfo {
    let ownerPID = (info[kCGWindowOwnerPID as String] as? NSNumber)?.int32Value ?? 0
    let layer = (info[kCGWindowLayer as String] as? NSNumber)?.intValue ?? -1
    let alpha = (info[kCGWindowAlpha as String] as? NSNumber)?.doubleValue ?? 0
    guard layer == 0, alpha > 0 else { continue }
    if let pid = pid, ownerPID != pid { continue }
    guard let boundsValue = info[kCGWindowBounds as String],
      let frame = CGRect(dictionaryRepresentation: boundsValue as! CFDictionary),
      frame.width >= 400 && frame.height >= 300
    else { continue }

    let name = info[kCGWindowName as String] as? String ?? ""
    let isTitleMatch = (name == "ChatGPT")
    let area = frame.width * frame.height

    if let current = bestCandidate {
      if isTitleMatch && !current.isTitleMatch {
        bestCandidate = (pid_t(ownerPID), frame, area, isTitleMatch)
      } else if isTitleMatch == current.isTitleMatch && area > current.area {
        bestCandidate = (pid_t(ownerPID), frame, area, isTitleMatch)
      }
    } else {
      bestCandidate = (pid_t(ownerPID), frame, area, isTitleMatch)
    }
  }

  if let best = bestCandidate {
    return (best.pid, best.frame)
  }
  if let osascriptRes = getWindowViaOsascript(pid: pid) {
    return osascriptRes
  }
  return nil
}

func saveWindowBounds() -> Never {
  guard let (pid, frame) = findCodexWindowFrame() else {
    print("NO_WINDOW_FOUND")
    exit(1)
  }
  let targetURL = defaultWindowBoundsURL()
  let codexDir = targetURL.deletingLastPathComponent()
  try? FileManager.default.createDirectory(at: codexDir, withIntermediateDirectories: true)

  let payload: [String: Any] = [
    "version": 1,
    "x": Double(frame.origin.x),
    "y": Double(frame.origin.y),
    "width": Double(frame.size.width),
    "height": Double(frame.size.height),
    "updated_at": Int(Date().timeIntervalSince1970),
  ]

  guard let data = try? JSONSerialization.data(withJSONObject: payload, options: [.prettyPrinted])
  else {
    print("JSON_SERIALIZATION_FAILED")
    exit(1)
  }

  let tmpPath = targetURL.path + ".\(getpid()).tmp"
  FileManager.default.createFile(
    atPath: tmpPath, contents: data, attributes: [.posixPermissions: 0o600])
  let tmpURL = URL(fileURLWithPath: tmpPath)
  do {
    _ = try FileManager.default.replaceItemAt(targetURL, withItemAt: tmpURL)
  } catch {
    _ = try? FileManager.default.removeItem(at: targetURL)
    _ = try? FileManager.default.moveItem(at: tmpURL, to: targetURL)
  }
  chmod(targetURL.path, 0o600)

  print(
    "WINDOW_BOUNDS_SAVED x=\(frame.origin.x) y=\(frame.origin.y) width=\(frame.size.width) height=\(frame.size.height) pid=\(pid)"
  )
  exit(0)
}

func getWindowBounds() -> Never {
  if let (pid, frame) = findCodexWindowFrame() {
    let payload: [String: Any] = [
      "version": 1,
      "x": Double(frame.origin.x),
      "y": Double(frame.origin.y),
      "width": Double(frame.size.width),
      "height": Double(frame.size.height),
      "pid": Int(pid),
      "updated_at": Int(Date().timeIntervalSince1970),
    ]
    if let data = try? JSONSerialization.data(withJSONObject: payload, options: [.prettyPrinted]),
      let str = String(data: data, encoding: .utf8)
    {
      print(str)
      exit(0)
    }
  }

  let targetURL = defaultWindowBoundsURL()
  if let data = try? Data(contentsOf: targetURL),
    let str = String(data: data, encoding: .utf8)
  {
    print(str)
    exit(0)
  }

  print("NO_WINDOW_BOUNDS")
  exit(1)
}

func restoreWindowBounds() -> Never {
  var targetX: CGFloat?
  var targetY: CGFloat?
  var targetW: CGFloat?
  var targetH: CGFloat?

  if let sx = argumentValue(after: "--x"), let x = Double(sx),
    let sy = argumentValue(after: "--y"), let y = Double(sy),
    let sw = argumentValue(after: "--width"), let w = Double(sw),
    let sh = argumentValue(after: "--height"), let h = Double(sh)
  {
    targetX = CGFloat(x)
    targetY = CGFloat(y)
    targetW = CGFloat(w)
    targetH = CGFloat(h)
  } else {
    let targetURL = defaultWindowBoundsURL()
    guard let data = try? Data(contentsOf: targetURL),
      let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
      let x = json["x"] as? NSNumber,
      let y = json["y"] as? NSNumber,
      let w = json["width"] as? NSNumber,
      let h = json["height"] as? NSNumber
    else {
      print("NO_BOUNDS_SAVED")
      exit(0)
    }
    targetX = CGFloat(x.doubleValue)
    targetY = CGFloat(y.doubleValue)
    targetW = CGFloat(w.doubleValue)
    targetH = CGFloat(h.doubleValue)
  }

  guard let x = targetX, let y = targetY, let width = targetW, let height = targetH,
    width >= 300, height >= 300
  else {
    print("INVALID_BOUNDS")
    exit(1)
  }

  var targetFrame = CGRect(x: x, y: y, width: width, height: height)

  var displayCount: UInt32 = 0
  CGGetActiveDisplayList(0, nil, &displayCount)
  var displays = [CGDirectDisplayID](repeating: 0, count: Int(displayCount))
  CGGetActiveDisplayList(displayCount, &displays, &displayCount)
  let displayFrames = displays.map { CGDisplayBounds($0) }

  let isVisible = displayFrames.contains { display in
    let inter = display.intersection(targetFrame)
    return inter.width >= 100 && inter.height >= 100
  }

  if !isVisible, let mainDisplay = displayFrames.first {
    let safeW = min(targetFrame.width, mainDisplay.width - 40)
    let safeH = min(targetFrame.height, mainDisplay.height - 60)
    let safeX = mainDisplay.minX + max(0, (mainDisplay.width - safeW) / 2)
    let safeY = mainDisplay.minY + max(0, (mainDisplay.height - safeH) / 2)
    targetFrame = CGRect(x: safeX, y: safeY, width: safeW, height: safeH)
  }

  // Poll for the actual main Codex window up to 35 seconds.
  // On startup or restart, an initial splash or loading window may appear first
  // before the main document window opens. We wait until the standard document
  // window (AXStandardWindow / named "ChatGPT" / largest area) is available.
  let deadline = Date().addingTimeInterval(35)
  var matchedWin: AXUIElement? = nil
  var matchedPID: pid_t? = nil
  var lastErr: Int32 = 0

  while Date() < deadline {
    if let pid = findCodexTargetPID() {
      matchedPID = pid
      let axApp = AXUIElementCreateApplication(pid)
      _ = AXUIElementSetAttributeValue(axApp, "AXManualAccessibility" as CFString, true as CFTypeRef)

      var candidateWindows: [AXUIElement] = []
      var windowsVal: AnyObject?
      let err = AXUIElementCopyAttributeValue(axApp, kAXWindowsAttribute as CFString, &windowsVal)
      if err == .success, let list = windowsVal as? [AXUIElement] {
        candidateWindows.append(contentsOf: list)
      } else {
        lastErr = err.rawValue
        if err.rawValue == -25211 { // kAXErrorAPIDisabled
          if let res = restoreViaOsascript(pid: pid, targetFrame: targetFrame, deadline: deadline) {
            print("WINDOW_BOUNDS_RESTORED x=\(res.0.x) y=\(res.0.y) width=\(res.1.width) height=\(res.1.height) pid=\(res.2)")
            exit(0)
          }
          print("FAILED_TO_COPY_AX_WINDOWS err=-25211")
          exit(1)
        }
      }

      var mainWinVal: AnyObject?
      if AXUIElementCopyAttributeValue(axApp, kAXMainWindowAttribute as CFString, &mainWinVal) == .success,
        let mw = mainWinVal
      {
        let mainEl = mw as! AXUIElement
        if !candidateWindows.contains(where: { CFEqual($0, mainEl) }) {
          candidateWindows.append(mainEl)
        }
      }

      var focusedWinVal: AnyObject?
      if AXUIElementCopyAttributeValue(axApp, kAXFocusedWindowAttribute as CFString, &focusedWinVal) == .success,
        let fw = focusedWinVal
      {
        let focusedEl = fw as! AXUIElement
        if !candidateWindows.contains(where: { CFEqual($0, focusedEl) }) {
          candidateWindows.append(focusedEl)
        }
      }

      var bestStandardWin: AXUIElement?
      var maxStandardArea: CGFloat = 0
      var fallbackWin: AXUIElement?
      var maxFallbackArea: CGFloat = 0

      for win in candidateWindows {
        var minVal: AnyObject?
        if AXUIElementCopyAttributeValue(win, kAXMinimizedAttribute as CFString, &minVal) == .success,
          (minVal as? Bool) == true
        {
          continue
        }
        var szVal: AnyObject?
        AXUIElementCopyAttributeValue(win, kAXSizeAttribute as CFString, &szVal)
        var sz = CGSize.zero
        if let sv = szVal { AXValueGetValue(sv as! AXValue, .cgSize, &sz) }
        guard sz.width >= 300 && sz.height >= 250 else { continue }

        let area = sz.width * sz.height
        var subroleVal: AnyObject?
        AXUIElementCopyAttributeValue(win, kAXSubroleAttribute as CFString, &subroleVal)
        let subrole = subroleVal as? String ?? ""

        var titleVal: AnyObject?
        AXUIElementCopyAttributeValue(win, kAXTitleAttribute as CFString, &titleVal)
        let title = titleVal as? String ?? ""

        let isStandard = (subrole == (kAXStandardWindowSubrole as String)) || title == "ChatGPT"

        if isStandard {
          if area > maxStandardArea {
            maxStandardArea = area
            bestStandardWin = win
          }
        }
        if area > maxFallbackArea {
          maxFallbackArea = area
          fallbackWin = win
        }
      }

      // If we found the real standard window, or if at least 15 seconds have passed
      // and we have a fallback window with substantial area (>= 500x400):
      if let win = bestStandardWin {
        matchedWin = win
        break
      } else if let fallback = fallbackWin, maxFallbackArea >= 500 * 400,
        deadline.timeIntervalSinceNow < 20
      {
        matchedWin = fallback
        break
      }
    }
    _ = RunLoop.current.run(mode: .default, before: Date().addingTimeInterval(0.3))
  }

  if let win = matchedWin, let pid = matchedPID {
    var pt = targetFrame.origin
    let pVal = AXValueCreate(.cgPoint, &pt)!
    var sz = targetFrame.size
    let sVal = AXValueCreate(.cgSize, &sz)!

    // Multi-display reliable positioning (avoids WindowServer scale/display snapping):
    // 1. Move to target display
    _ = AXUIElementSetAttributeValue(win, kAXPositionAttribute as CFString, pVal)
    usleep(100_000)

    // 2. Set size on target display
    _ = AXUIElementSetAttributeValue(win, kAXSizeAttribute as CFString, sVal)
    usleep(50_000)

    // 3. Re-apply target position
    _ = AXUIElementSetAttributeValue(win, kAXPositionAttribute as CFString, pVal)
    usleep(50_000)

    // 4. Final size pass
    _ = AXUIElementSetAttributeValue(win, kAXSizeAttribute as CFString, sVal)

    var resPt = targetFrame.origin
    var resSz = targetFrame.size
    var posVal: AnyObject?
    var szVal: AnyObject?
    if AXUIElementCopyAttributeValue(win, kAXPositionAttribute as CFString, &posVal) == .success,
      let pv = posVal
    {
      AXValueGetValue(pv as! AXValue, .cgPoint, &resPt)
    }
    if AXUIElementCopyAttributeValue(win, kAXSizeAttribute as CFString, &szVal) == .success,
      let sv = szVal
    {
      AXValueGetValue(sv as! AXValue, .cgSize, &resSz)
    }

    let sizeDiff = abs(resSz.width - targetFrame.size.width) + abs(resSz.height - targetFrame.size.height)
    let posDiff = abs(resPt.x - targetFrame.origin.x) + abs(resPt.y - targetFrame.origin.y)
    if sizeDiff <= 60 && posDiff <= 60 {
      print(
        "WINDOW_BOUNDS_RESTORED x=\(resPt.x) y=\(resPt.y) width=\(resSz.width) height=\(resSz.height) pid=\(pid)"
      )
      exit(0)
    }
  }

  // Fallback: Use AppleScript via System Events if AX did not find the window
  if let res = restoreViaOsascript(pid: matchedPID, targetFrame: targetFrame, deadline: Date().addingTimeInterval(10)) {
    print("WINDOW_BOUNDS_RESTORED x=\(res.0.x) y=\(res.0.y) width=\(res.1.width) height=\(res.1.height) pid=\(res.2)")
    exit(0)
  }

  guard let _ = matchedPID else {
    print("TARGET_PROCESS_NOT_FOUND")
    exit(1)
  }

  print("FAILED_TO_COPY_AX_WINDOWS err=\(lastErr)")
  exit(1)
}

struct CandidateButton {
  let element: AXUIElement
  let isPlay: Bool
  let isSteer: Bool
  let isResume: Bool
  let y: CGFloat
}

/// Identifies the non-functional text button inside the "Queue paused because you interrupted" banner
func isBannerResume(title: String, desc: String, width: CGFloat, height: CGFloat) -> Bool {
  let t = title.lowercased().trimmingCharacters(in: .whitespacesAndNewlines)
  let d = desc.lowercased().trimmingCharacters(in: .whitespacesAndNewlines)
  if (t == "resume" || t == "возобновить") && d.isEmpty && width > 50 {
    return true
  }
  return false
}

/// Identifies the circular "Play" button at the bottom-right of the composer (white right-facing triangle)
func isPlayButton(title: String, desc: String, width: CGFloat, height: CGFloat) -> Bool {
  let t = title.lowercased().trimmingCharacters(in: .whitespacesAndNewlines)
  let d = desc.lowercased().trimmingCharacters(in: .whitespacesAndNewlines)
  if (d == "resume" || d == "возобновить" || d == "play" || d == "start")
    && (t.isEmpty || t == "▶" || t == ">") && width <= 45 && height <= 45
  {
    return true
  }
  return false
}

func isResumeButton(title: String, desc: String) -> Bool {
  let t = title.lowercased().trimmingCharacters(in: .whitespacesAndNewlines)
  let d = desc.lowercased().trimmingCharacters(in: .whitespacesAndNewlines)
  if t == "resume" || d == "resume" || t == "retry" || d == "retry" || t == "возобновить"
    || d == "возобновить" || t == "повторить" || d == "повторить"
  {
    return true
  }
  if t == "try again" || d == "try again" || t.starts(with: "try again")
    || d.starts(with: "try again")
  {
    return true
  }
  if t == "continue generating" || d == "continue generating"
    || t.starts(with: "continue generating") || d.starts(with: "continue generating")
    || t == "продолжить" || d == "продолжить" || t.starts(with: "продолжить")
  {
    return true
  }
  if d.contains("resume") || d.contains("try sending this queued message again") {
    return true
  }
  if t.starts(with: "resume") || t.starts(with: "retry") || t.starts(with: "возобновить")
    || t.starts(with: "повторить")
  {
    return true
  }
  return false
}

func isSteerButton(title: String, desc: String) -> Bool {
  let t = title.lowercased().trimmingCharacters(in: .whitespacesAndNewlines)
  let d = desc.lowercased().trimmingCharacters(in: .whitespacesAndNewlines)
  if t == "steer" || d == "steer" || t == "направить" || d == "направить" {
    return true
  }
  if d.contains("submit without interrupting") || d.contains("steer") {
    return true
  }
  return false
}

func isGeneratingButton(desc: String, title: String) -> Bool {
  let d = desc.lowercased().trimmingCharacters(in: .whitespacesAndNewlines)
  let t = title.lowercased().trimmingCharacters(in: .whitespacesAndNewlines)
  return d == "stop" || t == "stop" || d == "остановить" || d == "зупинити"
}

func pressButton(el: AXUIElement) -> Bool {
  // AXPress is the canonical activation path. Do not also synthesize a mouse
  // click when it succeeds: after Resume, the same control can immediately
  // become Stop, so a second click races with hydration and cancels the turn.
  if AXUIElementPerformAction(el, kAXPressAction as CFString) == .success {
    return true
  }
  // An AX error is not permission to click stale screen coordinates.
  return false
}

/// Searches the Accessibility hierarchy of ChatGPT / Codex and performs
/// the circular `Play` action, `Steer` action, or turn `Resume` on any interrupted session.
func resumeChatGPT() -> (success: Bool, outcome: String) {
  guard let titleIndex = CommandLine.arguments.firstIndex(of: "--expected-title"),
    titleIndex + 1 < CommandLine.arguments.count
  else { return (false, "MISSING_TARGET") }
  let expectedTitle = CommandLine.arguments[titleIndex + 1]
  guard !expectedTitle.isEmpty else { return (false, "MISSING_TARGET") }
  guard
    let app = NSRunningApplication.runningApplications(withBundleIdentifier: "com.openai.codex")
      .first
      ?? NSRunningApplication.runningApplications(withBundleIdentifier: "com.openai.chat").first
  else {
    return (false, "APP_NOT_FOUND")
  }

  app.activate(options: .activateAllWindows)
  usleep(150000)

  let axApp = AXUIElementCreateApplication(app.processIdentifier)
  AXUIElementSetAttributeValue(axApp, "AXManualAccessibility" as CFString, true as CFTypeRef)

  var windows: AnyObject?
  guard AXUIElementCopyAttributeValue(axApp, kAXWindowsAttribute as CFString, &windows) == .success,
    let winList = windows as? [AXUIElement]
  else {
    return (false, "NO_WINDOWS")
  }

  var candidates: [CandidateButton] = []
  var isAlreadyGenerating = false
  var matchedTitle = false
  var windowBottom: CGFloat = 0

  func collectButtons(el: AXUIElement, depth: Int = 0) {
    if depth > 75 { return }
    var role: AnyObject?
    AXUIElementCopyAttributeValue(el, kAXRoleAttribute as CFString, &role)
    var desc: AnyObject?
    AXUIElementCopyAttributeValue(el, kAXDescriptionAttribute as CFString, &desc)
    var title: AnyObject?
    AXUIElementCopyAttributeValue(el, kAXTitleAttribute as CFString, &title)

    let r = (role as? String) ?? ""
    let d = (desc as? String) ?? ""
    let t = (title as? String) ?? ""
    if r == "AXWebArea" && t == expectedTitle { matchedTitle = true }

    if r == "AXButton" || r.contains("Button") {
      var enabledVal: AnyObject?
      if AXUIElementCopyAttributeValue(el, kAXEnabledAttribute as CFString, &enabledVal)
        == .success,
        let en = enabledVal as? Bool, !en
      {
        // skip disabled buttons
      } else {
        var posVal: AnyObject?
        AXUIElementCopyAttributeValue(el, kAXPositionAttribute as CFString, &posVal)
        var sizeVal: AnyObject?
        AXUIElementCopyAttributeValue(el, kAXSizeAttribute as CFString, &sizeVal)
        var pt = CGPoint.zero
        var sz = CGSize.zero
        if let pv = posVal { AXValueGetValue(pv as! AXValue, .cgPoint, &pt) }
        if let sv = sizeVal { AXValueGetValue(sv as! AXValue, .cgSize, &sz) }

        if isGeneratingButton(desc: d, title: t) && sz.width < 45 && sz.height < 45 {
          isAlreadyGenerating = true
        }

        // Skip the non-functional "Queue paused because you interrupted [Resume]" banner button
        if isBannerResume(title: t, desc: d, width: sz.width, height: sz.height) {
          // Do not add banner button
        } else {
          let play = isPlayButton(title: t, desc: d, width: sz.width, height: sz.height)
          let steer = isSteerButton(title: t, desc: d)
          let resume = isResumeButton(title: t, desc: d)

          if play || steer || resume {
            if sz.width >= 16 && sz.height >= 16 {
              candidates.append(
                CandidateButton(
                  element: el,
                  isPlay: play,
                  isSteer: steer,
                  isResume: resume,
                  y: pt.y
                ))
            }
          }
        }
      }
    }

    var children: AnyObject?
    if AXUIElementCopyAttributeValue(el, kAXChildrenAttribute as CFString, &children) == .success,
      let childList = children as? [AXUIElement]
    {
      for c in childList {
        collectButtons(el: c, depth: depth + 1)
      }
    }
  }

  // A deep link targets the frontmost Codex window. Restrict inspection to
  // that window so a running turn in another window cannot make this thread
  // look ALREADY_ACTIVE. Falling back to AXMain keeps this working when AX
  // has not published AXFocusedWindow yet.
  var targetWindows: [AXUIElement] = []
  var focusedWindowValue: AnyObject?
  if AXUIElementCopyAttributeValue(
    axApp, kAXFocusedWindowAttribute as CFString, &focusedWindowValue) == .success,
    let focusedWindowValue
  {
    targetWindows = [focusedWindowValue as! AXUIElement]
  }
  if targetWindows.isEmpty {
    for win in winList {
      var mainValue: AnyObject?
      if AXUIElementCopyAttributeValue(win, kAXMainAttribute as CFString, &mainValue) == .success,
        let isMain = mainValue as? Bool, isMain
      {
        targetWindows = [win]
        break
      }
    }
  }
  if targetWindows.isEmpty { return (false, "NO_FOCUSED_WINDOW") }

  for win in targetWindows {
    var posVal: AnyObject?
    AXUIElementCopyAttributeValue(win, kAXPositionAttribute as CFString, &posVal)
    var sizeVal: AnyObject?
    AXUIElementCopyAttributeValue(win, kAXSizeAttribute as CFString, &sizeVal)
    var pt = CGPoint.zero
    var sz = CGSize.zero
    if let pv = posVal { AXValueGetValue(pv as! AXValue, .cgPoint, &pt) }
    if let sv = sizeVal { AXValueGetValue(sv as! AXValue, .cgSize, &sz) }
    var minimizedValue: AnyObject?
    let isMinimized =
      AXUIElementCopyAttributeValue(win, kAXMinimizedAttribute as CFString, &minimizedValue)
      == .success
      && (minimizedValue as? Bool) == true
    // AX coordinates may be negative on displays left of the primary one.
    if !isMinimized && sz.width > 300 && sz.height > 300 {
      windowBottom = pt.y + sz.height
      collectButtons(el: win)
    }
  }

  if CommandLine.arguments.contains("--inspect") {
    let playCount = candidates.filter { $0.isPlay }.count
    let steerCount = candidates.filter { $0.isSteer }.count
    let resumeCount = candidates.filter { $0.isResume }.count
    print(
      "INSPECT matched=\(matchedTitle) generating=\(isAlreadyGenerating) candidates=\(candidates.count) play=\(playCount) steer=\(steerCount) resume=\(resumeCount)"
    )
    return (false, "INSPECT_ONLY")
  }
  guard matchedTitle, app.isActive else { return (false, "TARGET_NOT_MOUNTED") }

  // Sort descending by Y so bottom-most active controls take priority over scrollback history
  candidates.sort { $0.y > $1.y }

  // Bound by the window, not the lowest historical retry button in scrollback.
  let activeZone = candidates.filter { $0.y >= windowBottom - 350 && $0.y < windowBottom }
  let preferSteer = CommandLine.arguments.contains("--prefer-steer")

  if isAlreadyGenerating {
    print("ALREADY_ACTIVE")
    return (true, "ALREADY_ACTIVE")
  }

  // Quota-completed turns need a new queued `continue`, so consume that row
  // first. Restart-aborted turns use the user's native circular Play path and
  // do not receive an injected message.
  if preferSteer, let steerTarget = activeZone.first(where: { $0.isSteer }) {
    if pressButton(el: steerTarget.element) {
      print("RESUMED_VIA_STEER")
      return (true, "RESUMED_VIA_STEER")
    }
  }

  if let playTarget = activeZone.first(where: { $0.isPlay }) {
    if pressButton(el: playTarget.element) {
      print("RESUMED_VIA_PLAY_BUTTON")
      return (true, "RESUMED_VIA_PLAY_BUTTON")
    }
  }

  if !preferSteer, let steerTarget = activeZone.first(where: { $0.isSteer }) {
    if pressButton(el: steerTarget.element) {
      print("RESUMED_VIA_STEER")
      return (true, "RESUMED_VIA_STEER")
    }
  }

  // Native turn Resume/Retry button.
  if let resumeTarget = activeZone.first(where: { $0.isResume }) {
    if pressButton(el: resumeTarget.element) {
      print("RESUMED_VIA_TURN_RESUME")
      return (true, "RESUMED_VIA_TURN_RESUME")
    }
  }

  return (false, "NOT_FOUND")
}

if CommandLine.arguments.contains("--verify-visible") {
  verifyVisibleCodex()
}
if CommandLine.arguments.contains("--save-window-bounds") {
  saveWindowBounds()
}
if CommandLine.arguments.contains("--restore-window-bounds") {
  restoreWindowBounds()
}
if CommandLine.arguments.contains("--get-window-bounds") {
  getWindowBounds()
}

let result = resumeChatGPT()
print(result.outcome)
exit(result.success ? 0 : 1)
