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

/// What macOS reports about the Monitor's own login item.
public enum LoginItemState: Equatable {
  /// The main-app login service is enabled, or a System Events login item opens this bundle.
  case enabled
  /// The main-app login service is not enabled, and no System Events login item opens this bundle.
  case disabled
  /// The main-app login service is not enabled and the System Events login items could not be
  /// read, so a login item such as the one `scripts/install.sh` adds may or may not exist.
  case unknown
}

/// Reads and changes the Monitor's login item.
///
/// There are two registrations: the main-app login service (`SMAppService.mainApp`) and a
/// System Events login item, which `scripts/install.sh` adds for the installed bundle path. Either
/// one opens the app at login, so `state` reports enabled when either points at this bundle. No
/// preference is cached: every answer is read from macOS, and `setEnabled` reports the state read
/// back after the change, not whether each step claimed success.
public final class AutoLaunchManager {
  public static let shared = AutoLaunchManager()

  public static let defaultAppName = "Codex Monitor"
  public static let defaultAppPath = "/Applications/Codex Monitor.app"

  /// Plain AppleScript that addresses no application: prints each text item of `itemPaths` on
  /// its own line and skips items without a text path.
  public static let printLoginItemPathsScript = """
    set output to ""
    repeat with itemPathReference in itemPaths
      set itemPath to contents of itemPathReference
      if class of itemPath is text then set output to output & itemPath & linefeed
    end repeat
    return output
    """

  /// Lists the path of every System Events login item, one per line.
  public static let loginItemPathsScript =
    "tell application \"System Events\" to set itemPaths to path of every login item\n"
    + printLoginItemPathsScript

  private let scriptExecutor: ScriptExecuting
  private let smService: SMAppServiceManaging
  private let bundle: Bundle
  private let fileManager: FileManager

  public init(
    scriptExecutor: ScriptExecuting = DefaultScriptExecutor(),
    smService: SMAppServiceManaging = DefaultSMAppServiceManager(),
    bundle: Bundle = .main,
    fileManager: FileManager = .default
  ) {
    self.scriptExecutor = scriptExecutor
    self.smService = smService
    self.bundle = bundle
    self.fileManager = fileManager
  }

  public static func escapeAppleScriptString(_ str: String) -> String {
    return
      str
      .replacingOccurrences(of: "\\", with: "\\\\")
      .replacingOccurrences(of: "\"", with: "\\\"")
  }

  /// Replaces every System Events login item named `name` with one that opens `path`, as
  /// `scripts/install.sh` does.
  public static func addLoginItemScript(name: String, path: String) -> String {
    let safeName = escapeAppleScriptString(name)
    let safePath = escapeAppleScriptString(path)
    return """
      tell application "System Events"
          if exists (every login item whose name is "\(safeName)") then
              delete (every login item whose name is "\(safeName)")
          end if
          make login item at end with properties {name:"\(safeName)", path:"\(safePath)", hidden:false}
      end tell
      """
  }

  /// Deletes every System Events login item named `name`.
  public static func removeLoginItemScript(name: String) -> String {
    let safeName = escapeAppleScriptString(name)
    return """
      tell application "System Events"
          delete (every login item whose name is "\(safeName)")
      end tell
      """
  }

  /// Whether two paths name one file: the same device and inode when both exist, following
  /// symbolic links such as the `~/Applications` link to an `/Applications` install, and
  /// otherwise the same standardized path, so a trailing slash does not matter.
  public static func isSameFile(_ first: String, _ second: String) -> Bool {
    var firstStatus = stat()
    var secondStatus = stat()
    if stat(first, &firstStatus) == 0 && stat(second, &secondStatus) == 0 {
      return firstStatus.st_dev == secondStatus.st_dev && firstStatus.st_ino == secondStatus.st_ino
    }
    return URL(fileURLWithPath: first).standardizedFileURL.path
      == URL(fileURLWithPath: second).standardizedFileURL.path
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

  /// Reads the login item from macOS. Blocks while `osascript` asks System Events, so call it
  /// off the main thread.
  public var state: LoginItemState {
    if smService.isAvailable && smService.status == .enabled {
      return .enabled
    }
    let listing = scriptExecutor.executeAppleScript(Self.loginItemPathsScript)
    guard listing.exitCode == 0 else { return .unknown }
    let ownPath = appPath
    let opensThisApp = listing.output.split(whereSeparator: \.isNewline).contains { line in
      line.hasPrefix("/") && Self.isSameFile(String(line), ownPath)
    }
    return opensThisApp ? .enabled : .disabled
  }

  /// Adds or removes the login item, then returns the state read back from macOS. A step that
  /// fails is not reported by itself: the read-back shows whether the change took effect.
  public func setEnabled(_ enabled: Bool) -> LoginItemState {
    if enabled {
      addLoginItem()
    } else {
      removeLoginItems()
    }
    return state
  }

  private func addLoginItem() {
    if smService.isAvailable {
      try? smService.register()
      if smService.status == .enabled {
        return
      }
    }
    _ = scriptExecutor.executeAppleScript(Self.addLoginItemScript(name: appName, path: appPath))
  }

  private func removeLoginItems() {
    if smService.isAvailable {
      try? smService.unregister()
    }
    _ = scriptExecutor.executeAppleScript(Self.removeLoginItemScript(name: appName))
  }
}
