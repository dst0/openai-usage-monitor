import AppKit
import Foundation

/// Header view for organization-based or personal account groups inside card containers.
public final class AccountSectionHeaderView: NSView {
  public enum Kind {
    case business
    case personal

    fileprivate var symbolName: String {
      switch self {
      case .business: return "building.2.fill"
      case .personal: return "person.2.fill"
      }
    }
  }

  public let titleLabel: NSTextField
  public let countLabel: NSTextField
  public let kind: Kind

  private let iconView = NSImageView()

  public init(frame frameRect: NSRect, title: String, count: Int, kind: Kind) {
    self.kind = kind
    self.titleLabel = NSTextField(labelWithString: title)
    self.countLabel = NSTextField(labelWithString: "\(count)")
    super.init(frame: frameRect)

    let symbolConfig = NSImage.SymbolConfiguration(pointSize: 12, weight: .medium)
    iconView.image = NSImage(systemSymbolName: kind.symbolName, accessibilityDescription: title)?
      .withSymbolConfiguration(symbolConfig)
    iconView.imageScaling = .scaleProportionallyDown
    iconView.contentTintColor = .secondaryLabelColor
    addSubview(iconView)

    titleLabel.font = NSFont.systemFont(ofSize: 11.5, weight: .semibold)
    titleLabel.textColor = .labelColor
    titleLabel.lineBreakMode = .byTruncatingTail
    addSubview(titleLabel)

    countLabel.alignment = .center
    countLabel.font = NSFont.monospacedDigitSystemFont(ofSize: 10, weight: .semibold)
    countLabel.textColor = .secondaryLabelColor
    addSubview(countLabel)

    setAccessibilityElement(true)
    setAccessibilityRole(.group)
    setAccessibilityLabel("\(title), \(count)")
  }

  public required init?(coder: NSCoder) {
    fatalError("init(coder:) has not been implemented")
  }

  public override func layout() {
    super.layout()
    iconView.frame = NSRect(x: 14, y: 8, width: 16, height: 16)
    countLabel.frame = NSRect(x: bounds.width - 36, y: 6.5, width: 20, height: 16)
    titleLabel.frame = NSRect(
      x: 38,
      y: 7,
      width: max(0, countLabel.frame.minX - 46),
      height: 18
    )
  }

  public override func draw(_ dirtyRect: NSRect) {
    super.draw(dirtyRect)
    // Capsule pill background behind count badge, vertically centered with header text
    let pillRect = NSRect(x: bounds.width - 37, y: 8, width: 22, height: 15)
    let pillPath = NSBezierPath(roundedRect: pillRect, xRadius: 4.5, yRadius: 4.5)
    NSColor.quaternaryLabelColor.withAlphaComponent(0.40).setFill()
    pillPath.fill()

    // Bottom divider line
    NSColor.separatorColor.withAlphaComponent(0.55).setFill()
    NSRect(x: 12, y: 0, width: max(0, bounds.width - 24), height: 1).fill()
  }
}
