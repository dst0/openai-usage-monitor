import AppKit
import Foundation

/// Defines how a column is bracketed in the status bar based on session selection.
public enum BracketSelectionMode: Equatable, Sendable {
  case none
  case app
  case cli
  case both
}

extension MenuBarAppearanceHelper {
  /// Color matching the "APP " session tag (vibrant electric royal blue, matches AGI CLI).
  public static func ideTagColor(isScreenActive: Bool = true) -> NSColor {
    return isScreenActive
      ? NSColor(red: 0.12, green: 0.65, blue: 1.0, alpha: 1.0)
      : NSColor(red: 0.10, green: 0.58, blue: 0.90, alpha: 1.0)
  }

  public static func appTagColor(isScreenActive: Bool = true) -> NSColor {
    return ideTagColor(isScreenActive: isScreenActive)
  }

  /// Color matching the "CLI " session tag (crisp spring green, matches AGI CLI).
  public static func cliTagColor(isScreenActive: Bool = true) -> NSColor {
    return isScreenActive
      ? NSColor(red: 0.35, green: 0.92, blue: 0.45, alpha: 1.0)
      : NSColor(red: 0.30, green: 0.78, blue: 0.38, alpha: 1.0)
  }

  /// Creates a dual-colored split pattern NSColor (matches AGI CLI).
  /// The top half is rendered in `topColor` (APP), and the bottom half in `bottomColor` (CLI).
  public static func makeSplitBracketColor(
    topColor: NSColor,
    bottomColor: NSColor,
    height: CGFloat = 22.0
  ) -> NSColor {
    let patternW: CGFloat = 16.0
    let midY = height / 2.0
    let patternImg = NSImage(size: NSSize(width: patternW, height: height), flipped: false) { _ in
      bottomColor.setFill()
      NSRect(x: 0, y: 0, width: patternW, height: midY).fill()
      topColor.setFill()
      NSRect(x: 0, y: midY, width: patternW, height: height - midY).fill()
      return true
    }
    return NSColor(patternImage: patternImg)
  }

  /// Returns the bracket color for a given selection mode (matches AGI CLI).
  public static func bracketColor(
    for type: BracketSelectionMode,
    isScreenActive: Bool = true
  ) -> NSColor {
    switch type {
    case .app:
      return appTagColor(isScreenActive: isScreenActive)
    case .cli:
      return cliTagColor(isScreenActive: isScreenActive)
    case .both:
      return makeSplitBracketColor(
        topColor: appTagColor(isScreenActive: isScreenActive),
        bottomColor: cliTagColor(isScreenActive: isScreenActive)
      )
    case .none:
      return .white
    }
  }

  /// Appends an opening or closing bracket to the attributed string with the appropriate color/font.
  public static func appendBracket(
    to attributed: NSMutableAttributedString,
    bracket: String,
    mode: BracketSelectionMode,
    isScreenActive: Bool = true
  ) {
    guard mode != .none else { return }

    let font = (bracket == "]")
      ? rightBracketFont(isScreenActive: isScreenActive)
      : bracketFont(isScreenActive: isScreenActive)
    let shadow = bracketShadow(isScreenActive: isScreenActive)
    let offset = bracketBaselineOffset(isScreenActive: isScreenActive)
    let color = bracketColor(for: mode, isScreenActive: isScreenActive)

    attributed.append(
      NSAttributedString(
        string: bracket,
        attributes: [
          .font: font,
          .foregroundColor: color,
          .shadow: shadow,
          .baselineOffset: offset,
        ]))
  }
}
