import ApplicationServices
import Cocoa

enum BannerWindowFrameResult {
  case notCandidate
  case invalidGeometry
  case frame(CGRect)
}

/// Classifies a WindowServer record without trusting its bounds object or
/// numbers. A malformed window belonging to the target process blocks banner
/// placement rather than being silently skipped.
func bannerWindowFrame(_ info: [String: Any], expectedPID: pid_t) -> BannerWindowFrameResult {
  guard (info[kCGWindowOwnerPID as String] as? NSNumber)?.int32Value == expectedPID,
    (info[kCGWindowLayer as String] as? NSNumber)?.intValue == 0,
    (info[kCGWindowAlpha as String] as? NSNumber)?.doubleValue ?? 0 > 0 else {
    return .notCandidate
  }
  guard let bounds = info[kCGWindowBounds as String] as? [String: Any],
    let x = finiteWindowNumber(bounds["X"]),
    let y = finiteWindowNumber(bounds["Y"]),
    let width = finiteWindowNumber(bounds["Width"]),
    let height = finiteWindowNumber(bounds["Height"]),
    width > 0, height > 0 else { return .invalidGeometry }
  let frame = CGRect(x: x, y: y, width: width, height: height)
  guard frame.maxX.isFinite, frame.maxY.isFinite,
    (frame.width * frame.height).isFinite else { return .invalidGeometry }
  guard width >= 300, height >= 250 else { return .notCandidate }
  return .frame(frame)
}

private func finiteWindowNumber(_ value: Any?) -> CGFloat? {
  guard let number = value as? NSNumber,
    CFGetTypeID(number) != CFBooleanGetTypeID() else { return nil }
  let value = CGFloat(number.doubleValue)
  return value.isFinite ? value : nil
}

func processIdentityMatches(
  _ pid: pid_t, expectedBirth: String, birthReader: (pid_t) -> String?
) -> Bool {
  !expectedBirth.isEmpty && birthReader(pid) == expectedBirth
}
