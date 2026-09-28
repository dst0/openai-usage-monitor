import Foundation

/// Tests for tests/TestPreferencesSuite.swift itself: its Monitor guard and its cleanup.
///
/// The guard cases run copies of this binary laid out so that its standard store would be the
/// Monitor's own preferences. In that mode the copy only builds a `TestPreferencesSuite`, which
/// never touches the standard store, so even a broken guard could not change them.
@main
struct TestPreferencesSuiteTests {
  static let probeMode = "CODEX_TEST_DEFAULTS_PROBE"
  static var workDirectory: URL?

  static func main() {
    if let mode = ProcessInfo.processInfo.environment[probeMode] {
      runProbe(mode)
    }
    print("🧪 Running test defaults suite guard and cleanup tests...")
    let work = FileManager.default.temporaryDirectory.appendingPathComponent(
      "codex-test-defaults-guard-\(UUID().uuidString)")
    try! FileManager.default.createDirectory(at: work, withIntermediateDirectories: false)
    workDirectory = work
    // Assertions and the suite's own guard call exit(1); remove the copies either way.
    atexit {
      if let work = TestPreferencesSuiteTests.workDirectory { try? FileManager.default.removeItem(at: work) }
    }
    let monitor = TestPreferencesSuite.monitorBundleIdentifier()

    let control = layOutCopy(in: work.appendingPathComponent("control"), named: "defaults_probe")
    let created = runCopy(control, mode: "suite")
    require(created.status == 0, "A plain copy must make a suite: \(created.output)")
    let createdDirectory = suiteDirectory(in: created.output)
    require(createdDirectory != nil, "The probe must report its suite directory: \(created.output)")
    require(
      !FileManager.default.fileExists(atPath: createdDirectory ?? "/"),
      "tearDown() must remove the suite directory")

    let abandoned = runCopy(control, mode: "abandon")
    require(abandoned.status == 3, "The abandoning probe must exit 3: \(abandoned.output)")
    let abandonedDirectory = suiteDirectory(in: abandoned.output)
    require(abandonedDirectory != nil, "The abandoning probe must report its suite directory")
    require(
      !FileManager.default.fileExists(atPath: abandonedDirectory ?? "/"),
      "Exit cleanup must remove a suite directory that was never torn down")

    // A bare executable next to Resources/Info.plist takes that plist's bundle identifier.
    let bundledDirectory = work.appendingPathComponent("bundled")
    let bundled = layOutCopy(in: bundledDirectory, named: "defaults_probe")
    let resources = bundledDirectory.appendingPathComponent("Resources")
    try! FileManager.default.createDirectory(at: resources, withIntermediateDirectories: false)
    let info: NSDictionary = ["CFBundleIdentifier": monitor, "CFBundleExecutable": "defaults_probe"]
    try! info.write(to: resources.appendingPathComponent("Info.plist"))
    let bundleCheck = runCopy(bundled, mode: "identity")
    require(
      bundleCheck.output.contains("BUNDLE=\(monitor)"),
      "The layout must give the copy the Monitor's identifier, or this case proves nothing: \(bundleCheck.output)")
    expectRefused(runCopy(bundled, mode: "suite"), "a copy bundled as the Monitor")

    let named = layOutCopy(in: work.appendingPathComponent("named"), named: monitor)
    expectRefused(runCopy(named, mode: "suite"), "a copy named after the Monitor")

    print("  ✅ Monitor guard and suite cleanup verified")
    print("\n🎉 ALL TEST DEFAULTS SUITE TESTS PASSED!")
  }

  /// Child side: `suite` makes and removes a suite, `abandon` exits without removing it, and
  /// `identity` reports the bundle identifier this copy runs as.
  static func runProbe(_ mode: String) -> Never {
    if mode == "identity" {
      print("BUNDLE=\(Bundle.main.bundleIdentifier ?? "none")")
      exit(0)
    }
    let suite = TestPreferencesSuite(purpose: "probe")
    let directory = (suite.name as NSString).deletingLastPathComponent
    print("SUITE_DIR=\(directory)")
    if mode == "abandon" {
      exit(3)
    }
    suite.tearDown()
    exit(0)
  }

  static func layOutCopy(in directory: URL, named name: String) -> URL {
    try! FileManager.default.createDirectory(at: directory, withIntermediateDirectories: false)
    let copy = directory.appendingPathComponent(name)
    try! FileManager.default.copyItem(at: Bundle.main.executableURL!, to: copy)
    return copy
  }

  static func runCopy(_ executable: URL, mode: String) -> (status: Int32, output: String) {
    let process = Process()
    process.executableURL = executable
    var environment = ProcessInfo.processInfo.environment
    environment[probeMode] = mode
    process.environment = environment
    let pipe = Pipe()
    process.standardOutput = pipe
    process.standardError = pipe
    try! process.run()
    let data = pipe.fileHandleForReading.readDataToEndOfFile()
    process.waitUntilExit()
    return (process.terminationStatus, String(decoding: data, as: UTF8.self))
  }

  static func suiteDirectory(in output: String) -> String? {
    output.split(separator: "\n").first { $0.hasPrefix("SUITE_DIR=") }
      .map { String($0.dropFirst("SUITE_DIR=".count)) }
  }

  static func expectRefused(_ result: (status: Int32, output: String), _ layout: String) {
    require(result.status == 1, "\(layout) must be stopped: \(result.output)")
    require(
      result.output.contains("must not run as the Monitor"), "\(layout) must name the reason: \(result.output)")
    require(suiteDirectory(in: result.output) == nil, "\(layout) must stop before making a suite")
  }

  static func require(_ condition: Bool, _ message: String) {
    if !condition {
      print("❌ Assertion Failed: \(message)")
      exit(1)
    }
  }
}
