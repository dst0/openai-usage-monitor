import ApplicationServices
import Cocoa

@main
struct CodexWindowSafetyChecksTests {
  static func main() {
    let pid: pid_t = 42
    let base: [String: Any] = [
      kCGWindowOwnerPID as String: NSNumber(value: pid),
      kCGWindowLayer as String: NSNumber(value: 0),
      kCGWindowAlpha as String: NSNumber(value: 1),
    ]
    func withBounds(_ bounds: Any) -> [String: Any] {
      var info = base
      info[kCGWindowBounds as String] = bounds
      return info
    }
    let validBounds: [String: Any] = [
      "X": NSNumber(value: -320), "Y": NSNumber(value: 75),
      "Width": NSNumber(value: 850), "Height": NSNumber(value: 620),
    ]
    switch bannerWindowFrame(withBounds(validBounds), expectedPID: pid) {
    case .frame(let frame):
      assert(frame == CGRect(x: -320, y: 75, width: 850, height: 620))
    default: fatalError("Valid bounds were rejected")
    }
    if case .frame(let frame) = bannerWindowFrame(
      withBounds(NSDictionary(dictionary: validBounds)), expectedPID: pid
    ) {
      assert(frame.width == 850)
    } else { fatalError("Native dictionary bounds were rejected") }

    for invalid in [NSString(string: "wrong type") as Any, NSNumber(value: 1) as Any] {
      if case .invalidGeometry = bannerWindowFrame(withBounds(invalid), expectedPID: pid) {
        // Expected: never force-cast a foreign WindowServer payload.
      } else { fatalError("Wrong bounds type was accepted") }
    }
    for replacement in [Double.infinity, Double.nan] {
      var bounds = validBounds
      bounds["X"] = NSNumber(value: replacement)
      if case .invalidGeometry = bannerWindowFrame(withBounds(bounds), expectedPID: pid) {
      } else { fatalError("Non-finite origin was accepted") }
      bounds = validBounds
      bounds["Width"] = NSNumber(value: replacement)
      if case .invalidGeometry = bannerWindowFrame(withBounds(bounds), expectedPID: pid) {
      } else { fatalError("Non-finite width was accepted") }
    }
    var booleanBounds = validBounds
    booleanBounds["Width"] = NSNumber(value: true)
    if case .invalidGeometry = bannerWindowFrame(withBounds(booleanBounds), expectedPID: pid) {
    } else { fatalError("Boolean width was accepted") }
    if case .invalidGeometry = bannerWindowFrame(base, expectedPID: pid) {
    } else { fatalError("Missing bounds were accepted") }

    var otherProcess = withBounds(NSString(string: "foreign"))
    otherProcess[kCGWindowOwnerPID as String] = NSNumber(value: 43)
    if case .notCandidate = bannerWindowFrame(otherProcess, expectedPID: pid) {
    } else { fatalError("Foreign process was classified as candidate") }

    assert(processIdentityMatches(pid, expectedBirth: "1:000001", birthReader: { _ in "1:000001" }))
    assert(!processIdentityMatches(pid, expectedBirth: "1:000001", birthReader: { _ in "1:000002" }))
    assert(!processIdentityMatches(pid, expectedBirth: "1:000001", birthReader: { _ in nil }))
    print("Codex window safety checks tests passed")
  }
}
