import Carbon
import CoreGraphics
import Foundation

@main
struct CodexWindowTaskProbeTests {
  static func main() {
    copiedLinkRequiresOneObservedWrite()
    copiedLinkRequiresCanonicalTaskURL()
    windowMappingIsOneToOne()
    windowMappingFailsClosedOnAmbiguity()
    onlyTheInspectedDesktopBuildIsAccepted()
    appShortcutsOnTheCopyKeyAreRefused()
    privateOrPromptingPasteboardIsNotRead()
    copyKeyTypesItsLetterOnlyOnMatchingLayouts()
    print("Selected-task probe validation passed")
  }

  static let id = "01a00000-0000-4000-8000-00000000000f"
  static let link = "codex://threads/\(id)"

  static func copied(_ link: String?, _ before: Int, _ after: Int, _ confirmed: Int) -> String? {
    validateCopiedTaskLink(link, beforeChange: before, afterChange: after, confirmedChange: confirmed)
  }

  static func copiedLinkRequiresOneObservedWrite() {
    precondition(copied(link, 10, 11, 11) == id)
    precondition(copied(link, 0, 1, 1) == id)
    // No write, a competing second write, or a counter that moved backwards.
    precondition(copied(link, 10, 10, 10) == nil)
    precondition(copied(link, 10, 12, 12) == nil)
    precondition(copied(link, 10, 9, 9) == nil)
    // Another write landed between reading the count and reading the text.
    precondition(copied(link, 10, 11, 12) == nil)
    // The largest counter cannot overflow into an accepted value.
    precondition(copied(link, Int.max, Int.min, Int.min) == nil)
    precondition(copied(link, Int.max, Int.max, Int.max) == nil)
    precondition(copied(nil, 10, 11, 11) == nil)
  }

  static func copiedLinkRequiresCanonicalTaskURL() {
    let rejected = [
      "",
      "codex://threads/",
      "codex://threads/not-a-task",
      "\(link)?window=1",
      "\(link)\n",
      " \(link)",
      "codex://threads/01A00000-0000-4000-8000-00000000000F",
      "CODEX://threads/\(id)",
      "https://example.com/threads/\(id)",
      "codex://threads/new",
      "codex://threads/01a00000-0000-4000-8000-00000000000",
      "codex://threads/01a00000-0000-4000-8000-00000000000f0",
      "codex://threads/01a000000-000-4000-8000-00000000000f",
      "codex://threads/01a00000_0000_4000_8000_00000000000f",
      "codex://threads/０1a00000-0000-4000-8000-00000000000f",
      "codex://threads/01a00000-0000-4000-8000-00000000000g",
    ]
    for candidate in rejected {
      precondition(copied(candidate, 10, 11, 11) == nil, "accepted \(candidate)")
    }
  }

  static func windowMappingIsOneToOne() {
    let left = CGRect(x: 0, y: 25, width: 900, height: 700)
    let right = CGRect(x: 1000, y: 25, width: 900, height: 700)
    // Accessibility order differs from WindowServer order; mapping follows frames.
    let mapping = uniqueWindowFrameMapping(
      ids: [31, 32], windowServerFrames: [31: left, 32: right],
      accessibilityFrames: [right, left.offsetBy(dx: 1.5, dy: -2)])
    precondition(mapping == [1, 0])
    precondition(uniqueWindowFrameMapping(
      ids: [7], windowServerFrames: [7: left], accessibilityFrames: [left]) == [0])
  }

  static func windowMappingFailsClosedOnAmbiguity() {
    let left = CGRect(x: 0, y: 25, width: 900, height: 700)
    let right = CGRect(x: 1000, y: 25, width: 900, height: 700)
    let nearlyLeft = left.offsetBy(dx: 2, dy: 2)
    func map(_ ids: [UInt32], _ frames: [UInt32: CGRect], _ ax: [CGRect]) -> [Int]? {
      uniqueWindowFrameMapping(ids: ids, windowServerFrames: frames, accessibilityFrames: ax)
    }
    // Native window tabs share one frame, so neither window can be identified.
    precondition(map([31, 32], [31: left, 32: left], [left, left]) == nil)
    // One WindowServer frame matches two Accessibility windows.
    precondition(map([31, 32], [31: left, 32: right], [left, nearlyLeft]) == nil)
    // Two WindowServer windows claim the same Accessibility window.
    precondition(map([31, 32], [31: left, 32: nearlyLeft], [left, right]) == nil)
    // Outside the two-point tolerance there is no match at all.
    precondition(map([31], [31: left], [left.offsetBy(dx: 3, dy: 0)]) == nil)
    // Missing, extra, or duplicated inputs.
    precondition(map([], [:], []) == nil)
    precondition(map([31, 32], [31: left, 33: right], [left, right]) == nil)
    precondition(map([31, 32], [31: left, 32: right], [left]) == nil)
    precondition(map([31], [31: left, 32: right], [left]) == nil)
    precondition(map([31, 31], [31: left], [left, left]) == nil)
  }

