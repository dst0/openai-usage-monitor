import Foundation

// MARK: - Model Quota
public struct ModelQuota: Identifiable, Sendable {
    public let id: String
    public let displayName: String
    public let remainingFraction: Double
    public let resetTime: Date?

    public init(id: String, displayName: String, remainingFraction: Double, resetTime: Date? = nil) {
        self.id = id
        self.displayName = displayName
        self.remainingFraction = remainingFraction
        self.resetTime = resetTime
    }

    public var percentage: Double {
        return max(0.0, min(100.0, remainingFraction * 100.0))
    }

    public var shortPercentageString: String {
        return String(format: "%.0f%%", percentage)
    }

    public var isExhausted: Bool {
        return remainingFraction <= 0.01
    }
}

// MARK: - Account Quota
public struct AccountQuota: Identifiable, Sendable {
    public let id: String
    public let name: String?
    public let email: String
    public let planType: String
    public let isCurrentActive: Bool
    public let fiveHourPercentage: Double
    public let weeklyPercentage: Double?
    public let models: [ModelQuota]
    public let resetTime: Date?
    public let resetAfterSeconds: Int?
    public let credits: Int
    public let error: String?
    public let planMultiplier: Double

    public init(
        id: String,
        name: String? = nil,
        email: String,
        planType: String,
        isCurrentActive: Bool,
        fiveHourPercentage: Double,
        weeklyPercentage: Double?,
        models: [ModelQuota] = [],
        resetTime: Date?,
        resetAfterSeconds: Int?,
        credits: Int,
        error: String? = nil,
        planMultiplier: Double = 1.0
    ) {
        self.id = id
        self.name = name
        self.email = email
        self.planType = planType
        self.isCurrentActive = isCurrentActive
        self.fiveHourPercentage = max(0.0, fiveHourPercentage)
        self.weeklyPercentage = weeklyPercentage.map { max(0.0, $0) }
        self.models = models
        self.resetTime = resetTime
        self.resetAfterSeconds = resetAfterSeconds
        self.credits = credits
        self.error = error
        self.planMultiplier = max(0.1, planMultiplier)
    }

    public var displayName: String {
        if let n = name, !n.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            return n.trimmingCharacters(in: .whitespacesAndNewlines)
        }
        let prefix = email.components(separatedBy: "@").first ?? ""
        return prefix.isEmpty ? email : prefix
    }

    public var planBadgeString: String {
        let cleanPlan = planType.capitalized
        if planMultiplier > 1.0 {
            return String(format: "%@ %.0fx", cleanPlan, planMultiplier)
        }
        return cleanPlan
    }

    public var percentageString: String {
        return String(format: "%.0f%%", fiveHourPercentage)
    }

    public var statusEmoji: String {
        let tankPct = planMultiplier > 0.0 ? (fiveHourPercentage / planMultiplier) : fiveHourPercentage
        if tankPct > 50.0 {
            return "🟢"
        } else if tankPct > 15.0 {
            return "🟡"
        } else {
            return "🔴"
        }
    }

    public var isBusiness: Bool {
        let plan = planType.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        if plan == "business" || plan == "team" || plan == "enterprise" {
            return true
        }
        if let n = name?.trimmingCharacters(in: .whitespacesAndNewlines).lowercased() {
            if n == "business" || n == "team" || n.contains("business") || n.contains("corporate") {
                return true
            }
        }
        let idLower = id.lowercased()
        if idLower.contains("business") || idLower.contains("-[business]") {
            return true
        }
        return false
    }

    public var isReady: Bool {
        let tankPct = planMultiplier > 0.0 ? (fiveHourPercentage / planMultiplier) : fiveHourPercentage
        return tankPct > 15.0
    }

    public var bulletChar: String {
        return isReady ? "●" : "○"
    }

    public var timeUntilResetString: String {
        guard let seconds = resetAfterSeconds, seconds > 0 else {
            return L10n.resetNow
        }
        let totalSeconds = seconds
        let days = totalSeconds / 86400
        let hours = (totalSeconds % 86400) / 3600
        let minutes = (totalSeconds % 3600) / 60
        if days > 0 {
            return L10n.duration(days: days, hours: hours)
        } else if hours > 0 {
            return L10n.durationHoursMinutes(hours: hours, minutes: minutes)
        } else {
            return L10n.durationMinutes(minutes: max(1, minutes))
        }
    }
}

