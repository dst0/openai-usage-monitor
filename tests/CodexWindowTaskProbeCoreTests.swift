import Foundation

/// What the fake Desktop does when it receives the copy shortcut.
enum FakeCopy {
  case write(String)
  case noWrite
  case doubleWrite(String)
  case concealed(String)
  /// Clears first; the text becomes readable after `polls` type checks.
  case delayedText(String, polls: Int)
  /// Another app writes a different valid link after the count was read
  /// but before the text read.
  case overwrittenDuringRead(String)
}

/// A scripted macOS for `WindowTaskProbe`. It logs every call so tests can
/// check what happened before the first visible change and in what order
/// the clipboard was read.
final class FakeProbeSystem: WindowTaskProbeSystem {
  typealias Window = Int
  var log: [String] = []
  var clock: TimeInterval = 0
  var optedIn = true
  var unmet: WindowTaskProbeFailure?
  var births: [Bool] = []
  var ids: [UInt32] = [31, 32]
  var idsAfterFirstPost: [UInt32]?
  var mapping: WindowTaskProbeFailure?
  var grantsFocus = true
  /// After this many positive focus checks the window loses focus.
  var focusChecksBeforeLoss: Int?
  var losesFocusOnCopy = false
  var layoutChecks: [Bool] = []
  var postSucceeds = true
  var copies: [FakeCopy] = []
  var changeCount = 700
  var text: String? = "an earlier clipboard entry"
  var types = ["public.utf8-plain-text"]
  var pendingTextPolls = 0
  var pendingText: String?
  var overwriteOnRead: String?
  var focused: Int?
  var posts = 0


  func now() -> TimeInterval { clock }
  func pause(_ seconds: TimeInterval) { clock += seconds }
  func isOptedIn() -> Bool { log.append("opt-in"); return optedIn }
  func unmetPrecondition() -> WindowTaskProbeFailure? { log.append("preconditions"); return unmet }
  func processBirthMatches() -> Bool {
    log.append("birth")
    return births.isEmpty ? true : births.removeFirst()
  }
  func windowIDs() throws -> [UInt32] {
    log.append("ids")
    if posts > 0, let changed = idsAfterFirstPost { return changed }
    return ids
  }
  func mappedWindows(_ ids: [UInt32]) throws -> [Int] {
    log.append("map")
    if let mapping { throw mapping }
    return Array(ids.indices)
  }
  func sameWindow(_ left: Int, _ right: Int) -> Bool { left == right }
  func beginVisibleChanges() { log.append("visible") }
  func requestFocus(_ window: Int) {
    log.append("focus \(window)")
    if grantsFocus { focused = window }
  }
  func hasKeyboardFocus(_ window: Int) -> Bool {
    guard focused == window else { return false }
    if let remaining = focusChecksBeforeLoss {
      guard remaining > 0 else { focused = nil; return false }
      focusChecksBeforeLoss = remaining - 1
    }
    return true
  }
  func copyShortcutKeyIsExpected() -> Bool {
    log.append("layout")
    return layoutChecks.isEmpty ? true : layoutChecks.removeFirst()
  }
  func postCopyShortcut() -> Bool {
    log.append("post")
    guard postSucceeds else { return false }
    posts += 1
    if losesFocusOnCopy { focused = nil }
    switch copies.isEmpty ? FakeCopy.noWrite : copies.removeFirst() {
    case .write(let link): write(link)
    case .noWrite: break
    case .doubleWrite(let link): write(link); changeCount += 1
    case .concealed(let link): write(link); types.append("org.nspasteboard.ConcealedType")
    case .delayedText(let link, let polls):
      changeCount += 1
      text = nil
      types = []
      pendingText = link
      pendingTextPolls = polls
    case .overwrittenDuringRead(let link):
      write(link)
      overwriteOnRead = "codex://threads/01a00000-0000-4000-8000-0000000000ff"
    }
    return true
  }
  func pasteboardChangeCount() -> Int { log.append("count"); return changeCount }
  func pasteboardOffersTaskText() -> Bool {
    log.append("types")
    if let pending = pendingText {
      pendingTextPolls -= 1
      if pendingTextPolls <= 0 { write(pending, counting: false); pendingText = nil }
    }
    return pasteboardTypesAllowTaskRead(types)
  }
  func pasteboardString() -> String? {
    log.append("text")
    if let other = overwriteOnRead { write(other); overwriteOnRead = nil }
    return text
  }

