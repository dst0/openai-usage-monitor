import AppKit

/// Uses a uniquely named private pasteboard; the user's clipboard is never
/// read or written.
@main
struct CodexPreservedClipboardTests {
  static let custom = NSPasteboard.PasteboardType("com.example.codex-monitor-test")

  static func main() {
    restoresEveryItemAndTypeAfterOwnWrite()
    keepsANewerWriteByAnotherApp()
    neverReadsOrRestoresPrivateContents()
    skipsAClipboardOverTheLimit()
    restoresAnEmptyClipboardOnlyOnce()
    aPrivateMarkerOnAnyItemBlocksPreservation()
    aLaterPreserveReplacesTheEarlierItems()
    print("Preserved clipboard passed")
  }

  static func pasteboard() -> NSPasteboard {
    NSPasteboard(name: NSPasteboard.Name("codex-monitor-test-\(UUID().uuidString)"))
  }

  static func write(_ pasteboard: NSPasteboard, _ items: [[NSPasteboard.PasteboardType: Data]]) {
    pasteboard.clearContents()
    let objects = items.map { entries -> NSPasteboardItem in
      let item = NSPasteboardItem()
      for (type, data) in entries { item.setData(data, forType: type) }
      return item
    }
    precondition(objects.isEmpty || pasteboard.writeObjects(objects))
  }

  static func link(_ pasteboard: NSPasteboard) -> Int {
    write(pasteboard, [[.string: Data("codex://threads/01a00000-0000-4000-8000-00000000000a".utf8)]])
    return pasteboard.changeCount
  }

  static func restoresEveryItemAndTypeAfterOwnWrite() {
    let board = pasteboard()
    defer { board.releaseGlobally() }
    write(board, [[.string: Data("first".utf8), custom: Data([1, 2, 3])], [.string: Data("second".utf8)]])
    let clipboard = PreservedClipboard()
    clipboard.preserve(board)
    let own = link(board)
    precondition(clipboard.restore(board, expectedChangeCount: own))
    let items = board.pasteboardItems ?? []
    precondition(items.count == 2)
    precondition(items[0].string(forType: .string) == "first")
    precondition(items[0].data(forType: custom) == Data([1, 2, 3]))
    precondition(items[1].string(forType: .string) == "second")
  }

  static func keepsANewerWriteByAnotherApp() {
    let board = pasteboard()
    defer { board.releaseGlobally() }
    write(board, [[.string: Data("user".utf8)]])
    let clipboard = PreservedClipboard()
    clipboard.preserve(board)
    let own = link(board)
    write(board, [[.string: Data("another app".utf8)]])
    precondition(!clipboard.restore(board, expectedChangeCount: own))
    precondition(board.string(forType: .string) == "another app")
  }

  static func neverReadsOrRestoresPrivateContents() {
    for marker in ["org.nspasteboard.ConcealedType", "org.nspasteboard.TransientType"] {
      let board = pasteboard()
      defer { board.releaseGlobally() }
      write(board, [[.string: Data("secret".utf8), NSPasteboard.PasteboardType(marker): Data()]])
      let clipboard = PreservedClipboard()
      clipboard.preserve(board)
      let own = link(board)
      precondition(!clipboard.restore(board, expectedChangeCount: own))
      precondition(board.string(forType: .string)?.hasPrefix("codex://threads/") == true)
    }
  }

  static func skipsAClipboardOverTheLimit() {
    let board = pasteboard()
    defer { board.releaseGlobally() }
    write(board, [[.string: Data("sixteen bytes!!!".utf8)], [.string: Data("x".utf8)]])
    let clipboard = PreservedClipboard(limit: 16)
    clipboard.preserve(board)
    let own = link(board)
    precondition(!clipboard.restore(board, expectedChangeCount: own))
    let exact = PreservedClipboard(limit: 17)
    write(board, [[.string: Data("sixteen bytes!!!".utf8)], [.string: Data("x".utf8)]])
    exact.preserve(board)
    precondition(exact.restore(board, expectedChangeCount: link(board)))
    precondition(board.pasteboardItems?.count == 2)
  }

  static func restoresAnEmptyClipboardOnlyOnce() {
    let board = pasteboard()
    defer { board.releaseGlobally() }
    board.clearContents()
    let clipboard = PreservedClipboard()
    clipboard.preserve(board)
    let own = link(board)
    precondition(clipboard.restore(board, expectedChangeCount: own))
    precondition((board.pasteboardItems ?? []).isEmpty)
    let again = link(board)
    precondition(!clipboard.restore(board, expectedChangeCount: again))
    precondition(board.string(forType: .string)?.hasPrefix("codex://threads/") == true)
  }

  static func aPrivateMarkerOnAnyItemBlocksPreservation() {
    for marker in ["de.petermaurer.TransientPasteboardType", "com.agilebits.onepassword"] {
      let board = pasteboard()
      defer { board.releaseGlobally() }
      write(board, [[.string: Data("plain".utf8)], [NSPasteboard.PasteboardType(marker): Data([1])]])
      let clipboard = PreservedClipboard()
      clipboard.preserve(board)
      precondition(!clipboard.restore(board, expectedChangeCount: link(board)), marker)
    }
  }

  static func aLaterPreserveReplacesTheEarlierItems() {
    let board = pasteboard()
    defer { board.releaseGlobally() }
    write(board, [[.string: Data("first".utf8)]])
    let clipboard = PreservedClipboard()
    clipboard.preserve(board)
    write(board, [[.string: Data("secret".utf8), NSPasteboard.PasteboardType("org.nspasteboard.ConcealedType"): Data()]])
    clipboard.preserve(board)
    // The newer contents were private, so nothing, not "first", comes back.
    precondition(!clipboard.restore(board, expectedChangeCount: link(board)))
    write(board, [[.string: Data("second".utf8)]])
    clipboard.preserve(board)
    precondition(clipboard.restore(board, expectedChangeCount: link(board)))
    precondition(board.string(forType: .string) == "second")
  }
}
