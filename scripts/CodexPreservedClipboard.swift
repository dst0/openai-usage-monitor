import AppKit

/// A larger clipboard is not held in memory; it is left as ChatGPT's link.
let maximumPreservedClipboardBytes = 32 * 1024 * 1024

/// The user's clipboard items, held only in this process's memory while a
/// window-task command lets ChatGPT copy task links.
final class PreservedClipboard {
  private let limit: Int
  private var items: [[(NSPasteboard.PasteboardType, Data)]]?

  init(limit: Int = maximumPreservedClipboardBytes) { self.limit = limit }

  /// Private (concealed or transient) contents are never read, so they are
  /// not preserved; neither is a clipboard larger than the limit.
  func preserve(_ pasteboard: NSPasteboard) {
    items = nil
    guard !pasteboardTypesArePrivate((pasteboard.types ?? []).map { $0.rawValue }) else { return }
    var saved: [[(NSPasteboard.PasteboardType, Data)]] = []
    var total = 0
    for item in pasteboard.pasteboardItems ?? [] {
      var entries: [(NSPasteboard.PasteboardType, Data)] = []
      for type in item.types {
        guard let data = item.data(forType: type) else { continue }
        total += data.count
        guard total <= limit else { return }
        entries.append((type, data))
      }
      saved.append(entries)
    }
    items = saved
  }

  /// Nothing else may have written since the caller's own last copy. The
  /// check and the write are not atomic; a write in between is overwritten.
  func restore(_ pasteboard: NSPasteboard, expectedChangeCount: Int) -> Bool {
    guard let items, pasteboard.changeCount == expectedChangeCount else { return false }
    self.items = nil
    pasteboard.clearContents()
    let restored = items.map { entries -> NSPasteboardItem in
      let item = NSPasteboardItem()
      for (type, data) in entries { item.setData(data, forType: type) }
      return item
    }
    return restored.isEmpty || pasteboard.writeObjects(restored)
  }
}
