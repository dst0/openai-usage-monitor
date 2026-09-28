import Foundation

/// Tests for tests/TestCodexHome.swift and the Codex home tripwires it backs in `Sources/`.
///
/// The path decision is checked in this process. Each tripwire's stop, and each misuse the guard
/// refuses, is checked in a child: this binary run again in a probe mode. The child's home is a
/// temporary directory: `HOME` for `SingleInstanceGuard`, and `CFFIXED_USER_HOME` for
/// `homeDirectoryForCurrentUser`, which on macOS ignores `HOME` (the passing probe checks that it
/// took effect). A tripwire stops the child before the path is built. If one were removed, its
/// probe would build a path in that directory or check whether the installed Monitor's help page
/// exists, and would never write anything.
@main
struct TestCodexHomeTests {
  static let probeMode = "CODEX_TEST_HOME_PROBE"
  static let probeHome = "CODEX_TEST_HOME_PROBE_HOME"
  /// A probe prints this only after the call under test returns.
  static let returned = "PROBE_RETURNED"

  static func main() {
    if let mode = ProcessInfo.processInfo.environment[probeMode] {
      runProbe(mode)
    }
    print("🧪 Running Codex home guard and tripwire tests...")
    TestCodexHome.activate()
    checkPathDecision()
    checkHomes()
    checkTripwires()
    print("\n🎉 ALL CODEX HOME GUARD TESTS PASSED!")
  }

  // MARK: - The guard in this process

  static func checkPathDecision() {
    let run = "/private/var/folders/zz/T/codex-monitor-test-home.AbC123"
    for inside in ["\(run)/home", "\(run)/settings-1", "\(run)/probe-3/home-lock/.codex"] {
      require(TestCodexHome.isolationProblem(inside, run: run) == nil, "\(inside) is inside the run")
    }
    let outside = [
      ("", "not an absolute path"),
      ("home", "not an absolute path"),
      ("./home", "not an absolute path"),
      ("/Users/someone/.codex", "outside this run's directory"),
      (run, "outside this run's directory"),
      ("\(run)-evil/home", "outside this run's directory"),
      ("\(run)/home/", "component"),
      ("\(run)//home", "component"),
      ("\(run)/./home", "component"),
      ("\(run)/home/../../escape", "component"),
      ("\(run)/../codex-monitor-test-home.AbC123/home", "component"),
    ]
    for (raw, reason) in outside {
      let problem = TestCodexHome.isolationProblem(raw, run: run)
      require(problem?.contains(reason) == true, "'\(raw)' must be rejected as \(reason), got \(problem ?? "nil")")
    }
    print("  ✅ Only canonical absolute paths inside the run directory pass")
  }

  static func checkHomes() {
    let home = TestCodexHome.home
    guard let run = TestCodexHome.runDirectory else {
      require(false, "activate() must record the run directory")
      return
    }
    require(home.path == run + "/home", "The run's home must be <run>/home")
    require(ProcessInfo.processInfo.environment["CODEX_HOME"] == home.path, "activate() must set CODEX_HOME")
    require(CodexClient.codexHome.path == home.path, "The client must resolve the run's home")
    require(!FileManager.default.fileExists(atPath: home.path), "The run's home must start absent")
    var info = stat()
    require(lstat(run, &info) == 0, "The run directory must exist")
    require(info.st_mode & S_IFMT == S_IFDIR, "The run directory must be a real directory")
    require(info.st_uid == geteuid() && info.st_mode & 0o777 == 0o700, "The run directory must be private")

    let outer = TestCodexHome.enter("outer", create: true)
    require(outer.deletingLastPathComponent().path == run, "A test home must be in the run directory")
    require(FileManager.default.fileExists(atPath: outer.path), "create: true must make the home")
    require(CodexClient.codexHome.path == outer.path, "enter must point CODEX_HOME at the new home")
    let inner = TestCodexHome.enter("inner", create: false)
    require(!FileManager.default.fileExists(atPath: inner.path), "create: false must not make the home")
    require(CodexClient.codexHome.path == inner.path, "A nested enter must take over")
    TestCodexHome.leave(inner)
    require(CodexClient.codexHome.path == outer.path, "leave must restore the enclosing home")
    TestCodexHome.leave(outer)
    require(!FileManager.default.fileExists(atPath: outer.path), "leave must remove the home")
    require(CodexClient.codexHome.path == home.path, "leave must restore the run's home")
    print("  ✅ Private run directory and nested test homes verified")
  }

