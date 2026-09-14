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
    return pid_t(expectedPID)
  }
  if let app = NSRunningApplication.runningApplications(withBundleIdentifier: "com.openai.codex").first
    ?? NSRunningApplication.runningApplications(withBundleIdentifier: "com.openai.chat").first
  {
    return app.processIdentifier
  }
  let apps = NSWorkspace.shared.runningApplications
  if let app = apps.first(where: { $0.executableURL?.lastPathComponent == "ChatGPT" }) {
    return app.processIdentifier
  }
  return nil
}

func findCodexWindowFrame(targetPID: pid_t? = nil) -> (pid: pid_t, frame: CGRect)? {
  let pid = targetPID ?? findCodexTargetPID()
  let windowInfo =
    CGWindowListCopyWindowInfo(
      [.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID)
    as? [[String: Any]] ?? []

  for info in windowInfo {
    let ownerPID = (info[kCGWindowOwnerPID as String] as? NSNumber)?.int32Value ?? 0
    let layer = (info[kCGWindowLayer as String] as? NSNumber)?.intValue ?? -1
    let alpha = (info[kCGWindowAlpha as String] as? NSNumber)?.doubleValue ?? 0
    guard layer == 0, alpha > 0 else { continue }
    if let pid = pid, ownerPID != pid { continue }
    guard let boundsValue = info[kCGWindowBounds as String],
      let frame = CGRect(dictionaryRepresentation: boundsValue as! CFDictionary),
      frame.width >= 300 && frame.height >= 300
    else { continue }
    return (pid_t(ownerPID), frame)
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

  let deadline = Date().addingTimeInterval(15)
  var targetPID: pid_t? = nil
  repeat {
    if let pid = findCodexTargetPID() {
      let windowInfo =
        CGWindowListCopyWindowInfo(
          [.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID)
        as? [[String: Any]] ?? []
      let hasWindow = windowInfo.contains { info in
        let ownerPID = (info[kCGWindowOwnerPID as String] as? NSNumber)?.int32Value ?? 0
        let layer = (info[kCGWindowLayer as String] as? NSNumber)?.intValue ?? -1
        let alpha = (info[kCGWindowAlpha as String] as? NSNumber)?.doubleValue ?? 0
        guard let boundsValue = info[kCGWindowBounds as String],
          let frame = CGRect(dictionaryRepresentation: boundsValue as! CFDictionary)
        else { return false }
        return ownerPID == pid && layer == 0 && alpha > 0 && frame.width >= 300 && frame.height >= 300
      }
      if hasWindow {
        targetPID = pid
        break
      }
    }
    _ = RunLoop.current.run(mode: .default, before: Date().addingTimeInterval(0.25))
  } while Date() < deadline

  guard let pid = targetPID else {
    print("TARGET_WINDOW_NOT_FOUND")
    exit(1)
  }

  let axApp = AXUIElementCreateApplication(pid)
  var windowsVal: AnyObject?
  guard AXUIElementCopyAttributeValue(axApp, kAXWindowsAttribute as CFString, &windowsVal) == .success,
    let winList = windowsVal as? [AXUIElement], !winList.isEmpty
  else {
    print("FAILED_TO_COPY_AX_WINDOWS")
    exit(1)
  }

  var targetWin: AXUIElement?
  for win in winList {
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
    if sz.width >= 200 && sz.height >= 200 {
      targetWin = win
      break
    }
  }

  guard let win = targetWin ?? winList.first else {
    print("NO_VALID_AX_WINDOW")
    exit(1)
  }

  var pt = targetFrame.origin
  let pVal = AXValueCreate(.cgPoint, &pt)!
  _ = AXUIElementSetAttributeValue(win, kAXPositionAttribute as CFString, pVal)

  var sz = targetFrame.size
  let sVal = AXValueCreate(.cgSize, &sz)!
  _ = AXUIElementSetAttributeValue(win, kAXSizeAttribute as CFString, sVal)

  _ = AXUIElementSetAttributeValue(win, kAXPositionAttribute as CFString, pVal)

  print(
    "WINDOW_BOUNDS_RESTORED x=\(targetFrame.origin.x) y=\(targetFrame.origin.y) width=\(targetFrame.size.width) height=\(targetFrame.size.height) pid=\(pid)"
  )
  exit(0)
}

func runAutomationBanner() -> Never {
  let taskCount = max(1, Int(argumentValue(after: "--tasks") ?? "1") ?? 1)
  let parentPID = pid_t(Int32(argumentValue(after: "--parent-pid") ?? "0") ?? 0)
  let readyFile = argumentValue(after: "--ready-file")
  guard parentPID > 1, Darwin.kill(parentPID, 0) == 0, let readyFile,
    readyFile.hasPrefix("/")
  else {
    exit(2)
  }
  let app = NSApplication.shared
  app.setActivationPolicy(.accessory)

  let width: CGFloat = 470
  let height: CGFloat = 82
  func makePanel(on screen: NSScreen) -> NSPanel {
    let panel = NSPanel(
      contentRect: NSRect(x: 0, y: 0, width: width, height: height),
      styleMask: [.borderless, .nonactivatingPanel],
      backing: .buffered,
      defer: false
    )
    panel.level = .statusBar
    panel.isOpaque = false
    panel.backgroundColor = .clear
    panel.hasShadow = true
    panel.ignoresMouseEvents = true
    panel.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .stationary]
    panel.hidesOnDeactivate = false

    let effect = NSVisualEffectView(frame: panel.contentView?.bounds ?? .zero)
    effect.autoresizingMask = [.width, .height]
    effect.material = .hudWindow
    effect.blendingMode = .behindWindow
    effect.state = .active
    effect.wantsLayer = true
    effect.layer?.cornerRadius = 14
    effect.layer?.masksToBounds = true
    effect.layer?.borderWidth = 0.6
    effect.layer?.borderColor = NSColor.white.withAlphaComponent(0.24).cgColor

    let title = NSTextField(
      labelWithString: "Codex Monitor is restoring \(taskCount) task\(taskCount == 1 ? "" : "s")")
    title.font = .systemFont(ofSize: 15, weight: .semibold)
    title.textColor = .labelColor
    title.frame = NSRect(x: 22, y: 43, width: width - 44, height: 21)

    let detail = NSTextField(
      labelWithString: "Tasks may switch briefly. You do not need to resume them manually."
    )
    detail.font = .systemFont(ofSize: 12, weight: .regular)
    detail.textColor = .secondaryLabelColor
    detail.frame = NSRect(x: 22, y: 19, width: width - 44, height: 18)

    effect.addSubview(title)
    effect.addSubview(detail)
    panel.contentView = effect
    let frame = screen.visibleFrame
    panel.setFrameOrigin(
      NSPoint(x: frame.midX - width / 2, y: frame.maxY - height - 22)
    )
    panel.alphaValue = 0.94
    return panel
  }
  let panels = NSScreen.screens.map(makePanel)
  guard !panels.isEmpty else { exit(2) }
  panels.forEach { $0.orderFrontRegardless() }
  // AppKit visibility alone is not compositor proof. Poll WindowServer until
  // every per-display panel belonging to this helper process is on-screen.
  let readinessDeadline = Date().addingTimeInterval(4)
  var compositorConfirmed = false
  repeat {
    panels.forEach { $0.orderFrontRegardless() }
    _ = RunLoop.current.run(
      mode: .default,
      before: Date().addingTimeInterval(0.1)
    )
    let expectedWindowNumbers = Set(panels.map { $0.windowNumber }.filter { $0 > 0 })
    let visibleWindowNumbers = Set(
      (CGWindowListCopyWindowInfo(
        [.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID)
        as? [[String: Any]] ?? [])
        .compactMap { info -> Int? in
          let ownerPID = (info[kCGWindowOwnerPID as String] as? NSNumber)?.int32Value
          let number = (info[kCGWindowNumber as String] as? NSNumber)?.intValue
          return ownerPID == getpid() ? number : nil
        })
    compositorConfirmed =
      expectedWindowNumbers.count == panels.count
      && expectedWindowNumbers.isSubset(of: visibleWindowNumbers)
      && panels.allSatisfy { $0.isVisible }
  } while !compositorConfirmed && Date() < readinessDeadline

  guard compositorConfirmed,
    FileManager.default.createFile(
      atPath: readyFile,
      contents: Data("visible\n".utf8),
      attributes: [.posixPermissions: 0o600]
    )
  else {
    panels.forEach { $0.orderOut(nil) }
    exit(2)
  }

  let deadline = Date().addingTimeInterval(900)
  var nextRaise = Date()
  while Date() < deadline {
    if parentPID > 1 && Darwin.kill(parentPID, 0) != 0 { break }
    if Date() >= nextRaise {
      panels.forEach { $0.orderFrontRegardless() }
      nextRaise = Date().addingTimeInterval(1)
    }
    _ = RunLoop.current.run(
      mode: .default,
      before: Date().addingTimeInterval(0.25)
    )
  }
  panels.forEach { $0.orderOut(nil) }
  exit(0)
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
  AXUIElementSetAttributeValue(axApp, "AXEnhancedUserInterface" as CFString, true as CFTypeRef)

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

if CommandLine.arguments.contains("--automation-banner") {
  runAutomationBanner()
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
