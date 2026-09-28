import Foundation

#if !CODEX_MONITOR_TESTS
#error("Build Swift tests with -D CODEX_MONITOR_TESTS so the Codex home tripwires in Sources/ run")
#endif

/// The Monitor's Codex home for one run of a Swift test binary.
///
/// Monitor state lives in `CODEX_HOME`, or in the live `~/.codex` when that variable is unset.
/// A test binary that compiles a Source which resolves it calls `activate()` first: that makes a
/// directory private to this run and points `CODEX_HOME` at `<run>/home`. The directory is removed
/// when the binary exits, also after a failed assertion or a tripwire; a crash or an abort leaves
/// it in the temporary directory. A test that needs a home of its own takes one with
/// `enter(_:create:)` and gives it back with `leave(_:)`.
///
/// `scripts/test_swift.sh` builds tests with `-D CODEX_MONITOR_TESTS`, which compiles tripwires
/// into `Sources/`: `CodexClient.codexHome` calls `requireIsolated(_:seam:)` with the raw
/// `CODEX_HOME` value, and every path that names the live `~/.codex` calls `forbid(_:)` (the
/// home fallback when `CODEX_HOME` is unset, the single-instance lock, the help page there). Both
/// run before the path is built, because building a file URL without `isDirectory:` already reads
/// the file system, and both stop the run, so no test reads or creates anything in `~/.codex`.
enum TestCodexHome {
  private static let lock = NSLock()
  private static var runPath: String?
  private static var entered: [(home: String, previous: String?)] = []
  private static var nextScope = 0
  /// Set once, before exit cleanup is registered; exit cleanup reads it without the lock, so a
  /// failure raised on any thread cannot deadlock the exit.
  private static var cleanupPath: String?

  /// Makes this run's private directory and points `CODEX_HOME` at `<run>/home`, which does not
  /// exist until a test creates it. Call it once, before anything resolves the Codex home.
  static func activate() {
    lock.lock()
    let alreadyActive = runPath != nil
    lock.unlock()
    if alreadyActive { fail("TestCodexHome.activate() was called twice") }

    var template = Array(
      FileManager.default.temporaryDirectory
        .appendingPathComponent("codex-monitor-test-home.XXXXXX").path.utf8CString)
    let created = template.withUnsafeMutableBufferPointer { mkdtemp($0.baseAddress!) }
    if created == nil { fail("mkdtemp failed for this run's Codex home directory") }
    let run = String(cString: template)
    if let problem = pathProblem(run) { fail("The run directory \(run) \(problem)") }

    cleanupPath = run
    atexit { TestCodexHome.removeRunDirectory() }
    lock.lock()
    runPath = run
    lock.unlock()
    setenv("CODEX_HOME", run + "/home", 1)
  }

  /// This run's default Codex home, `<run>/home`.
  static var home: URL {
    URL(fileURLWithPath: activeRunPath("TestCodexHome.home") + "/home", isDirectory: true)
  }

  /// This run's private directory, or nil before `activate()`.
  static var runDirectory: String? {
    lock.lock()
    defer { lock.unlock() }
    return runPath
  }

  /// Points `CODEX_HOME` at a new home `<run>/<purpose>-<n>`, made when `create` is true, and
  /// returns it. Give it back with `leave(_:)`, innermost first.
  static func enter(_ purpose: String, create: Bool) -> URL {
    let run = activeRunPath("TestCodexHome.enter(_:create:)")
    if purpose.isEmpty || purpose.contains("/") { fail("A test home purpose must be one path component") }
    lock.lock()
    nextScope += 1
    let scoped = "\(run)/\(purpose)-\(nextScope)"
    lock.unlock()
    if create && mkdir(scoped, 0o700) != 0 {
      fail("Cannot create the test Codex home \(scoped): errno \(errno)")
    }
    lock.lock()
    entered.append((scoped, ProcessInfo.processInfo.environment["CODEX_HOME"]))
    lock.unlock()
    setenv("CODEX_HOME", scoped, 1)
    return URL(fileURLWithPath: scoped, isDirectory: true)
  }

  /// Points `CODEX_HOME` back at the value the matching `enter(_:create:)` replaced, then
  /// removes `home`.
  static func leave(_ home: URL) {
    lock.lock()
    let last = entered.last
    let matches = last?.home == home.path
    if matches { entered.removeLast() }
    lock.unlock()
    guard matches, let last else {
      fail("TestCodexHome.leave(_:) must give back the innermost enter(_:create:) home, not \(home.path)")
    }
    if let previous = last.previous { setenv("CODEX_HOME", previous, 1) } else { unsetenv("CODEX_HOME") }
    try? FileManager.default.removeItem(atPath: last.home)
  }

  /// Tripwire in `CodexClient.codexHome`: `raw`, the `CODEX_HOME` value, must name a path inside
  /// this run's directory.
  static func requireIsolated(_ raw: String, seam: String) {
    guard let run = runDirectory else {
      fail("\(seam) resolved CODEX_HOME=\(raw) before TestCodexHome.activate()")
    }
    if let problem = isolationProblem(raw, run: run) {
      fail("\(seam) resolved CODEX_HOME=\(raw), which \(problem)")
    }
  }

  /// Tripwire for a path that names the live `~/.codex` whatever `CODEX_HOME` says.
  static func forbid(_ seam: String) {
    fail("\(seam) names the live ~/.codex; tests must not reach it")
  }

  /// Why `raw` is not a path inside `run`, or nil when it is. Purely lexical: it reads nothing,
  /// so it can judge a live path safely. `run` must itself pass `pathProblem`.
  static func isolationProblem(_ raw: String, run: String) -> String? {
    if let problem = pathProblem(raw) { return problem }
    guard raw.hasPrefix(run + "/") else { return "is outside this run's directory \(run)" }
    return nil
  }

  /// Why `path` is not an absolute path in canonical spelling, or nil when it is.
  private static func pathProblem(_ path: String) -> String? {
    guard path.hasPrefix("/") else { return "is not an absolute path" }
    let components = path.split(separator: "/", omittingEmptySubsequences: false).dropFirst()
    if components.contains(where: { $0.isEmpty || $0 == "." || $0 == ".." }) {
      return "has an empty, '.', or '..' component"
    }
    return nil
  }

  private static func activeRunPath(_ caller: String) -> String {
    guard let run = runDirectory else { fail("\(caller) ran before TestCodexHome.activate()") }
    return run
  }

  private static func removeRunDirectory() {
    if let path = cleanupPath { try? FileManager.default.removeItem(atPath: path) }
  }

  private static func fail(_ message: String) -> Never {
    print("❌ Codex home isolation failed: \(message)")
    exit(1)
  }
}
