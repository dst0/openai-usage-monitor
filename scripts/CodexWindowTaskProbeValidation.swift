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

/// The only Desktop build whose Copy deeplink binding was inspected:
/// ChatGPT 26.924.20706 binds its hidden `copyDeeplink` command to
/// CmdOrCtrl+Alt+L by default. Another build may bind that key differently,
/// so the probe refuses it until its bundle has been re-inspected.
let verifiedDesktopBundleIdentifier = "com.openai.codex"
let verifiedDesktopVersion = "26.924.20706"

func isVerifiedDesktopBuild(bundleIdentifier: String?, version: String?) -> Bool {
  bundleIdentifier == verifiedDesktopBundleIdentifier && version == verifiedDesktopVersion
}

/// macOS App Shortcuts (`NSUserKeyEquivalents`) can assign Cmd+Opt+L to any
/// menu item. Returns true when the value is malformed or any entry is
/// exactly Cmd+Opt+L, written as `@` (Command) and `~` (Option) before `l`.
func keyEquivalentsConflictWithCopyShortcut(_ value: Any?) -> Bool {
  guard let value else { return false }
  guard let equivalents = value as? [String: Any] else { return true }
  for equivalent in equivalents.values {
    guard let text = equivalent as? String else { return true }
    let modifiers = Set(text.prefix { "@~$^".contains($0) })
    let key = text.drop { "@~$^".contains($0) }
    if modifiers == ["@", "~"] && key == "l" { return true }
  }
  return false
}

/// Password managers and similar apps mark sensitive or short-lived writes.
/// Such contents are never read, even to reject them.
private let privatePasteboardMarkers: Set<String> = [
  "org.nspasteboard.ConcealedType", "org.nspasteboard.TransientType",
]

func pasteboardTypesAllowTaskRead(_ types: [String]) -> Bool {
  types.contains("public.utf8-plain-text") && privatePasteboardMarkers.isDisjoint(with: types)
}

/// macOS 15.4 and later ask the user before a programmatic pasteboard read
/// unless the reading app is set to always allow it (behavior 2). The
/// behavior is nil on systems without the setting, where reads never ask.
func pasteboardReadIsSilent(accessBehavior: Int?) -> Bool {
  accessBehavior == nil || accessBehavior == 2
}
