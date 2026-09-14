import AppKit
import Foundation

/// Visual effect header for primary menu sections (e.g. Codex Desktop App, Codex CLI, Reserve Accounts).
public final class PrimaryMenuSectionHeaderView: NSVisualEffectView {
  public let titleLabel: NSTextField
  private let iconView = NSImageView()

  public init(frame frameRect: NSRect, title: String, symbolName: String) {
    self.titleLabel = NSTextField(labelWithString: title)
    super.init(frame: frameRect)

    material = .headerView
    blendingMode = .withinWindow
    state = .followsWindowActiveState

    let symbolConfig = NSImage.SymbolConfiguration(pointSize: 12.5, weight: .semibold)
    iconView.image = NSImage(systemSymbolName: symbolName, accessibilityDescription: title)?
      .withSymbolConfiguration(symbolConfig)
    iconView.imageScaling = .scaleProportionallyDown
    iconView.contentTintColor = .secondaryLabelColor
    addSubview(iconView)

    titleLabel.font = NSFont.systemFont(ofSize: 12.5, weight: .bold)
    titleLabel.textColor = .secondaryLabelColor
    titleLabel.lineBreakMode = .byTruncatingTail
    addSubview(titleLabel)

    setAccessibilityElement(true)
    setAccessibilityRole(.group)
    setAccessibilityLabel(title)
  }

  public required init?(coder: NSCoder) {
    fatalError("init(coder:) has not been implemented")
  }

  public override func layout() {
    super.layout()
    iconView.frame = NSRect(x: 14, y: 7, width: 16, height: 16)
    titleLabel.frame = NSRect(x: 36, y: 6, width: max(0, bounds.width - 50), height: 18)
  }
}
