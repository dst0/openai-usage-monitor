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
/// `tests/swift_test_defaults_isolation.sh` rejects Swift tests that use the standard store.
final class TestPreferencesSuite {
  let name: String
  let defaults: UserDefaults
  private let directory: URL

  init(purpose: String) {
    preconditionIsolated(
      !purpose.isEmpty && !purpose.contains("/"), "Suite purpose must be one path component")
    let monitorDomain = TestPreferencesSuite.monitorBundleIdentifier()
    // A binary named or bundled as the Monitor would make its standard store the user's.
    preconditionIsolated(
      Bundle.main.bundleIdentifier != monitorDomain
        && ProcessInfo.processInfo.processName != monitorDomain,
      "The test binary must not run as the Monitor (\(monitorDomain))")

    var template = Array(
      FileManager.default.temporaryDirectory
        .appendingPathComponent("codex-monitor-test-defaults.XXXXXX").path.utf8CString)
    let created = template.withUnsafeMutableBufferPointer { mkdtemp($0.baseAddress!) }
    preconditionIsolated(created != nil, "mkdtemp failed for a test defaults directory")
    directory = URL(fileURLWithPath: String(cString: template), isDirectory: true)
    name = directory.appendingPathComponent(purpose).path
    preconditionIsolated(
      name.hasPrefix(directory.path + "/"), "Test defaults suite \(name) must be in its own directory")
    guard let suite = UserDefaults(suiteName: name) else {
      preconditionIsolated(false, "UserDefaults refused the suite \(name)")
      fatalError("unreachable")
    }
    defaults = suite
    TestPreferencesSuite.pendingDirectories.append(directory.path)
    TestPreferencesSuite.registerExitCleanupOnce()

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

  /// The installed Monitor's preferences domain, read from the bundle's own Info.plist.
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