  // MARK: - Tripwires, each stopping a child

  static func checkTripwires() {
    let work = TestCodexHome.enter("probe-work", create: true)
    defer { TestCodexHome.leave(work) }

    // The harness itself: a probe that stays inside its home runs to the end.
    let allowed = runChild("allowed", work: work)
    require(allowed.status == 0, "A probe inside its home must pass: \(allowed.output)")
    require(allowed.output.contains(returned), "The passing probe must finish: \(allowed.output)")
    requireRunRemoved(allowed.output, "allowed")

    // With CODEX_HOME unset, as on a runner or a desktop, the client would use the live ~/.codex.
    expectStopped(
      runChild("home", work: work, codexHome: .some(nil)), "unset",
      "CodexClient.codexHome with CODEX_HOME unset names the live ~/.codex", activated: false)
    // A home set before activate() is not trusted: it may be the live one.
    expectStopped(
      runChild("home", work: work), "unactivated",
      "CodexClient.codexHome resolved CODEX_HOME=\(work.path) before TestCodexHome.activate()",
      activated: false)
    // After activate(), a home set around the guard is rejected before it is read.
    let foreign = FileManager.default.temporaryDirectory.appendingPathComponent("codex-foreign-home").path
    expectStopped(
      runChild("foreign", work: work), "foreign",
      "CodexClient.codexHome resolved CODEX_HOME=\(foreign), which is outside this run's directory")
    expectStopped(
      runChild("lock", work: work), "lock", "SingleInstanceGuard.defaultLockPath names the live ~/.codex")
    require(
      !FileManager.default.fileExists(atPath: work.appendingPathComponent("home-lock/.codex").path),
      "The lock tripwire must stop before the lock directory is made")
    expectStopped(
      runChild("helps", work: work), "helps",
      "HelpsDocHelper.findHelpsHTMLURL home fallback names the live ~/.codex")
    print("  ✅ Every Codex home tripwire stops its probe before the path is built")

    // Misuse the guard refuses, so a test cannot remove or reuse a home it does not own.
    expectStopped(runChild("twice", work: work), "twice", "TestCodexHome.activate() was called twice")
    expectStopped(
      runChild("purpose", work: work), "purpose", "A test home purpose must be one path component")
    expectStopped(
      runChild("misordered", work: work), "misordered",
      "TestCodexHome.leave(_:) must give back the innermost enter(_:create:) home")
    print("  ✅ The guard refuses a second activate(), a purpose that is a path, and an out-of-order leave")
  }

  /// Child side. Every mode but `home` activates first; `returned` follows the call under test.
  static func runProbe(_ mode: String) -> Never {
    let scratch = ProcessInfo.processInfo.environment[probeHome] ?? "/nonexistent"
    if mode != "home" {
      TestCodexHome.activate()
      print("RUN_DIR=\(TestCodexHome.runDirectory ?? "")")
    }
    switch mode {
    case "allowed":
      guard FileManager.default.homeDirectoryForCurrentUser.path == scratch else {
        print("CFFIXED_USER_HOME must move the home directory, got \(FileManager.default.homeDirectoryForCurrentUser.path)")
        exit(2)
      }
      _ = CodexClient.codexHome
      _ = SingleInstanceGuard(lockPath: TestCodexHome.home.appendingPathComponent("monitor.lock").path)
      // The help page next to the executable is found before the home fallback is reached.
      let found = HelpsDocHelper.findHelpsHTMLURL(
        fileManager: HomeOverridingFileManager(home: scratch), bundle: emptyBundle(scratch),
        arguments: [scratch + "/App/MacOS/CodexMonitor"])
      guard found?.path == scratch + "/App/Resources/helps.html" else {
        print("The help page next to the executable must be found first, got \(found?.path ?? "nil")")
        exit(2)
      }
    case "home":
      _ = CodexClient.codexHome
    case "foreign":
      setenv(
        "CODEX_HOME",
        FileManager.default.temporaryDirectory.appendingPathComponent("codex-foreign-home").path, 1)
      _ = CodexClient.codexHome
    case "lock":
      _ = SingleInstanceGuard.defaultLockPath
    case "helps":
      _ = HelpsDocHelper.findHelpsHTMLURL(
        fileManager: HomeOverridingFileManager(home: scratch), bundle: emptyBundle(scratch),
        arguments: [scratch + "/MacOS/Missing"])
    case "twice":
      TestCodexHome.activate()
    case "purpose":
      _ = TestCodexHome.enter("../escape", create: true)
    case "misordered":
      let outer = TestCodexHome.enter("outer", create: true)
      _ = TestCodexHome.enter("inner", create: true)
      TestCodexHome.leave(outer)
    default:
      print("Unknown probe mode \(mode)")
      exit(2)
    }
    print(returned)
    exit(0)
  }

