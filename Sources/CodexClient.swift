import Foundation

public final class CodexClient: @unchecked Sendable {
  public static let shared = CodexClient()
  private static let daemonLabel = "com.codex.switcher"
  private static let restartWorkerLabel = "com.codex.switcher.restart-worker"

  private static let fractionalFmt: ISO8601DateFormatter = {
    let fmt = ISO8601DateFormatter()
    fmt.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
    return fmt
  }()
  private static let plainFmt: ISO8601DateFormatter = {
    let fmt = ISO8601DateFormatter()
    fmt.formatOptions = [.withInternetDateTime]
    return fmt
  }()

  public static func parseDate(_ str: String?) -> Date? {
    guard let s = str, !s.isEmpty else { return nil }
    if let d = fractionalFmt.date(from: s) { return d }
    return plainFmt.date(from: s)
  }

  public func parseDate(_ str: String?) -> Date? {
    return Self.parseDate(str)
  }

  public init() {}

  public static var codexHome: URL {
    if let env = ProcessInfo.processInfo.environment["CODEX_HOME"], !env.isEmpty {
      return URL(fileURLWithPath: env)
    }
    return FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent(".codex")
  }

  public static var statusFileURL: URL {
    return codexHome.appendingPathComponent("usage-status.json")
  }

  private static var daemonLaunchAgentURL: URL {
    FileManager.default.homeDirectoryForCurrentUser
      .appendingPathComponent("Library/LaunchAgents/com.codex.switcher.plist")
  }

  private static var restartCancellationURL: URL {
    codexHome.appendingPathComponent("recovery-runs/cancel-restart")
  }

  internal static func backgroundAutomationStopCommands(daemonPath: String) -> [[String]] {
    // Stop the producer first. An in-flight one-shot worker is cancelled by a
    // durable marker only while still pre-shutdown; killing it through
    // launchctl can strand Codex after SIGTERM and before relaunch.
    [["unload", daemonPath]]
  }

  private func setRestartCancellation(_ requested: Bool) -> Bool {
    let url = Self.restartCancellationURL
    do {
      if requested {
        try FileManager.default.createDirectory(
          at: url.deletingLastPathComponent(),
          withIntermediateDirectories: true,
          attributes: [.posixPermissions: 0o700]
        )
        try Data("cancel\n".utf8).write(to: url, options: .atomic)
        try FileManager.default.setAttributes([.posixPermissions: 0o600], ofItemAtPath: url.path)
      } else if FileManager.default.fileExists(atPath: url.path) {
        try FileManager.default.removeItem(at: url)
      }
      return true
    } catch {
      NSLog("Could not update restart cancellation marker: %@", String(describing: error))
      return false
    }
  }

  @discardableResult
  private func runLaunchctl(_ arguments: [String]) -> Bool {
    let process = Process()
    process.executableURL = URL(fileURLWithPath: "/bin/launchctl")
    process.arguments = arguments
    let output = Pipe()
    process.standardOutput = output
    process.standardError = output
    do {
      try process.run()
      _ = output.fileHandleForReading.readDataToEndOfFile()
      process.waitUntilExit()
      return process.terminationStatus == 0
    } catch {
      return false
    }
  }

  /// The background auto-switcher belongs to the menu application's
  /// lifecycle. It may survive app restarts, but not an explicit Monitor Quit.
  public func startBackgroundAutomation() {
    guard setRestartCancellation(false) else { return }
    let path = Self.daemonLaunchAgentURL.path
    guard FileManager.default.fileExists(atPath: path) else { return }
    if !runLaunchctl(["list", Self.daemonLabel]) {
      _ = runLaunchctl(["load", path])
    }
    if !runLaunchctl(["list", Self.daemonLabel]) {
      NSLog("Codex background automation did not start")
    }
  }