  private func write(_ value: String, counting: Bool = true) {
    if counting { changeCount += 1 }
    text = value
    types = ["public.utf8-plain-text"]
  }
}

@main
struct CodexWindowTaskProbeCoreTests {
  static let first = "codex://threads/01a00000-0000-4000-8000-000000000001"
  static let second = "codex://threads/01a00000-0000-4000-8000-000000000002"

  static func main() {
    probesEveryWindowOnceAndReportsOnlyTheCount()
    refusesBeforeAnyVisibleChange()
    requiresObservedFocusBeforeEachShortcut()
    clipboardIsReadOnlyAfterExactlyOneUnconcealedWrite()
    waitsForTextAfterAClearingWrite()
    rejectsDuplicateLinksAndChangesAfterACopy()
    print("Selected-task probe sequencing passed")
  }

  static func run(_ system: FakeProbeSystem) -> Result<(windowIDs: [UInt32], taskCount: Int), WindowTaskProbeFailure> {
    do { return .success(try WindowTaskProbe(system: system).run()) } catch {
      return .failure(error as! WindowTaskProbeFailure)
    }
  }

  static func failure(_ system: FakeProbeSystem) -> WindowTaskProbeFailure? {
    if case .failure(let failure) = run(system) { return failure }
    return nil
  }

  static func probesEveryWindowOnceAndReportsOnlyTheCount() {
    let system = FakeProbeSystem()
    system.copies = [.write(first), .write(second)]
    guard case .success(let result) = run(system) else { fatalError("valid probe failed") }
    precondition(result.windowIDs == [31, 32] && result.taskCount == 2)
    precondition(system.posts == 2)
    // Nothing visible happens until every precondition has passed.
    let visible = system.log.firstIndex(of: "visible")!
    precondition(Array(system.log[..<visible]) == ["opt-in", "preconditions", "birth", "ids", "map"])
    precondition(system.log[visible + 1] == "birth" && system.log[visible + 2] == "focus 0")
    // Each post follows a focus, a birth recheck, and a layout recheck.
    for (index, entry) in system.log.enumerated() where entry == "post" {
      precondition(system.log[index - 1] == "count" && system.log[index - 2] == "layout")
    }
    // Text is read only between a type check and a confirming count read.
    for (index, entry) in system.log.enumerated() where entry == "text" {
      precondition(system.log[index - 1] == "types" && system.log[index + 1] == "count")
    }
  }

  static func refusesBeforeAnyVisibleChange() {
    let optedOut = FakeProbeSystem()
    optedOut.optedIn = false
    precondition(failure(optedOut) == .explicitOptInRequired && optedOut.log == ["opt-in"])
    let unmet: [WindowTaskProbeFailure] = [
      .probeAccessDenied, .pasteboardAccessNotAllowed, .desktopVersionUnverified,
      .appShortcutConflict, .keyboardLayoutUnsupported,
    ]
    for reason in unmet {
      let system = FakeProbeSystem()
      system.unmet = reason
      precondition(failure(system) == reason && system.log == ["opt-in", "preconditions"])
    }
    let recycled = FakeProbeSystem()
    recycled.births = [false]
    precondition(failure(recycled) == .processIdentityRejected)
    let empty = FakeProbeSystem()
    empty.ids = []
    precondition(failure(empty) == .windowNotFound)
    let many = FakeProbeSystem()
    many.ids = (1...65).map { UInt32($0) }
    precondition(failure(many) == .windowLimitExceeded)
    var refused = [recycled, empty, many]
    for reason in [WindowTaskProbeFailure.windowMinimized, .windowMappingAmbiguous] {
      let system = FakeProbeSystem()
      system.mapping = reason
      precondition(failure(system) == reason)
      refused.append(system)
    }
    for system in refused {
      precondition(!system.log.contains("visible") && system.posts == 0)
      precondition(!system.log.contains { $0.hasPrefix("focus") })
    }
  }

