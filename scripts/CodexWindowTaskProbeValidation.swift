import Foundation

/// A copy shortcut must perform exactly one clipboard write. More writes are
/// ambiguous because another app may have raced with ChatGPT's copy action.
func validateCopiedTaskLink(_ link: String?, beforeChange: Int, afterChange: Int) -> String? {
  guard afterChange == beforeChange + 1, let link,
    link.hasPrefix("codex://threads/") else { return nil }
  let id = String(link.dropFirst("codex://threads/".count))
  guard id.utf8.count == 36 else { return nil }
  for (index, byte) in id.utf8.enumerated() {
    if [8, 13, 18, 23].contains(index) {
      guard byte == 45 else { return nil }
    } else {
      guard (48...57).contains(byte) || (97...102).contains(byte) else {
        return nil
      }
    }
  }
  return id
}
