import ApplicationServices
import Cocoa

/// Accessibility providers can return a value of the wrong Core Foundation
/// type. Check both the CF type and AX payload type before reading its bytes.
func decodeAXPoint(_ raw: AnyObject?) -> CGPoint? {
  guard let raw, CFGetTypeID(raw) == AXValueGetTypeID() else { return nil }
  let value = raw as! AXValue
  guard AXValueGetType(value) == .cgPoint else { return nil }
  var point = CGPoint.zero
  guard AXValueGetValue(value, .cgPoint, &point),
    point.x.isFinite, point.y.isFinite else { return nil }
  return point
}

func decodeAXSize(_ raw: AnyObject?) -> CGSize? {
  guard let raw, CFGetTypeID(raw) == AXValueGetTypeID() else { return nil }
  let value = raw as! AXValue
  guard AXValueGetType(value) == .cgSize else { return nil }
  var size = CGSize.zero
  guard AXValueGetValue(value, .cgSize, &size),
    size.width.isFinite, size.height.isFinite else { return nil }
  return size
}