  /// Quit stops the producer synchronously and asks a scheduled worker to stop
  /// before shutdown. A worker already past that safe boundary is deliberately
  /// allowed to finish relaunch/recovery rather than strand Codex closed.
  @discardableResult
  public func stopBackgroundAutomation() -> Bool {
    let daemonPath = Self.daemonLaunchAgentURL.path

    guard setRestartCancellation(true) else { return false }
    // Repeat the daemon unload to close a race with a tick that was already
    // submitting a worker when Quit began.
    for attempt in 0..<3 {
      for command in Self.backgroundAutomationStopCommands(daemonPath: daemonPath) {
        _ = runLaunchctl(command)
      }

      let daemonStopped = !runLaunchctl(["list", Self.daemonLabel])
      if daemonStopped {
        if runLaunchctl(["list", Self.restartWorkerLabel]) {
          NSLog("An in-flight Codex recovery worker will finish its safe relaunch before exiting")
        }
        return true
      }
      if attempt < 2 {
        Thread.sleep(forTimeInterval: 0.1)
      }
    }

    NSLog("Codex background automation daemon is still loaded; refusing to report a clean stop")
    return false
  }

  public static var configTOMLURL: URL {
    return codexHome.appendingPathComponent("config.toml")
  }

  public static var cliExecutableURL: URL {
    let localBin = FileManager.default.homeDirectoryForCurrentUser
      .appendingPathComponent(".local/bin/codex-mon")
    if FileManager.default.fileExists(atPath: localBin.path) {
      return localBin
    }
    let devBin = FileManager.default.homeDirectoryForCurrentUser
      .appendingPathComponent("dev/openai-usage-monitor/codex-switcher/target/release/codex-mon")
    if FileManager.default.fileExists(atPath: devBin.path) {
      return devBin
    }
    return URL(fileURLWithPath: "/usr/local/bin/codex-mon")
  }

  public func isCodexAppRunning() -> Bool {
    let proc = Process()
    proc.executableURL = URL(fileURLWithPath: "/usr/bin/pgrep")
    proc.arguments = ["-f", "/Applications/ChatGPT.app/Contents/MacOS/ChatGPT"]
    let pipe = Pipe()
    proc.standardOutput = pipe
    do {
      try proc.run()
      let data = pipe.fileHandleForReading.readDataToEndOfFile()
      proc.waitUntilExit()
      return !data.isEmpty
    } catch {
      return false
    }
  }

  public func getActiveModelName() -> String? {
    guard let content = try? String(contentsOf: Self.configTOMLURL, encoding: .utf8) else {
      return "GPT-5.5"
    }
    for line in content.components(separatedBy: .newlines) {
      let trimmed = line.trimmingCharacters(in: .whitespaces)
      if trimmed.hasPrefix("model =") {
        let parts = trimmed.components(separatedBy: "=")
        if parts.count >= 2 {
          return parts[1].trimmingCharacters(in: CharacterSet(charactersIn: " \"'"))
        }
      }
    }
    return "GPT-5.5"
  }

  public func setActiveModelName(_ model: String) {
    let profileScript = Self.codexHome.appendingPathComponent("bin/codex-profile").path
    if FileManager.default.isExecutableFile(atPath: profileScript) {
      let proc = Process()
      proc.executableURL = URL(fileURLWithPath: profileScript)
      proc.arguments = ["switch", model]
      try? proc.run()
      proc.waitUntilExit()
    } else if let content = try? String(contentsOf: Self.configTOMLURL, encoding: .utf8) {
      var lines = content.components(separatedBy: .newlines)
      for (idx, line) in lines.enumerated() {
        let trimmed = line.trimmingCharacters(in: .whitespaces)
        if trimmed.hasPrefix("model =") && !trimmed.contains("model_") {
          lines[idx] = "model = \"\(model)\""
          break
        }
      }
      let updated = lines.joined(separator: "\n")
      try? updated.write(to: Self.configTOMLURL, atomically: true, encoding: .utf8)
    }
  }

  public func loadCachedSnapshot() -> MultiAccountSnapshot? {
    let url = Self.statusFileURL
    guard let data = try? Data(contentsOf: url),
      let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any]
    else {
      return nil
    }

    let tsStr = json["timestamp"] as? String ?? ""
    let timestamp = Self.parseDate(tsStr) ?? Date()