  static func onlyTheInspectedDesktopBuildIsAccepted() {
    precondition(isVerifiedDesktopBuild(
      bundleIdentifier: "com.openai.codex", version: "26.924.20706", buildNumber: "11431"))
    for (identifier, version, build) in [
      ("com.openai.codex", "26.924.20707", "11431"), ("com.openai.codex", "26.924.2070", "11431"),
      ("com.openai.chat", "26.924.20706", "11431"), ("com.openai.codex", "26.924.20706", "11432"),
      ("com.openai.codex", nil, "11431"), (nil, "26.924.20706", "11431"),
      ("com.openai.codex", "26.924.20706", nil),
    ] as [(String?, String?, String?)] {
      precondition(!isVerifiedDesktopBuild(bundleIdentifier: identifier, version: version, buildNumber: build))
    }
    let launch = Date(timeIntervalSince1970: 1_790_000_000)
    precondition(bundleUnchangedSinceLaunch(infoModified: launch.addingTimeInterval(-60), launched: launch))
    precondition(bundleUnchangedSinceLaunch(infoModified: launch, launched: launch))
    // Replaced on disk after launch: the running code is not what was read.
    precondition(!bundleUnchangedSinceLaunch(infoModified: launch.addingTimeInterval(1), launched: launch))
    precondition(!bundleUnchangedSinceLaunch(infoModified: nil, launched: launch))
    precondition(!bundleUnchangedSinceLaunch(infoModified: launch, launched: nil))
  }

  static func appShortcutsOnTheCopyKeyAreRefused() {
    precondition(!keyEquivalentsConflictWithCopyShortcut(nil))
    precondition(!keyEquivalentsConflictWithCopyShortcut([String: Any]()))
    precondition(!keyEquivalentsConflictWithCopyShortcut(
      ["Paste and Match Style": "@~$v", "Lock": "@~$L", "Other": "@l", "Option": "~l"]))
    for conflict in ["@~l", "~@l", "@@~l"] {
      precondition(keyEquivalentsConflictWithCopyShortcut(["Archive": conflict]), conflict)
    }
    // Unreadable settings fail closed.
    precondition(keyEquivalentsConflictWithCopyShortcut(["Archive": 7]))
    precondition(keyEquivalentsConflictWithCopyShortcut("@~l"))
  }

  static func privateOrPromptingPasteboardIsNotRead() {
    precondition(pasteboardTypesAllowTaskRead(["public.utf8-plain-text"]))
    precondition(!pasteboardTypesAllowTaskRead([]))
    precondition(!pasteboardTypesAllowTaskRead(["public.png"]))
    precondition(!pasteboardTypesAllowTaskRead(
      ["public.utf8-plain-text", "org.nspasteboard.ConcealedType"]))
    precondition(!pasteboardTypesAllowTaskRead(
      ["org.nspasteboard.TransientType", "public.utf8-plain-text"]))
    for legacy in ["de.petermaurer.TransientPasteboardType", "com.agilebits.onepassword"] {
      precondition(!pasteboardTypesAllowTaskRead(["public.utf8-plain-text", legacy]))
    }
    // NSPasteboard.AccessBehavior: 0 default (asks), 1 ask, 2 always allow, 3 deny.
    for behavior in [nil, 0, 1, 2] as [Int?] {
      precondition(pasteboardReadIsPermitted(accessBehavior: behavior))
    }
    for behavior in [3, -1, 4] {
      precondition(!pasteboardReadIsPermitted(accessBehavior: behavior))
    }
  }

  /// Uses the layouts bundled with macOS, read-only; the current input
  /// source is never changed.
  static func copyKeyTypesItsLetterOnlyOnMatchingLayouts() {
    func layout(_ id: String) -> TISInputSource {
      let filter = [kTISPropertyInputSourceID as String: id] as CFDictionary
      guard let list = TISCreateInputSourceList(filter, true)?.takeRetainedValue()
        as? [TISInputSource], let source = list.first else {
        fatalError("bundled layout \(id) is missing")
      }
      return source
    }
    precondition(commandCharacter(forKeyCode: 37, layout: layout("com.apple.keylayout.US")) == "l")
    precondition(commandCharacter(forKeyCode: 37, layout: layout("com.apple.keylayout.Dvorak")) == "n")
    precondition(commandCharacter(forKeyCode: 37, layout: layout("com.apple.keylayout.Colemak")) == "i")
    // Dvorak - QWERTY Command switches to QWERTY while Command is held.
    precondition(
      commandCharacter(forKeyCode: 37, layout: layout("com.apple.keylayout.DVORAK-QWERTYCMD")) == "l")
  }
}
