import AppKit
import Foundation

extension AppDelegate {
  // MARK: - General Settings & Lifecycle Actions

  internal func refreshCLIVersion() {
    DispatchQueue.global(qos: .background).async { [weak self] in
      guard self != nil else { return }
      let candidatePaths = [
        "\(NSHomeDirectory())/.local/bin/cxi",
        "\(NSHomeDirectory())/.local/bin/codex-mon",
        "/opt/homebrew/bin/cxi",
        "/usr/local/bin/cxi",
      ]
      for path in candidatePaths {
        guard FileManager.default.fileExists(atPath: path) else { continue }
        let proc = Process()
        proc.executableURL = URL(fileURLWithPath: path)
        proc.arguments = ["--version"]
        let pipe = Pipe()
        proc.standardOutput = pipe
        proc.standardError = FileHandle.nullDevice
        do {
          try proc.run()
          proc.waitUntilExit()
          let data = pipe.fileHandleForReading.readDataToEndOfFile()
          if let str = String(data: data, encoding: .utf8)?.trimmingCharacters(in: .whitespacesAndNewlines),
            let ver = str.components(separatedBy: " ").last, !ver.isEmpty
          {
            DispatchQueue.main.async { [weak self] in
              self?.detectedCLIVersion = ver
              self?.updateCLIItemDisplay()
            }
            return
          }
        } catch {
          continue
        }
      }
    }
  }

  internal func updateCLIItemDisplay() {
    let title = VersionHelper.formatCLIStatusTitle(
      currentVersion: detectedCLIVersion,
      isUpdateAvailable: isCLIUpdateAvailable,
      availableVersion: availableCLIVersion
    )
    updateCLIItem?.title = title
  }

  @objc internal func updateCodexCLI() {
    refreshCLIVersion()
    refreshNow()
  }

  @objc internal func changeInterval(_ sender: NSMenuItem) {
    let newInterval = TimeInterval(sender.tag)
    refreshInterval = newInterval
    UserDefaults.standard.set(newInterval, forKey: AppDelegate.refreshIntervalKey)
    if let submenu = sender.menu {
      for item in submenu.items {
        item.state = (item.tag == sender.tag) ? .on : .off
      }
    }
    startTimer()
  }

  @objc internal func toggleRestartAppOnSwitch(_ sender: NSMenuItem) {
    let newState = sender.state != .on
    sender.state = newState ? .on : .off
    client.setRestartAppOnSwitch(newState)
  }

  @objc internal func handleRestartApp() {
    client.restartCodexDesktopApp()
  }

  @objc internal func openHelpPage() {
    let langCode = LocalizationManager.shared.currentLanguage.rawValue
    if let url = HelpsDocHelper.localizedHelpsHTMLURL(languageCode: langCode) {
      NSWorkspace.shared.open(url)
    } else if let fallbackURL = HelpsDocHelper.findHelpsHTMLURL() {
      NSWorkspace.shared.open(fallbackURL)
    }
  }

  @objc internal func handleUninstall() {
    let alert = NSAlert()
    alert.alertStyle = .critical
    alert.messageText = L10n.uninstallTitle
    alert.informativeText = L10n.uninstallMessage

    let confirmationField = NSTextField(frame: NSRect(x: 0, y: 0, width: 280, height: 24))
    confirmationField.placeholderString = L10n.uninstallConfirm
    confirmationField.setAccessibilityLabel(L10n.uninstallConfirm)
    let purgeToggle = NSButton(
      checkboxWithTitle: L10n.uninstallPurgeData, target: nil, action: nil)
    purgeToggle.setAccessibilityLabel(L10n.uninstallPurgeData)
    let accessory = NSView(frame: NSRect(x: 0, y: 0, width: 360, height: 58))
    confirmationField.frame = NSRect(x: 0, y: 32, width: 280, height: 24)
    purgeToggle.frame = NSRect(x: 0, y: 0, width: 360, height: 24)
    accessory.addSubview(confirmationField)
    accessory.addSubview(purgeToggle)
    alert.accessoryView = accessory
    alert.addButton(withTitle: L10n.uninstallConfirm)
    alert.addButton(withTitle: L10n.cancelBtn)

    NSApp.activate(ignoringOtherApps: true)
    alert.window.initialFirstResponder = confirmationField
    let response = alert.runModal()

    guard response == .alertFirstButtonReturn else { return }
    guard confirmationField.stringValue == "UNINSTALL" else {
      showAlert(title: L10n.uninstallTitle, message: L10n.uninstallInvalid, style: .warning)
      return
    }
    launchBundledUninstaller(purgeData: purgeToggle.state == .on)
  }

  private func launchBundledUninstaller(purgeData: Bool) {
    guard let scriptURL = Bundle.main.url(forResource: "uninstall", withExtension: "sh") else {
      showAlert(title: L10n.uninstallFailedTitle, message: L10n.uninstallFailedMessage, style: .warning)
      return
    }

    do {
      let script = try Data(contentsOf: scriptURL)
      guard !script.isEmpty else {
        showAlert(title: L10n.uninstallFailedTitle, message: L10n.uninstallFailedMessage, style: .warning)
        return
      }

      let input = Pipe()
      let process = Process()
      process.executableURL = URL(fileURLWithPath: "/bin/bash")
      process.arguments = ["-s", "--", "--yes"] + (purgeData ? ["--purge-data"] : [])
      process.standardInput = input
      process.standardOutput = FileHandle.nullDevice
      process.standardError = FileHandle.nullDevice
      try process.run()

      input.fileHandleForWriting.write(script)
      input.fileHandleForWriting.closeFile()
      ownsBackgroundAutomation = false
      NSApp.terminate(nil)
    } catch {
      showAlert(title: L10n.uninstallFailedTitle, message: L10n.uninstallFailedMessage, style: .warning)
    }
  }

  @objc internal func toggleLaunchAtLogin() {
    let newState = !autoLaunchManager.isEnabled
    autoLaunchManager.setEnabled(newState)
    launchAtLoginItem?.state = newState ? .on : .off
  }

  @objc internal func toggleStackPercentages() {
    let current = UserDefaults.standard.object(forKey: "stackPercentages") as? Bool ?? true
    let updated = !current
    UserDefaults.standard.set(updated, forKey: "stackPercentages")
    stackPercentagesItem?.state = updated ? .on : .off
    if let snap = lastSnapshot {
      updateStatusBar(with: snap)
    }
  }

  @objc internal func quitApp() {
    if ownsBackgroundAutomation {
      guard client.stopBackgroundAutomation() else {
        let alert = NSAlert()
        alert.alertStyle = .critical
        alert.messageText = "Could not stop Codex automation"
        alert.informativeText =
          "Codex Monitor is staying open because its background daemon or restart worker is still active."
        alert.addButton(withTitle: "OK")
        alert.runModal()
        return
      }
      ownsBackgroundAutomation = false
    }
    NSApp.terminate(nil)
  }
}