// MARK: - Multi-Account Snapshot
public struct MultiAccountSnapshot: Sendable {
    public let timestamp: Date
    public let activeAccountId: String?
    public let activeEmail: String?
    public let activePlan: String?
    public let fiveHourPercentage: Double
    public let weeklyPercentage: Double?
    public let resetTime: Date?
    public let resetAfterSeconds: Int?
    public let credits: Int
    public let autoSwitchEnabled: Bool
    public let autoSwitchBusinessOnly: Bool
    public let autoSwitchBusinessPriority: Bool
    public let isAppRunning: Bool
    public let activeModelName: String?
    public let planMultiplier: Double
    public let accounts: [AccountQuota]
    public let appAccount: AccountQuota?
    public let cliAccount: AccountQuota?

    public init(
        timestamp: Date,
        activeAccountId: String?,
        activeEmail: String?,
        activePlan: String?,
        fiveHourPercentage: Double,
        weeklyPercentage: Double?,
        resetTime: Date?,
        resetAfterSeconds: Int?,
        credits: Int,
        autoSwitchEnabled: Bool = true,
        autoSwitchBusinessOnly: Bool = false,
        autoSwitchBusinessPriority: Bool = false,
        isAppRunning: Bool = true,
        activeModelName: String? = nil,
        planMultiplier: Double = 1.0,
        accounts: [AccountQuota] = [],
        appAccount: AccountQuota? = nil,
        cliAccount: AccountQuota? = nil
    ) {
        self.timestamp = timestamp
        self.activeAccountId = activeAccountId
        self.activeEmail = activeEmail
        self.activePlan = activePlan
        self.fiveHourPercentage = fiveHourPercentage
        self.weeklyPercentage = weeklyPercentage
        self.resetTime = resetTime
        self.resetAfterSeconds = resetAfterSeconds
        self.credits = credits
        self.autoSwitchEnabled = autoSwitchEnabled
        self.autoSwitchBusinessOnly = autoSwitchBusinessOnly
        self.autoSwitchBusinessPriority = autoSwitchBusinessPriority
        self.isAppRunning = isAppRunning
        self.activeModelName = activeModelName
        self.planMultiplier = max(0.1, planMultiplier)
        self.accounts = accounts

        let primary = cliAccount ?? accounts.first(where: { $0.isCurrentActive }) ?? accounts.first
        self.cliAccount = primary
        self.appAccount = appAccount ?? (isAppRunning ? primary : nil)
    }

    public var statusEmoji: String {
        let mult = cliAccount?.planMultiplier ?? planMultiplier
        let tankPct = mult > 0.0 ? (fiveHourPercentage / mult) : fiveHourPercentage
        if tankPct > 50.0 {
            return "🟢"
        } else if tankPct > 15.0 {
            return "🟡"
        } else {
            return "🔴"
        }
    }

    public var bulletsString: String {
        if accounts.isEmpty { return "[●]" }
        let bullets = accounts.map { $0.bulletChar }.joined()
        return "[\(bullets)]"
    }

    public var timeUntilResetString: String {
        guard let seconds = resetAfterSeconds, seconds > 0 else {
            return L10n.resetNow
        }
        let totalSeconds = seconds
        let days = totalSeconds / 86400
        let hours = (totalSeconds % 86400) / 3600
        let minutes = (totalSeconds % 3600) / 60
        if days > 0 {
            return L10n.duration(days: days, hours: hours)
        } else if hours > 0 {
            return L10n.durationHoursMinutes(hours: hours, minutes: minutes)
        } else {
            return L10n.durationMinutes(minutes: max(1, minutes))
        }
    }
}

// MARK: - Version Formatting & Detection Helpers
public enum VersionHelper {
    public static func formatMenuTitle(baseTitle: String, version: String?) -> String {
        guard let v = version?.trimmingCharacters(in: .whitespacesAndNewlines), !v.isEmpty else {
            return baseTitle
        }
        return "\(baseTitle) v\(v)"
    }

