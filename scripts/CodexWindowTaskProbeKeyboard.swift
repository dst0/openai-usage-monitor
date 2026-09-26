import Carbon
import CoreGraphics

/// The character a key types with Command held in `layout`, which is what
/// macOS menus match a Command shortcut against. Returns nil when the layout
/// has no Unicode key data or the key does not produce exactly one character.
func commandCharacter(forKeyCode keyCode: CGKeyCode, layout: TISInputSource) -> String? {
  guard let raw = TISGetInputSourceProperty(layout, kTISPropertyUnicodeKeyLayoutData) else {
    return nil
  }
  let data = Unmanaged<CFData>.fromOpaque(raw).takeUnretainedValue() as Data
  return data.withUnsafeBytes { bytes -> String? in
    guard let keyboard = bytes.bindMemory(to: UCKeyboardLayout.self).baseAddress else { return nil }
    var deadKeyState: UInt32 = 0
    var length = 0
    var characters = [UniChar](repeating: 0, count: 4)
    let status = UCKeyTranslate(
      keyboard, UInt16(keyCode), UInt16(kUCKeyActionDown), UInt32((cmdKey >> 8) & 0xFF),
      UInt32(LMGetKbdType()), OptionBits(kUCKeyTranslateNoDeadKeysBit),
      &deadKeyState, characters.count, &length, &characters)
    guard status == noErr, length == 1 else { return nil }
    return String(utf16CodeUnits: characters, count: length)
  }
}

/// The current keyboard layout, the one a posted key event is translated with.
func currentKeyboardLayout() -> TISInputSource? {
  TISCopyCurrentKeyboardLayoutInputSource()?.takeRetainedValue()
}
