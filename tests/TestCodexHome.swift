import Foundation

/// A Codex home that belongs to one test run.
///
/// `AppDelegate` and `CodexClient` read, watch, and lock Monitor state only in the Codex home
/// their client was built with. `CodexClient.shared` uses the live one (`CODEX_HOME`, or
/// `~/.codex`), which ChatGPT.app and the Rust core own, so every test builds its client on a
/// home made here instead: a path inside a private `mkdtemp` directory that the run deletes.
/// `tests/swift_test_codex_home_isolation.sh` rejects tests that use the shared client, build
/// one without `codexHome:`, or change `CODEX_HOME`.
final class TestCodexHome {
  let url: URL
  private let directory: URL

  /// `created: false` names a home that does not exist yet, to show that nothing creates it.
  init(purpose: String, created: Bool = true) {
    requireCodexHome(
      !purpose.isEmpty && !purpose.contains("/"), "Codex home purpose must be one path component")
    var template = Array(
      FileManager.default.temporaryDirectory
        .appendingPathComponent("codex-monitor-test-home.XXXXXX", isDirectory: false).path.utf8CString)
    let made = template.withUnsafeMutableBufferPointer { mkdtemp($0.baseAddress!) }
    requireCodexHome(made != nil, "mkdtemp failed for a test Codex home")
    directory = URL(fileURLWithPath: String(cString: template), isDirectory: true)
    TestCodexHome.pendingDirectories.append(directory.path)
    TestCodexHome.registerExitCleanupOnce()
    url = directory.appendingPathComponent(purpose, isDirectory: true)
    if created {
      requireCodexHome(
        (try? FileManager.default.createDirectory(at: url, withIntermediateDirectories: false))
          != nil, "Cannot create the test Codex home \(url.path)")
    }
  }

  var exists: Bool { FileManager.default.fileExists(atPath: url.path) }

  func file(_ name: String) -> URL { url.appendingPathComponent(name, isDirectory: false) }

  /// Writes `object` as JSON to `name` with the owner-only mode the Rust core uses.
  func writeJSON(_ name: String, _ object: [String: Any]) {
    let data = try! JSONSerialization.data(withJSONObject: object)
    write(name, data)
  }

  /// Replaces `name` the way the Rust core does: a sibling file renamed over it.
  func write(_ name: String, _ data: Data) {
    let staging = file(".\(name).\(UUID().uuidString).tmp")
    requireCodexHome(
      FileManager.default.createFile(
        atPath: staging.path, contents: data, attributes: [.posixPermissions: 0o600]),
      "Cannot stage \(name) in the test Codex home")
    requireCodexHome(rename(staging.path, file(name).path) == 0, "Cannot replace \(name)")
  }

  /// A client on this home that touches nothing live: its coordinator runs never start a
  /// process, its CLI is a path in this home that does not exist unless a test writes it, and
  /// it sees no running Desktop.
  func client(
    distributionRunner: @escaping ([String]) -> Bool = { _ in false },
    cliExecutable: (() -> URL)? = nil,
    desktopProcess: @escaping () -> CodexDesktopProcessIdentity? = { nil },
    desktopAppAccountIdProvider: @escaping () -> String? = { nil }
  ) -> CodexClient {
    let missingCLI = file("codex-mon-not-installed")
    return CodexClient(codexHome: url, distributionRunner: distributionRunner, cliExecutable: cliExecutable ?? { missingCLI }, desktopProcess: desktopProcess, desktopAppAccountIdProvider: desktopAppAccountIdProvider)
  }

  /// Removes the home and its directory. Exit cleanup does the same after a failed assertion.
  func tearDown() {
    try? FileManager.default.removeItem(at: directory)
    TestCodexHome.pendingDirectories.removeAll { $0 == directory.path }
    requireCodexHome(
      !FileManager.default.fileExists(atPath: directory.path),
      "Test Codex home directory \(directory.path) must be removed")
  }

  private static var pendingDirectories: [String] = []
  private static var exitCleanupRegistered = false

  private static func registerExitCleanupOnce() {
    guard !exitCleanupRegistered else { return }
    exitCleanupRegistered = true
    // Test assertions call exit(1); remove every home still in use when that happens.
    atexit {
      for path in TestCodexHome.pendingDirectories {
        try? FileManager.default.removeItem(atPath: path)
      }
    }
  }
}

private func requireCodexHome(
  _ condition: Bool, _ message: String, file: StaticString = #file, line: UInt = #line
) {
  if !condition {
    print("❌ Test Codex home failed: \(message) at \(file):\(line)")
    exit(1)
  }
}
