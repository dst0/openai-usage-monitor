import Foundation

public final class CodexClient: @unchecked Sendable {
    public static let shared = CodexClient()

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
              let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            return nil
        }

        let tsStr = json["timestamp"] as? String ?? ""
        let timestamp = Self.parseDate(tsStr) ?? Date()

        let activeId = json["active_account_id"] as? String
        let activeEmail = json["active_email"] as? String
        let activePlan = json["active_plan"] as? String
        let fiveHour = json["five_hour_percentage"] as? Double ?? 100.0
        let weekly = json["weekly_percentage"] as? Double
        let resetTimeStr = json["reset_time"] as? String
        let resetTime = resetTimeStr.flatMap { Self.parseDate($0) }
        let resetAfterSec = json["reset_after_seconds"] as? Int
        let credits = json["credits"] as? Int ?? 0
        let autoSwitch = json["auto_switch_enabled"] as? Bool ?? true
        let autoSwitchBizOnly = json["auto_switch_business_only"] as? Bool ?? false
        let autoSwitchBizPriority = json["auto_switch_business_priority"] as? Bool ?? false
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
                let rStr = a["reset_time"] as? String
                let rTime = rStr.flatMap { Self.parseDate($0) }
                let rSec = a["reset_after_seconds"] as? Int
                let cr = a["credits"] as? Int ?? 0
                let err = a["error"] as? String
                let mult = a["plan_multiplier"] as? Double ?? 1.0

                accountsList.append(AccountQuota(
                    id: id,
                    name: name,
                    email: email,
                    planType: plan,
                    isCurrentActive: isAct,
                    fiveHourPercentage: pct,
                    weeklyPercentage: wPct,
                    models: [],
                    resetTime: rTime,
                    resetAfterSeconds: rSec,
                    credits: cr,
                    error: err,
                    planMultiplier: mult
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
            resetTime: resetTime,
            resetAfterSeconds: resetAfterSec,
            credits: credits,
            autoSwitchEnabled: autoSwitch,
            autoSwitchBusinessOnly: autoSwitchBizOnly,
            autoSwitchBusinessPriority: autoSwitchBizPriority,
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
                    errMsg = String(data: errData, encoding: .utf8)?.trimmingCharacters(in: .whitespacesAndNewlines)
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
                    errMsg = String(data: errData, encoding: .utf8)?.trimmingCharacters(in: .whitespacesAndNewlines)
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
            // Send SIGTERM to ChatGPT main process to bypass interactive beforeunload ("Leave") dialog
            let pkill = Process()
            pkill.executableURL = URL(fileURLWithPath: "/usr/bin/pkill")
            pkill.arguments = ["-TERM", "-f", "/Applications/ChatGPT.app/Contents/MacOS/ChatGPT"]
            try? pkill.run()
            pkill.waitUntilExit()

            Thread.sleep(forTimeInterval: 0.8)

            let openProc = Process()
            openProc.executableURL = URL(fileURLWithPath: "/usr/bin/open")
            openProc.arguments = ["-a", "/Applications/ChatGPT.app"]
            try? openProc.run()
        }
    }

    public func renameAccount(id: String, newName: String?, completion: @escaping (Bool, String?) -> Void) {
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
                let errMsg = success ? nil : String(data: errData, encoding: .utf8)?.trimmingCharacters(in: .whitespacesAndNewlines)
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
              let restart = settings["restart_app_on_switch"] as? Bool else {
            return false // Default is false
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
              let enabled = settings["auto_switch_enabled"] as? Bool else {
            return true // Default is true
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
              let enabled = settings["auto_switch_business_only"] as? Bool else {
            return false // Default is false
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
              let enabled = settings["auto_switch_business_priority"] as? Bool else {
            return false // Default is false
        }
        return enabled
    }
}