    public static func formatCLIStatusTitle(
        currentVersion: String?,
        isUpdateAvailable: Bool = false,
        availableVersion: String? = nil
    ) -> String {
        guard let ver = currentVersion?.trimmingCharacters(in: .whitespacesAndNewlines), !ver.isEmpty else {
            return L10n.cliNotFound
        }
        if isUpdateAvailable, let newVer = availableVersion?.trimmingCharacters(in: .whitespacesAndNewlines), !newVer.isEmpty {
            return L10n.cliUpdateAvailable(version: newVer)
        }
        return L10n.cliUpToDate(version: ver)
    }

    public static func parseCLIVersion(from rawOutput: String) -> String? {
        let clean = rawOutput.replacingOccurrences(of: #"\x1B\[[0-9;]*[a-zA-Z]"#, with: "", options: .regularExpression)
        if let match = clean.range(of: #"(?:(?<=[vV])|\b)\d+\.\d+\.\d+(?:-[a-zA-Z0-9.]+)?\b"#, options: .regularExpression) {
            return String(clean[match])
        }
        return nil
    }
}

// MARK: - Help Documentation Path Resolution Helper
public enum HelpsDocHelper {
    public static func findHelpsHTMLURL(
        fileManager: FileManager = .default,
        bundle: Bundle = .main,
        arguments: [String] = ProcessInfo.processInfo.arguments
    ) -> URL? {
        if let url = bundle.url(forResource: "helps", withExtension: "html"), fileManager.fileExists(atPath: url.path) {
            return url
        }

        if let firstArg = arguments.first, !firstArg.isEmpty {
            let execURL = URL(fileURLWithPath: firstArg)
            let bundleResourcesURL = execURL.deletingLastPathComponent().appendingPathComponent("../Resources/helps.html").standardized
            if fileManager.fileExists(atPath: bundleResourcesURL.path) {
                return bundleResourcesURL
            }
        }

        let home = fileManager.homeDirectoryForCurrentUser

        let codexHelpURL = home.appendingPathComponent(".codex/helps.html")
        if fileManager.fileExists(atPath: codexHelpURL.path) {
            return codexHelpURL
        }

        let systemAppURL = URL(fileURLWithPath: "/Applications/Codex Monitor.app/Contents/Resources/helps.html")
        if fileManager.fileExists(atPath: systemAppURL.path) {
            return systemAppURL
        }

        let devURL = home.appendingPathComponent("dev/openai-usage-monitor/resources/helps.html")
        if fileManager.fileExists(atPath: devURL.path) {
            return devURL
        }

        return nil
    }

    public static func localizedHelpsHTMLURL(
        languageCode: String,
        fileManager: FileManager = .default,
        bundle: Bundle = .main,
        arguments: [String] = ProcessInfo.processInfo.arguments
    ) -> URL? {
        guard let url = findHelpsHTMLURL(fileManager: fileManager, bundle: bundle, arguments: arguments) else {
            return nil
        }
        let supportedDocLanguages: Set<String> = ["en", "uk", "de", "fr", "es", "it", "pt", "pl", "nl"]
        let trimmed = languageCode.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        let docLang = supportedDocLanguages.contains(trimmed) ? trimmed : "en"

        if var comps = URLComponents(url: url, resolvingAgainstBaseURL: false) {
            comps.queryItems = [URLQueryItem(name: "lang", value: docLang)]
            if let localizedURL = comps.url {
                return localizedURL
            }
        }
        return url
    }
}

// MARK: - Model Matcher
public enum ModelMatcher {
    public static func normalize(_ str: String) -> String {
        return str.lowercased()
            .replacingOccurrences(of: "-", with: "")
            .replacingOccurrences(of: "_", with: "")
            .replacingOccurrences(of: ".", with: "")
            .trimmingCharacters(in: .whitespacesAndNewlines)
    }

    public static func matches(displayName: String?, target: String?) -> Bool {
        guard let displayName = displayName, let target = target else { return false }
        let dTrim = displayName.trimmingCharacters(in: .whitespacesAndNewlines)
        let tTrim = target.trimmingCharacters(in: .whitespacesAndNewlines)
        if dTrim.isEmpty || tTrim.isEmpty { return false }

        if dTrim.caseInsensitiveCompare(tTrim) == .orderedSame {
            return true
        }

        let normD = normalize(dTrim)
        let normT = normalize(tTrim)
        return normD == normT || normD.contains(normT) || normT.contains(normD)
    }
}
