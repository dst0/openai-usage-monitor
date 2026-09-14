import Cocoa

extension AppDelegate {

  // MARK: - Desktop App Window Tracking

  internal func setupAppDeactivationObserver() {
    appDeactivateObserver = NSWorkspace.shared.notificationCenter.addObserver(
      forName: NSWorkspace.didDeactivateApplicationNotification,
      object: nil,
      queue: .main
    ) { [weak self] note in
      guard let app = note.userInfo?[NSWorkspace.applicationUserInfoKey] as? NSRunningApplication,
        let bundleID = app.bundleIdentifier,
        bundleID == "com.openai.codex" || bundleID == "com.openai.chat"
          || app.executableURL?.lastPathComponent == "ChatGPT"
      else { return }
      self?.saveDesktopWindowBoundsPassive(for: app.processIdentifier)
    }
  }

  internal func removeAppDeactivationObserver() {
    if let obs = appDeactivateObserver {
      NSWorkspace.shared.notificationCenter.removeObserver(obs)
      appDeactivateObserver = nil
    }
  }

  internal func saveDesktopWindowBoundsPassive(for targetPID: pid_t) {
    let home = CodexClient.codexHome
    if let data = try? Data(contentsOf: home.appendingPathComponent("accounts.json")),
      let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
      let settings = json["settings"] as? [String: Any],
      (settings["preserve_window_bounds_on_restart"] as? Bool) == false { return }

    // Do NOT overwrite saved bounds while an operation (switch, restart, recovery) is active
    let lockURL = home.appendingPathComponent("desktop-recovery.lock")
    let fd = open(lockURL.path, O_RDWR | O_CREAT, 0o600)
    if fd >= 0 {
      let isLocked = flock(fd, LOCK_EX | LOCK_NB) != 0
      close(fd)
      if isLocked { return }
    }

    let cooldownURL = home.appendingPathComponent("desktop-automation-cooldown")
    if let str = try? String(contentsOf: cooldownURL, encoding: .utf8),
      let deadlineMs = Double(str.trimmingCharacters(in: .whitespacesAndNewlines)) {
      let nowMs = Date().timeIntervalSince1970 * 1000.0
      if nowMs < deadlineMs { return }
    }

    let list = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID) as? [[String: Any]] ?? []
    var best: (frame: CGRect, area: CGFloat, isTitle: Bool)?

    for info in list {
      let ownerPID = (info[kCGWindowOwnerPID as String] as? NSNumber)?.int32Value ?? 0
      let layer = (info[kCGWindowLayer as String] as? NSNumber)?.intValue ?? -1
      let alpha = (info[kCGWindowAlpha as String] as? NSNumber)?.doubleValue ?? 0
      guard ownerPID == targetPID, layer == 0, alpha > 0 else { continue }
      guard let boundsVal = info[kCGWindowBounds as String],
        let frame = CGRect(dictionaryRepresentation: boundsVal as! CFDictionary),
        frame.width >= 400 && frame.height >= 300 else { continue }

      let isTitle = (info[kCGWindowName as String] as? String) == "ChatGPT"
      let area = frame.width * frame.height
      if best == nil || (isTitle && !best!.isTitle) || (isTitle == best!.isTitle && area > best!.area) {
        best = (frame, area, isTitle)
      }
    }
    guard let frame = best?.frame else { return }

    let targetURL = home.appendingPathComponent("desktop-window.json")
    let payload: [String: Any] = [
      "version": 1, "x": Double(frame.origin.x), "y": Double(frame.origin.y),
      "width": Double(frame.size.width), "height": Double(frame.size.height),
      "updated_at": Int(Date().timeIntervalSince1970),
    ]
    guard let data = try? JSONSerialization.data(withJSONObject: payload, options: [.prettyPrinted]) else { return }
    let tmpPath = targetURL.path + ".\(getpid()).tmp"
    FileManager.default.createFile(atPath: tmpPath, contents: data, attributes: [.posixPermissions: 0o600])
    let tmpURL = URL(fileURLWithPath: tmpPath)
    _ = try? FileManager.default.replaceItemAt(targetURL, withItemAt: tmpURL)
    chmod(targetURL.path, 0o600)
  }
}
