import AppKit

/// An `NSTextField` for modal dialogs and accessory views that ensures standard
/// keyboard shortcuts (Cmd+V, Cmd+C, Cmd+X, Cmd+A, Cmd+Z) work reliably even when running
/// in an accessory application (`.accessory`) without a standard Dock window.
public class ModalInputField: NSTextField {
  public override func performKeyEquivalent(with event: NSEvent) -> Bool {
    if event.type == .keyDown && event.modifierFlags.contains(.command) {
      switch event.charactersIgnoringModifiers {
      case "v":
        if NSApp.sendAction(#selector(NSText.paste(_:)), to: nil, from: self) {
          return true
        }
      case "c":
        if NSApp.sendAction(#selector(NSText.copy(_:)), to: nil, from: self) {
          return true
        }
      case "x":
        if NSApp.sendAction(#selector(NSText.cut(_:)), to: nil, from: self) {
          return true
        }
      case "a":
        if NSApp.sendAction(#selector(NSText.selectAll(_:)), to: nil, from: self) {
          return true
        }
      case "z":
        let action =
          event.modifierFlags.contains(.shift) ? Selector(("redo:")) : Selector(("undo:"))
        if NSApp.sendAction(action, to: nil, from: self) {
          return true
        }
      default:
        break
      }
    }
    return super.performKeyEquivalent(with: event)
  }
}
