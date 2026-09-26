import ApplicationServices
import Cocoa

@main
struct CodexWindowRestoreAXValueTests {
  static func main() {
    var expectedPoint = CGPoint(x: -320, y: 96)
    var expectedSize = CGSize(width: 850, height: 620)
    guard let point = AXValueCreate(.cgPoint, &expectedPoint),
      let size = AXValueCreate(.cgSize, &expectedSize) else {
      fatalError("Could not create AX test values")
    }

    assert(decodeAXPoint(point) == expectedPoint)
    assert(decodeAXSize(size) == expectedSize)
    assert(decodeAXPoint(size) == nil)
    assert(decodeAXSize(point) == nil)
    assert(decodeAXPoint(NSNumber(value: 1)) == nil)
    assert(decodeAXSize(NSString(string: "wrong type")) == nil)

    var invalidPoint = CGPoint(x: CGFloat.infinity, y: 0)
    if let invalid = AXValueCreate(.cgPoint, &invalidPoint) {
      assert(decodeAXPoint(invalid) == nil)
    }
    print("Codex window AX value decoder tests passed")
  }
}
