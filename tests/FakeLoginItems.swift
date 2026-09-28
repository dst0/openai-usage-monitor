import Foundation
import ServiceManagement

/// System Events login items held in memory, so no test reaches the real ones.
///
/// It answers only the scripts `AutoLaunchManager` sends: the path listing, the add for `appName`
/// and `appPath`, and a remove by exact paths. Any other script fails and is kept in
/// `unexpectedScripts`. A listing prints one path per line, as `osascript` prints the output of
/// `AutoLaunchManager.printLoginItemPathsScript` (the Launch at Login tests run that script to
/// check), followed by `listingNoise` when set. `readGate` and `writeGate`, when set, hold a
/// listing or a change until they are signalled; a listing takes its answer before it waits.
final class FakeSystemEventsLoginItems: ScriptExecuting {
  struct Item: Equatable {
    var name: String
    var path: String
  }

  /// What `osascript` prints when the Monitor may not control System Events.
  static let notAuthorized = "execution error: Not authorized to send Apple events to System Events. (-1743)"

  let appName: String
  let appPath: String
  private let lock = NSLock()
  private var storedItems: [Item] = []
  private var storedCanRead = true
  private var storedCanWrite = true
  private var storedAcceptsWithoutEffect = false
  private var storedListingNoise: String?
  private var storedRemovedPaths: [[String]] = []
  private var storedReads = 0
  private var storedWrites: [String] = []
  private var storedUnexpected: [String] = []
  private var storedReadGate: DispatchSemaphore?
  private var storedWriteGate: DispatchSemaphore?

  init(appName: String, appPath: String) {
    self.appName = appName
    self.appPath = appPath
  }

  var items: [Item] {
    get { locked { storedItems } }
    set { locked { storedItems = newValue } }
  }
  var canRead: Bool {
    get { locked { storedCanRead } }
    set { locked { storedCanRead = newValue } }
  }
  var canWrite: Bool {
    get { locked { storedCanWrite } }
    set { locked { storedCanWrite = newValue } }
  }
  /// When true, an add or remove reports success and changes nothing.
  var acceptsChangesWithoutEffect: Bool {
    get { locked { storedAcceptsWithoutEffect } }
    set { locked { storedAcceptsWithoutEffect = newValue } }
  }
  /// Extra text a listing prints after the paths, such as a warning on the merged stderr.
  var listingNoise: String? {
    get { locked { storedListingNoise } }
    set { locked { storedListingNoise = newValue } }
  }
  /// The paths each remove asked to delete, in order.
  var removedPaths: [[String]] { locked { storedRemovedPaths } }
  var readGate: DispatchSemaphore? {
    get { locked { storedReadGate } }
    set { locked { storedReadGate = newValue } }
  }
  var writeGate: DispatchSemaphore? {
    get { locked { storedWriteGate } }
    set { locked { storedWriteGate = newValue } }
  }
  /// Listings answered, including refused ones.
  var reads: Int { locked { storedReads } }
  /// "add" or "remove" for each change asked for, including refused ones.
  var writes: [String] { locked { storedWrites } }
  var unexpectedScripts: [String] { locked { storedUnexpected } }

  func executeAppleScript(_ script: String) -> (exitCode: Int32, output: String) {
    if script == AutoLaunchManager.loginItemPathsScript {
      let (answer, gate): ((Int32, String), DispatchSemaphore?) = locked {
        storedReads += 1
        let listed = (storedItems.map(\.path) + [storedListingNoise].compactMap { $0 }).joined(separator: "\n")
        let answer: (Int32, String) = storedCanRead ? (0, listed) : (1, Self.notAuthorized)
        return (answer, storedReadGate)
      }
      gate?.wait()
      return answer
    }
    let change: String
    let removing = Self.removedPaths(in: script)
    if script == AutoLaunchManager.addLoginItemScript(name: appName, path: appPath) {
      change = "add"
    } else if removing != nil {
      change = "remove"
    } else {
      locked { storedUnexpected.append(script) }
      return (1, "unexpected script")
    }
    locked { storedWriteGate }?.wait()
    return locked {
      storedWrites.append(change)
      if let removing { storedRemovedPaths.append(removing) }
      guard storedCanWrite else { return (1, Self.notAuthorized) }
      guard !storedAcceptsWithoutEffect else { return (0, "") }
      if let removing {
        storedItems.removeAll { removing.contains($0.path) }
        return (0, "")
      }
      // The add first deletes every item with the app's name, as scripts/install.sh does.
      storedItems.removeAll { $0.name == appName }
      storedItems.append(Item(name: appName, path: appPath))
      return (0, "login item \(appName)")
    }
  }

  /// The paths a remove script deletes, when `script` is exactly the one
  /// `AutoLaunchManager.removeLoginItemsScript(paths:)` builds for them; otherwise nil.
  private static func removedPaths(in script: String) -> [String]? {
    let prefix = "    delete (every login item whose path is \""
    let suffix = "\")"
    let lines = script.components(separatedBy: "\n")
    guard lines.count > 2 else { return nil }
    var paths: [String] = []
    for line in lines.dropFirst().dropLast() {
      guard line.hasPrefix(prefix), line.hasSuffix(suffix) else { return nil }
      let escaped = String(line.dropFirst(prefix.count).dropLast(suffix.count))
      paths.append(escaped.replacingOccurrences(of: "\\\"", with: "\"").replacingOccurrences(of: "\\\\", with: "\\"))
    }
    return script == AutoLaunchManager.removeLoginItemsScript(paths: paths) ? paths : nil
  }

  private func locked<T>(_ body: () -> T) -> T {
    lock.lock()
    defer { lock.unlock() }
    return body()
  }
}

/// The main-app login service (`SMAppService.mainApp`) held in memory.
final class FakeMainAppLoginService: SMAppServiceManaging {
  struct Refused: Error {}

  private let lock = NSLock()
  private var storedAvailable = true
  private var storedStatus: SMAppService.Status = .notRegistered
  private var storedStatusAfterRegister: SMAppService.Status = .enabled
  private var storedRegisterFails = false
  private var storedUnregisterFails = false
  private var storedRegisterCalls = 0
  private var storedUnregisterCalls = 0

  var isAvailable: Bool {
    get { locked { storedAvailable } }
    set { locked { storedAvailable = newValue } }
  }
  var status: SMAppService.Status {
    get { locked { storedStatus } }
    set { locked { storedStatus = newValue } }
  }
  /// The status a successful `register()` leaves; `.requiresApproval` is a registration the user
  /// has not approved.
  var statusAfterRegister: SMAppService.Status {
    get { locked { storedStatusAfterRegister } }
    set { locked { storedStatusAfterRegister = newValue } }
  }
  var registerFails: Bool {
    get { locked { storedRegisterFails } }
    set { locked { storedRegisterFails = newValue } }
  }
  var unregisterFails: Bool {
    get { locked { storedUnregisterFails } }
    set { locked { storedUnregisterFails = newValue } }
  }
  var registerCalls: Int { locked { storedRegisterCalls } }
  var unregisterCalls: Int { locked { storedUnregisterCalls } }

  func register() throws {
    try locked {
      storedRegisterCalls += 1
      if storedRegisterFails { throw Refused() }
      storedStatus = storedStatusAfterRegister
    }
  }

  func unregister() throws {
    try locked {
      storedUnregisterCalls += 1
      if storedUnregisterFails { throw Refused() }
      storedStatus = .notRegistered
    }
  }

  private func locked<T>(_ body: () throws -> T) rethrows -> T {
    lock.lock()
    defer { lock.unlock() }
    return try body()
  }
}