    let activeId = json["active_account_id"] as? String
    let activeEmail = json["active_email"] as? String
    let activePlan = json["active_plan"] as? String
    let fiveHour = json["five_hour_percentage"] as? Double ?? 100.0
    let weekly = json["weekly_percentage"] as? Double
    let weeklyResetTimeStr = json["weekly_reset_time"] as? String
    let weeklyResetTime = weeklyResetTimeStr.flatMap { Self.parseDate($0) }
    let weeklyResetAfterSec = json["weekly_reset_after_seconds"] as? Int
    let resetTimeStr = json["reset_time"] as? String
    let resetTime = resetTimeStr.flatMap { Self.parseDate($0) }
    let resetAfterSec = json["reset_after_seconds"] as? Int
    let credits = json["credits"] as? Int ?? 0
    let autoSwitch = json["auto_switch_enabled"] as? Bool ?? true
    let autoSwitchBizOnly = json["auto_switch_business_only"] as? Bool ?? false
    let autoSwitchBizPriority = json["auto_switch_business_priority"] as? Bool ?? false
    let autoResetWeekly = json["auto_reset_weekly_enabled"] as? Bool ?? false
    let autoResetMinRemaining = json["auto_reset_weekly_min_remaining_seconds"] as? Int ?? 0
    let autoResetState = json["auto_reset_state"] as? String ?? "disabled"
    let autoResetReason = json["auto_reset_reason"] as? String
    let autoResetLastEventStr = json["auto_reset_last_event_at"] as? String
    let autoResetLastEventAt = autoResetLastEventStr.flatMap { Self.parseDate($0) }
    let activeMultiplier = json["plan_multiplier"] as? Double ?? 1.0

    var accountsList: [AccountQuota] = []
    if let rawAccs = json["accounts"] as? [[String: Any]] {
      for a in rawAccs {
        let id = a["id"] as? String ?? "unknown"
        let name = a["name"] as? String
        let email = a["email"] as? String ?? ""
        let plan = a["plan_type"] as? String ?? "team"
        let isAct = a["is_active"] as? Bool ?? false
        let pct = a["five_hour_percentage"] as? Double ?? 100.0
        let wPct = a["weekly_percentage"] as? Double
        let wRStr = a["weekly_reset_time"] as? String
        let wRTime = wRStr.flatMap { Self.parseDate($0) }
        let wRSec = a["weekly_reset_after_seconds"] as? Int
        let rStr = a["reset_time"] as? String
        let rTime = rStr.flatMap { Self.parseDate($0) }
        let rSec = a["reset_after_seconds"] as? Int
        let cr = a["credits"] as? Int ?? 0
        let err = a["error"] as? String
        let mult = a["plan_multiplier"] as? Double ?? 1.0
        let orgName = a["organization_name"] as? String

        accountsList.append(
          AccountQuota(
            id: id,
            name: name,
            email: email,
            planType: plan,
            isCurrentActive: isAct,
            fiveHourPercentage: pct,
            weeklyPercentage: wPct,
            weeklyResetTime: wRTime,
            weeklyResetAfterSeconds: wRSec,
            models: [],
            resetTime: rTime,
            resetAfterSeconds: rSec,
            credits: cr,
            error: err,
            planMultiplier: mult,
            organizationName: orgName
          ))
      }
    }

    let appRunning = isCodexAppRunning()
    let activeModel = getActiveModelName()

