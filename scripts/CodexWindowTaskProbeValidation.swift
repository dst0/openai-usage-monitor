import CoreGraphics
import Foundation

/// ChatGPT copies exactly this prefix plus the selected local task ID.
private let copiedTaskLinkPrefix = Array("codex://threads/".utf8)

/// Returns the canonical task ID only when the shortcut produced exactly one
/// clipboard write and nothing wrote again while the link was being read.
/// More writes are ambiguous because another app may have raced with
/// ChatGPT's copy action; one competing write of a valid link is still
/// indistinguishable, so callers must not treat the ID as window-attributed.
func validateCopiedTaskLink(
  _ link: String?, beforeChange: Int, afterChange: Int, confirmedChange: Int
) -> String? {
  guard beforeChange < Int.max, afterChange == beforeChange + 1,
    confirmedChange == afterChange, let link else { return nil }
  let bytes = Array(link.utf8)
  guard bytes.starts(with: copiedTaskLinkPrefix) else { return nil }
  let id = bytes.dropFirst(copiedTaskLinkPrefix.count)
  guard id.count == 36 else { return nil }
  for (index, byte) in id.enumerated() {
    if [8, 13, 18, 23].contains(index) {
      guard byte == 45 else { return nil }
    } else {
      guard (48...57).contains(byte) || (97...102).contains(byte) else { return nil }
    }
  }
  return String(decoding: id, as: UTF8.self)
}

/// Pairs each WindowServer window ID with exactly one Accessibility standard
/// window by frame and returns the Accessibility index for each ID, in order.
/// Returns nil when an ID has no frame, matches zero or several Accessibility
/// frames, or shares its match with another ID. Equal frames, such as native
/// window tabs, therefore fail closed instead of being paired by guesswork.
func uniqueWindowFrameMapping(
  ids: [UInt32], windowServerFrames: [UInt32: CGRect], accessibilityFrames: [CGRect]
) -> [Int]? {
  guard !ids.isEmpty, Set(ids).count == ids.count,
    windowServerFrames.count == ids.count,
    accessibilityFrames.count == ids.count else { return nil }
  var claimed = Set<Int>()
  var mapping: [Int] = []
  for id in ids {
    guard let frame = windowServerFrames[id] else { return nil }
    let matches = accessibilityFrames.indices.filter { framesMatch(accessibilityFrames[$0], frame) }
    guard matches.count == 1, claimed.insert(matches[0]).inserted else { return nil }
    mapping.append(matches[0])
  }
  return mapping
}