  static func requiresObservedFocusBeforeEachShortcut() {
    let unfocused = FakeProbeSystem()
    unfocused.grantsFocus = false
    precondition(failure(unfocused) == .windowFocusFailed)
    precondition(unfocused.posts == 0 && unfocused.clock >= probeFocusTimeout)
    let wandered = FakeProbeSystem()
    wandered.focusChecksBeforeLoss = 1
    precondition(failure(wandered) == .windowFocusFailed && wandered.posts == 0)
    let recycled = FakeProbeSystem()
    recycled.births = [true, true, false]
    precondition(failure(recycled) == .processIdentityRejected && recycled.posts == 0)
    let relaid = FakeProbeSystem()
    relaid.layoutChecks = [false]
    precondition(failure(relaid) == .keyboardLayoutUnsupported && relaid.posts == 0)
    let unposted = FakeProbeSystem()
    unposted.postSucceeds = false
    precondition(failure(unposted) == .copyLinkEventFailed)
  }

  static func clipboardIsReadOnlyAfterExactlyOneUnconcealedWrite() {
    let cases: [(FakeCopy, WindowTaskProbeFailure, Bool)] = [
      (.noWrite, .copyLinkMissing, false),
      (.doubleWrite(first), .copyLinkAmbiguous, false),
      (.concealed(first), .copyLinkAmbiguous, false),
      (.overwrittenDuringRead(first), .copyLinkAmbiguous, true),
      (.write("codex://threads/new"), .copyLinkAmbiguous, true),
      (.write("https://example.com/"), .copyLinkAmbiguous, true),
    ]
    for (copy, expected, readsText) in cases {
      let system = FakeProbeSystem()
      system.copies = [copy]
      precondition(failure(system) == expected, "\(copy)")
      precondition(system.log.contains("text") == readsText, "\(copy) text read")
      precondition(system.posts == 1)
    }
    let silent = FakeProbeSystem()
    silent.copies = [.noWrite]
    _ = failure(silent)
    precondition(silent.clock >= probeClipboardTimeout)
  }

  static func waitsForTextAfterAClearingWrite() {
    let system = FakeProbeSystem()
    system.copies = [.delayedText(first, polls: 3), .write(second)]
    guard case .success(let result) = run(system) else { fatalError("delayed text was rejected") }
    precondition(result.taskCount == 2)
    let late = FakeProbeSystem()
    late.copies = [.delayedText(first, polls: 1_000)]
    precondition(failure(late) == .copyLinkAmbiguous && !late.log.contains("text"))
  }

  static func rejectsDuplicateLinksAndChangesAfterACopy() {
    let duplicate = FakeProbeSystem()
    duplicate.copies = [.write(first), .write(first)]
    precondition(failure(duplicate) == .taskLinkDuplicate)
    let changed = FakeProbeSystem()
    changed.copies = [.write(first), .write(second)]
    changed.idsAfterFirstPost = [31, 33]
    precondition(failure(changed) == .windowMappingChanged && changed.posts == 1)
    let unfocused = FakeProbeSystem()
    unfocused.copies = [.write(first), .write(second)]
    unfocused.losesFocusOnCopy = true
    precondition(failure(unfocused) == .windowMappingChanged && unfocused.posts == 1)
    let recycled = FakeProbeSystem()
    recycled.copies = [.write(first), .write(second)]
    // Start, first focus, pre-post recheck, then the post-copy recheck.
    recycled.births = [true, true, true, false]
    precondition(failure(recycled) == .windowMappingChanged && recycled.posts == 1)
  }
}