    return MultiAccountSnapshot(
      timestamp: timestamp,
      activeAccountId: activeId,
      activeEmail: activeEmail,
      activePlan: activePlan,
      fiveHourPercentage: fiveHour,
      weeklyPercentage: weekly,
      weeklyResetTime: weeklyResetTime,
      weeklyResetAfterSeconds: weeklyResetAfterSec,
      resetTime: resetTime,
      resetAfterSeconds: resetAfterSec,
      credits: credits,
      autoSwitchEnabled: autoSwitch,
      autoSwitchBusinessOnly: autoSwitchBizOnly,
      autoSwitchBusinessPriority: autoSwitchBizPriority,
      autoResetWeeklyEnabled: autoResetWeekly,
      autoResetWeeklyMinRemainingSeconds: autoResetMinRemaining,
      autoResetState: autoResetState,
      autoResetReason: autoResetReason,
      autoResetLastEventAt: autoResetLastEventAt,
      isAppRunning: appRunning,
      activeModelName: activeModel,
      planMultiplier: activeMultiplier,
      accounts: accountsList
    )
  }

  public func refreshQuotas(completion: @escaping (MultiAccountSnapshot?) -> Void) {
    DispatchQueue.global(qos: .userInitiated).async {
      let bin = Self.cliExecutableURL.path
      let proc = Process()
      proc.executableURL = URL(fileURLWithPath: bin)
      proc.arguments = ["status", "--refresh"]
      let pipe = Pipe()
      proc.standardOutput = pipe
      proc.standardError = Pipe()

      do {
        try proc.run()
        _ = pipe.fileHandleForReading.readDataToEndOfFile()
        proc.waitUntilExit()
      } catch {}

      let snapshot = self.loadCachedSnapshot()
      DispatchQueue.main.async {
        completion(snapshot)
      }
    }
  }

  public func switchToAccount(id: String, completion: @escaping (Bool) -> Void) {
    DispatchQueue.global(qos: .userInitiated).async {
      let bin = Self.cliExecutableURL.path
      let proc = Process()
      proc.executableURL = URL(fileURLWithPath: bin)
      proc.arguments = ["switch", id]
      let pipe = Pipe()
      proc.standardOutput = pipe
      proc.standardError = pipe

      do {
        try proc.run()
        _ = pipe.fileHandleForReading.readDataToEndOfFile()
        proc.waitUntilExit()
        let success = proc.terminationStatus == 0
        DispatchQueue.main.async {
          completion(success)
        }
      } catch {
        DispatchQueue.main.async {
          completion(false)
        }
      }
    }
  }

  public func removeAccount(id: String, completion: @escaping (Bool) -> Void) {
    DispatchQueue.global(qos: .userInitiated).async {
      let bin = Self.cliExecutableURL.path
      let proc = Process()
      proc.executableURL = URL(fileURLWithPath: bin)
      proc.arguments = ["remove", id]
      let pipe = Pipe()
      proc.standardOutput = pipe
      proc.standardError = Pipe()
      do {
        try proc.run()
        _ = pipe.fileHandleForReading.readDataToEndOfFile()
        proc.waitUntilExit()
        let success = proc.terminationStatus == 0
        DispatchQueue.main.async {
          completion(success)
        }
      } catch {
        DispatchQueue.main.async {
          completion(false)
        }
      }
    }
  }

  public func addNewAccount(id: String, completion: @escaping (Bool, String?) -> Void) {
    DispatchQueue.global(qos: .userInitiated).async {
      let bin = Self.cliExecutableURL.path
      let proc = Process()
      proc.executableURL = URL(fileURLWithPath: bin)
      proc.arguments = ["add", id]
      let outPipe = Pipe()
      let errPipe = Pipe()
      proc.standardOutput = outPipe
      proc.standardError = errPipe
      do {
        try proc.run()
        let errData = errPipe.fileHandleForReading.readDataToEndOfFile()
        _ = outPipe.fileHandleForReading.readDataToEndOfFile()
        proc.waitUntilExit()
        let success = proc.terminationStatus == 0
        var errMsg: String? = nil
        if !success {
          errMsg = String(data: errData, encoding: .utf8)?.trimmingCharacters(
            in: .whitespacesAndNewlines)
        }
        DispatchQueue.main.async {
          completion(success, errMsg)
        }
      } catch {
        DispatchQueue.main.async {
          completion(false, error.localizedDescription)
        }
      }
    }
  }

  public func saveCurrentSession(id: String, completion: @escaping (Bool, String?) -> Void) {
    DispatchQueue.global(qos: .userInitiated).async {
      let bin = Self.cliExecutableURL.path
      let proc = Process()
      proc.executableURL = URL(fileURLWithPath: bin)
      proc.arguments = ["save-current", id]
      let outPipe = Pipe()
      let errPipe = Pipe()
      proc.standardOutput = outPipe
      proc.standardError = errPipe
      do {
        try proc.run()
        let errData = errPipe.fileHandleForReading.readDataToEndOfFile()
        _ = outPipe.fileHandleForReading.readDataToEndOfFile()
        proc.waitUntilExit()
        let success = proc.terminationStatus == 0
        var errMsg: String? = nil
        if !success {
          errMsg = String(data: errData, encoding: .utf8)?.trimmingCharacters(
            in: .whitespacesAndNewlines)
        }
        DispatchQueue.main.async {
          completion(success, errMsg)
        }
      } catch {
        DispatchQueue.main.async {
          completion(false, error.localizedDescription)
        }
      }
    }
  }

  public func restartCodexDesktopApp() {
    DispatchQueue.global(qos: .userInitiated).async {
      // Share detection, restart journaling, and verified recovery with
      // automatic switching instead of terminating the app without a snapshot.
      let proc = Process()
      proc.executableURL = Self.cliExecutableURL
      proc.arguments = ["restart"]
      let output = Pipe()
      proc.standardOutput = output
      proc.standardError = output
      do {
        try proc.run()
        _ = output.fileHandleForReading.readDataToEndOfFile()
        proc.waitUntilExit()
        if proc.terminationStatus != 0 {
          NSLog("Codex restart/recovery did not complete (exit %d)", proc.terminationStatus)
        }
      } catch {
        NSLog("Could not start Codex recovery worker")
      }
    }
  }

  public func renameAccount(
    id: String, newName: String?, completion: @escaping (Bool, String?) -> Void
  ) {
    DispatchQueue.global(qos: .userInitiated).async {
      let bin = Self.cliExecutableURL.path
      let proc = Process()
      proc.executableURL = URL(fileURLWithPath: bin)
      if let name = newName, !name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
        proc.arguments = ["rename", id, name.trimmingCharacters(in: .whitespacesAndNewlines)]
      } else {
        proc.arguments = ["rename", id, "--clear"]
      }
      let outPipe = Pipe()
      let errPipe = Pipe()
      proc.standardOutput = outPipe
      proc.standardError = errPipe
      do {
        try proc.run()
        let errData = errPipe.fileHandleForReading.readDataToEndOfFile()
        _ = outPipe.fileHandleForReading.readDataToEndOfFile()
        proc.waitUntilExit()
        let success = proc.terminationStatus == 0
        let errMsg =
          success
          ? nil
          : String(data: errData, encoding: .utf8)?.trimmingCharacters(in: .whitespacesAndNewlines)
        DispatchQueue.main.async {
          completion(success, errMsg)
        }
      } catch {
        DispatchQueue.main.async {
          completion(false, error.localizedDescription)
        }
      }
    }
  }

  public func setRestartAppOnSwitch(_ enabled: Bool, completion: ((Bool) -> Void)? = nil) {
    DispatchQueue.global(qos: .userInitiated).async {
      let bin = Self.cliExecutableURL.path
      let proc = Process()
      proc.executableURL = URL(fileURLWithPath: bin)
      proc.arguments = ["config", "--restart-app-on-switch", enabled ? "true" : "false"]
      do {
        try proc.run()
        proc.waitUntilExit()
        let success = proc.terminationStatus == 0
        DispatchQueue.main.async {
          completion?(success)
        }
      } catch {
        DispatchQueue.main.async {
          completion?(false)
        }
      }
    }
  }

  public func getRestartAppOnSwitch() -> Bool {
    let accountsPath = Self.codexHome.appendingPathComponent("accounts.json").path
    guard let data = try? Data(contentsOf: URL(fileURLWithPath: accountsPath)),
      let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
      let settings = json["settings"] as? [String: Any],
      let restart = settings["restart_app_on_switch"] as? Bool
    else {
      return false  // Default is false
    }
    return restart
  }

  public func setAutoSwitchEnabled(_ enabled: Bool, completion: ((Bool) -> Void)? = nil) {
    DispatchQueue.global(qos: .userInitiated).async {
      let bin = Self.cliExecutableURL.path
      let proc = Process()
      proc.executableURL = URL(fileURLWithPath: bin)
      proc.arguments = ["config", "--auto-switch-enabled", enabled ? "true" : "false"]
      do {
        try proc.run()
        proc.waitUntilExit()
        let success = proc.terminationStatus == 0
        DispatchQueue.main.async {
          completion?(success)
        }
      } catch {
        DispatchQueue.main.async {
          completion?(false)
        }
      }
    }
  }

  public func getAutoSwitchEnabled() -> Bool {
    let accountsPath = Self.codexHome.appendingPathComponent("accounts.json").path
    guard let data = try? Data(contentsOf: URL(fileURLWithPath: accountsPath)),
      let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
      let settings = json["settings"] as? [String: Any],
      let enabled = settings["auto_switch_enabled"] as? Bool
    else {
      return true  // Default is true
    }
    return enabled
  }

  public func setAutoSwitchBusinessOnly(_ enabled: Bool, completion: ((Bool) -> Void)? = nil) {
    DispatchQueue.global(qos: .userInitiated).async {
      let bin = Self.cliExecutableURL.path
      let proc = Process()
      proc.executableURL = URL(fileURLWithPath: bin)
      proc.arguments = ["config", "--auto-switch-business-only", enabled ? "true" : "false"]
      do {
        try proc.run()
        proc.waitUntilExit()
        let success = proc.terminationStatus == 0
        DispatchQueue.main.async {
          completion?(success)
        }
      } catch {
        DispatchQueue.main.async {
          completion?(false)
        }
      }
    }
  }

  public func getAutoSwitchBusinessOnly() -> Bool {
    let accountsPath = Self.codexHome.appendingPathComponent("accounts.json").path
    guard let data = try? Data(contentsOf: URL(fileURLWithPath: accountsPath)),
      let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
      let settings = json["settings"] as? [String: Any],
      let enabled = settings["auto_switch_business_only"] as? Bool
    else {
      return false  // Default is false
    }
    return enabled
  }

  public func setAutoSwitchBusinessPriority(_ enabled: Bool, completion: ((Bool) -> Void)? = nil) {
    DispatchQueue.global(qos: .userInitiated).async {
      let bin = Self.cliExecutableURL.path
      let proc = Process()
      proc.executableURL = URL(fileURLWithPath: bin)
      proc.arguments = ["config", "--auto-switch-business-priority", enabled ? "true" : "false"]
      do {
        try proc.run()
        proc.waitUntilExit()
        let success = proc.terminationStatus == 0
        DispatchQueue.main.async {
          completion?(success)
        }
      } catch {
        DispatchQueue.main.async {
          completion?(false)
        }
      }
    }
  }

  public func getAutoSwitchBusinessPriority() -> Bool {
    let accountsPath = Self.codexHome.appendingPathComponent("accounts.json").path
    guard let data = try? Data(contentsOf: URL(fileURLWithPath: accountsPath)),
      let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
      let settings = json["settings"] as? [String: Any],
      let enabled = settings["auto_switch_business_priority"] as? Bool
    else {
      return false  // Default is false
    }
    return enabled
  }

  /// Configures the weekly reset-credit policy in one CLI invocation so the
  /// daemon never observes only half of a menu change. The CLI validates the
  /// 0...167 hour range again before saving it atomically.
  public func setAutoResetWeekly(
    enabled: Bool,
    minRemainingHours: Int,
    completion: ((Bool) -> Void)? = nil
  ) {
    let safeHours = min(167, max(0, minRemainingHours))
    DispatchQueue.global(qos: .userInitiated).async {
      let bin = Self.cliExecutableURL.path
      let proc = Process()
      proc.executableURL = URL(fileURLWithPath: bin)
      proc.arguments = [
        "config",
        "--auto-reset-weekly-enabled", enabled ? "true" : "false",
        "--auto-reset-weekly-min-hours", String(safeHours),
      ]
      do {
        try proc.run()
        proc.waitUntilExit()
        let success = proc.terminationStatus == 0
        DispatchQueue.main.async {
          completion?(success)
        }
      } catch {
        DispatchQueue.main.async {
          completion?(false)
        }
      }
    }
  }

  public func getAutoResetWeeklyConfiguration() -> (enabled: Bool, minRemainingHours: Int) {
    let accountsPath = Self.codexHome.appendingPathComponent("accounts.json").path
    guard let data = try? Data(contentsOf: URL(fileURLWithPath: accountsPath)),
      let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
      let settings = json["settings"] as? [String: Any]
    else {
      return (false, 0)
    }
    let enabled = settings["auto_reset_weekly_enabled"] as? Bool ?? false
    let seconds = (settings["auto_reset_weekly_min_remaining_seconds"] as? NSNumber)?.intValue
      ?? 0
    return (enabled, min(167, max(0, seconds / 3600)))
  }
}