  /// A bundle for `<scratch>/EmptyBundle`, which has no resources.
  static func emptyBundle(_ scratch: String) -> Bundle {
    let path = scratch + "/EmptyBundle"
    guard let bundle = Bundle(path: path) else {
      print("Cannot open the empty bundle \(path)")
      exit(2)
    }
    return bundle
  }

  /// Runs this binary in probe `mode` with its own scratch home in `work` (`HOME` and
  /// `CFFIXED_USER_HOME`). `codexHome` replaces `CODEX_HOME` when given; `.some(nil)` removes it.
  /// A child still running after 60 seconds is killed and fails the test.
  static func runChild(
    _ mode: String, work: URL, codexHome: String?? = .none
  ) -> (status: Int32, output: String) {
    // Only App/ has a help page, so a lookup from <scratch>/MacOS falls through to the home.
    let scratch = work.appendingPathComponent("home-\(mode)")
    for directory in ["App/MacOS", "App/Resources", "EmptyBundle"] {
      try! FileManager.default.createDirectory(
        at: scratch.appendingPathComponent(directory), withIntermediateDirectories: true)
    }
    try! Data("<html></html>".utf8).write(to: scratch.appendingPathComponent("App/Resources/helps.html"))

    var environment = ProcessInfo.processInfo.environment
    environment[probeMode] = mode
    environment[probeHome] = scratch.path
    environment["HOME"] = scratch.path
    environment["CFFIXED_USER_HOME"] = scratch.path
    if case .some(let replacement) = codexHome { environment["CODEX_HOME"] = replacement }
    let process = Process()
    process.executableURL = Bundle.main.executableURL
    process.environment = environment
    let pipe = Pipe()
    process.standardOutput = pipe
    process.standardError = pipe
    try! process.run()
    let deadline = DispatchWorkItem { process.terminate() }
    DispatchQueue.global().asyncAfter(deadline: .now() + 60, execute: deadline)
    let data = pipe.fileHandleForReading.readDataToEndOfFile()
    process.waitUntilExit()
    deadline.cancel()
    let output = String(decoding: data, as: UTF8.self)
    require(
      process.terminationReason == .exit,
      "Probe \(mode) was killed after 60 seconds or crashed (signal \(process.terminationStatus)): \(output)")
    return (process.terminationStatus, output)
  }

  static func expectStopped(
    _ result: (status: Int32, output: String), _ name: String, _ message: String, activated: Bool = true
  ) {
    require(result.status == 1, "Probe \(name) must stop with status 1: \(result.output)")
    require(
      result.output.contains("❌ Codex home isolation failed: \(message)"),
      "Probe \(name) must stop at its tripwire: \(result.output)")
    require(!result.output.contains(returned), "Probe \(name) must stop before the call returns: \(result.output)")
    if activated {
      requireRunRemoved(result.output, name)
    } else {
      require(!result.output.contains("RUN_DIR="), "Probe \(name) must not have a run directory")
    }
  }

  /// A probe that activated must have removed its run directory at exit, even after a tripwire.
  static func requireRunRemoved(_ output: String, _ name: String) {
    let line = output.split(separator: "\n").first(where: { $0.hasPrefix("RUN_DIR=") })
    let path = line.map { String($0.dropFirst("RUN_DIR=".count)) } ?? ""
    require(!path.isEmpty, "Probe \(name) must report its run directory: \(output)")
    require(!FileManager.default.fileExists(atPath: path), "Probe \(name) must remove its run directory \(path)")
  }

  static func require(_ condition: Bool, _ message: String) {
    if !condition {
      print("❌ Assertion Failed: \(message)")
      exit(1)
    }
  }
}

/// A file manager whose home directory is `home`, so the help lookup's home fallback names no live
/// path even if its tripwire were removed.
final class HomeOverridingFileManager: FileManager {
  private let fakeHome: URL

  init(home: String) {
    fakeHome = URL(fileURLWithPath: home, isDirectory: true)
    super.init()
  }

  override var homeDirectoryForCurrentUser: URL { fakeHome }
}
