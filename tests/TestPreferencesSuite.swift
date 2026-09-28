import Foundation

/// A preferences store that belongs to one test run.
///
/// A test binary has no bundle, so its standard defaults are the domain named after the
/// executable (`app_delegate_test`). Every run of that binary on the machine shares it, from
/// any checkout or worktree, and a concurrent run's writes land between another run's write
/// and read. Each suite here is named by an absolute path inside a directory made for this run
/// only, so no other process uses it.
///
/// The suite does not live in `~/Library/Preferences`: `removePersistentDomain(forName:)`
/// empties a domain there but leaves its plist file behind, one per run. Here the store is
/// removed with `removePersistentDomain(forName:)` and then its directory is deleted.
/// `tests/swift_test_defaults_isolation.sh` rejects Swift tests and sources that use the
/// standard store.
final class TestPreferencesSuite {
  let name: String
  let defaults: UserDefaults
  private let directory: URL

  init(purpose: String) {
    preconditionIsolated(
      !purpose.isEmpty && !purpose.contains("/"), "Suite purpose must be one path component")
    TestPreferencesSuite.requireNotRunningAsMonitor()

    var template = Array(
      FileManager.default.temporaryDirectory
        .appendingPathComponent("codex-monitor-test-defaults.XXXXXX").path.utf8CString)
    let created = template.withUnsafeMutableBufferPointer { mkdtemp($0.baseAddress!) }
    preconditionIsolated(created != nil, "mkdtemp failed for a test defaults directory")
    directory = URL(fileURLWithPath: String(cString: template), isDirectory: true)
    TestPreferencesSuite.pendingDirectories.append(directory.path)
    TestPreferencesSuite.registerExitCleanupOnce()
    name = directory.appendingPathComponent(purpose).path
    guard let suite = UserDefaults(suiteName: name) else {
      preconditionIsolated(false, "UserDefaults refused the suite \(name)")
      fatalError("unreachable")
    }
    defaults = suite

    // Prove the store is the file in this directory, not a domain in ~/Library/Preferences.
    let probeKey = "codexMonitorTestDefaultsProbe"
    suite.set(true, forKey: probeKey)
    _ = suite.synchronize()
    preconditionIsolated(
      FileManager.default.fileExists(atPath: name + ".plist"),
      "Test defaults suite must be stored at \(name).plist")
    suite.removeObject(forKey: probeKey)
  }

  /// Removes the store and its directory. Exit cleanup does the same after a failed assertion.
  func tearDown() {
    defaults.removePersistentDomain(forName: name)
    TestPreferencesSuite.removeDirectory(atPath: directory.path)
    TestPreferencesSuite.pendingDirectories.removeAll { $0 == directory.path }
    preconditionIsolated(
      !FileManager.default.fileExists(atPath: directory.path),
      "Test defaults directory \(directory.path) must be removed")
  }

  /// Stops the run if this binary's standard store would be the Monitor's own preferences.
  /// That happens when the binary is named after the Monitor's bundle identifier, or when it
  /// sits next to a `Resources/Info.plist` (matched case-insensitively) that declares it: a
  /// bare executable in such a directory takes that plist's identifier. For example, a test
  /// binary built into the repository root would.
  static func requireNotRunningAsMonitor() {
    let monitorDomain = monitorBundleIdentifier()
    preconditionIsolated(
      Bundle.main.bundleIdentifier != monitorDomain
        && ProcessInfo.processInfo.processName != monitorDomain,
      "The test binary must not run as the Monitor (\(monitorDomain))")
  }

  /// The Monitor's preferences domain, from this repository's `resources/Info.plist`. The file
  /// is found from this source file's compile-time path, which `scripts/test_swift.sh` passes
  /// relative to the repository root, so run the binary from there.
  static func monitorBundleIdentifier() -> String {
    let infoPlist = URL(fileURLWithPath: #filePath)
      .deletingLastPathComponent().deletingLastPathComponent()
      .appendingPathComponent("resources/Info.plist")
    guard let info = NSDictionary(contentsOf: infoPlist),
      let identifier = info["CFBundleIdentifier"] as? String, !identifier.isEmpty
    else {
      preconditionIsolated(false, "Cannot read the Monitor bundle identifier from \(infoPlist.path)")
      fatalError("unreachable")
    }
    return identifier
  }

  private static var pendingDirectories: [String] = []
  private static var exitCleanupRegistered = false

  private static func registerExitCleanupOnce() {
    guard !exitCleanupRegistered else { return }
    exitCleanupRegistered = true
    // Test assertions call exit(1); remove every directory still in use when that happens.
    atexit {
      for path in TestPreferencesSuite.pendingDirectories {
        TestPreferencesSuite.removeDirectory(atPath: path)
      }
    }
  }

  private static func removeDirectory(atPath path: String) {
    try? FileManager.default.removeItem(atPath: path)
  }
}

private func preconditionIsolated(
  _ condition: Bool, _ message: String, file: StaticString = #file, line: UInt = #line
) {
  if !condition {
    print("❌ Test defaults isolation failed: \(message) at \(file):\(line)")
    exit(1)
  }
}
