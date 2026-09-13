import Foundation
import ServiceManagement

public protocol ScriptExecuting {
  func executeAppleScript(_ script: String) -> (exitCode: Int32, output: String)
}

public final class DefaultScriptExecutor: ScriptExecuting {
  public init() {}

  public func executeAppleScript(_ script: String) -> (exitCode: Int32, output: String) {
    let process = Process()
    process.executableURL = URL(fileURLWithPath: "/usr/bin/osascript")
    process.arguments = ["-e", script]

    let pipe = Pipe()
    process.standardOutput = pipe
    process.standardError = pipe

    do {
      try process.run()
      let data = pipe.fileHandleForReading.readDataToEndOfFile()
      process.waitUntilExit()
      let output =
        String(data: data, encoding: .utf8)?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
      return (process.terminationStatus, output)
    } catch {
      return (-1, error.localizedDescription)
    }
  }
}

public protocol SMAppServiceManaging {
  var isAvailable: Bool { get }
  var status: SMAppService.Status { get }
  func register() throws
  func unregister() throws
}

public final class DefaultSMAppServiceManager: SMAppServiceManaging {
  public init() {}

  public var isAvailable: Bool {
    if #available(macOS 13.0, *) {
      return Bundle.main.bundleIdentifier != nil
    }
    return false
  }

  public var status: SMAppService.Status {
    if #available(macOS 13.0, *) {
      return SMAppService.mainApp.status
    }
    return .notFound
  }

  public func register() throws {
    if #available(macOS 13.0, *) {
      try SMAppService.mainApp.register()
    }
  }

  public func unregister() throws {
    if #available(macOS 13.0, *) {
      try SMAppService.mainApp.unregister()
    }
  }
}

public final class AutoLaunchManager {
  public static let shared = AutoLaunchManager()

  public static let userDefaultsKey = "CodexMonitorLaunchAtLogin"
  public static let defaultAppName = "Codex Monitor"
  public static let defaultAppPath = "/Applications/Codex Monitor.app"

  private let scriptExecutor: ScriptExecuting
  private let smService: SMAppServiceManaging
  private let userDefaults: UserDefaults
  private let bundle: Bundle
  private let fileManager: FileManager

  public init(
    scriptExecutor: ScriptExecuting = DefaultScriptExecutor(),
    smService: SMAppServiceManaging = DefaultSMAppServiceManager(),
    userDefaults: UserDefaults = .standard,
    bundle: Bundle = .main,
    fileManager: FileManager = .default
  ) {
    self.scriptExecutor = scriptExecutor
    self.smService = smService
    self.userDefaults = userDefaults
    self.bundle = bundle
    self.fileManager = fileManager
  }

  public static func escapeAppleScriptString(_ str: String) -> String {
    return
      str
      .replacingOccurrences(of: "\\", with: "\\\\")
      .replacingOccurrences(of: "\"", with: "\\\"")
  }

  public var appPath: String {
    let bundlePath = bundle.bundlePath
    if bundlePath.hasSuffix(".app") && fileManager.fileExists(atPath: bundlePath) {
      return bundlePath
    }
    if fileManager.fileExists(atPath: Self.defaultAppPath) {
      return Self.defaultAppPath
    }
    let userAppPath = (fileManager.homeDirectoryForCurrentUser.path as NSString)
      .appendingPathComponent("Applications/Codex Monitor.app")
    if fileManager.fileExists(atPath: userAppPath) {
      return userAppPath
    }
    return Self.defaultAppPath
  }

  public var appName: String {
    if let displayName = bundle.object(forInfoDictionaryKey: "CFBundleDisplayName") as? String,
      !displayName.isEmpty
    {
      return displayName
    }
    if let bundleName = bundle.object(forInfoDictionaryKey: "CFBundleName") as? String,
      !bundleName.isEmpty
    {
      return bundleName
    }
    return Self.defaultAppName
  }

  public var isEnabled: Bool {
    if smService.isAvailable {
      let status = smService.status
      if status == .enabled {
        return true
      }
    }
    let safeName = Self.escapeAppleScriptString(appName)
    let script = "tell application \"System Events\" to get name of every login item"
    let result = scriptExecutor.executeAppleScript(script)
    if result.exitCode == 0 {
      let items = result.output.components(separatedBy: ",").map {
        $0.trimmingCharacters(in: .whitespacesAndNewlines)
      }
      return items.contains(safeName) || items.contains("\(safeName).app")
    }
    return userDefaults.bool(forKey: Self.userDefaultsKey)
  }

  @discardableResult
  public func setEnabled(_ enabled: Bool) -> Bool {
    var success = false
    if enabled {
      if smService.isAvailable {
        do {
          try smService.register()
          if smService.status == .enabled {
            success = true
          }
        } catch {}
      }
      if !success {
        let safeName = Self.escapeAppleScriptString(appName)
        let safePath = Self.escapeAppleScriptString(appPath)
        let script = """
          tell application "System Events"
              if exists (every login item whose name is "\(safeName)") then
                  delete (every login item whose name is "\(safeName)")
              end if
              make login item at end with properties {name:"\(safeName)", path:"\(safePath)", hidden:false}
          end tell
          """
        success = scriptExecutor.executeAppleScript(script).exitCode == 0
      }
      if success {
        userDefaults.set(true, forKey: Self.userDefaultsKey)
      }
    } else {
      if smService.isAvailable {
        try? smService.unregister()
      }
      let safeName = Self.escapeAppleScriptString(appName)
      let script = """
        tell application "System Events"
            delete (every login item whose name is "\(safeName)")
        end tell
        """
      success = scriptExecutor.executeAppleScript(script).exitCode == 0
      if success {
        userDefaults.set(false, forKey: Self.userDefaultsKey)
      }
    }
    return success
  }
}
