import AppKit
import Foundation

/// Reusable interactive button engineered for drop-down menus with smooth hover rings,
/// capsule pill styling, pointing hand cursor tracking, and SF Symbol vector graphics.
public final class MenuIconButton: NSButton {
  public var hoverBackgroundColor: NSColor = NSColor.quaternaryLabelColor.withAlphaComponent(0.5)
  public var pressedBackgroundColor: NSColor = NSColor.tertiaryLabelColor.withAlphaComponent(0.7)
  public var hoverTintColor: NSColor? = nil
  public var normalTintColor: NSColor? = nil
  public let isCapsule: Bool
  public var managesOwnTracking: Bool = true

  private var trackingArea: NSTrackingArea?
  public private(set) var isHovered: Bool = false
  public private(set) var isMouseDown: Bool = false

  private var customTitle: String?
  public private(set) var iconImageView: NSImageView?
  public private(set) var titleLabel: NSTextField?

  public override var title: String {
    get { customTitle ?? super.title }
    set {
      if customTitle != nil {
        customTitle = newValue
        titleLabel?.stringValue = newValue
        needsLayout = true
      } else {
        super.title = newValue
      }
    }
  }

  public init(
    frame: NSRect,
    title: String? = nil,
    symbolName: String,
    pointSize: CGFloat = 11,
    weight: NSFont.Weight = .semibold,
    tintColor: NSColor = .labelColor,
    hoverTintColor: NSColor? = nil,
    backgroundColor: NSColor? = nil,
    hoverBackgroundColor: NSColor? = nil,
    isCapsule: Bool = false,
    tooltip: String? = nil,
    accessibilityLabel: String? = nil
  ) {
    self.normalTintColor = tintColor
    self.hoverTintColor = hoverTintColor
    self.isCapsule = isCapsule
    if let bg = hoverBackgroundColor { self.hoverBackgroundColor = bg }
    if let bg = backgroundColor { self.pressedBackgroundColor = bg }
    super.init(frame: frame)

    self.isBordered = false
    self.contentTintColor = tintColor
    self.toolTip = tooltip

    let symbolConfig = NSImage.SymbolConfiguration(pointSize: pointSize, weight: weight)
    let symbolImage = NSImage(systemSymbolName: symbolName, accessibilityDescription: accessibilityLabel)?
      .withSymbolConfiguration(symbolConfig)

    if let text = title, !text.isEmpty {
      self.customTitle = text
      self.font = NSFont.systemFont(ofSize: 11, weight: .semibold)
      self.imagePosition = .noImage

      let iv = NSImageView()
      iv.image = symbolImage
      iv.imageScaling = .scaleProportionallyDown
      iv.contentTintColor = tintColor
      addSubview(iv)
      self.iconImageView = iv

      let tf = NSTextField(labelWithString: text)
      tf.font = NSFont.systemFont(ofSize: 11, weight: .semibold)
      tf.textColor = tintColor
      tf.alignment = .left
      tf.isBezeled = false
      tf.drawsBackground = false
      tf.isEditable = false
      tf.isSelectable = false
      addSubview(tf)
      self.titleLabel = tf
    } else {
      self.customTitle = nil
      self.image = symbolImage
      self.imagePosition = .imageOnly
    }

    if isCapsule {
      self.hoverBackgroundColor = hoverBackgroundColor ?? tintColor.withAlphaComponent(0.24)
      self.pressedBackgroundColor = tintColor.withAlphaComponent(0.38)
    }

    if let label = accessibilityLabel ?? tooltip ?? title {
      self.setAccessibilityElement(true)
      self.setAccessibilityRole(.button)
      self.setAccessibilityLabel(label)
    }

    self.wantsLayer = true
    layout()
  }

  public required init?(coder: NSCoder) {
    fatalError("init(coder:) has not been implemented")
  }

  private var isCursorPushed = false
  public func pushPointingCursor() {
    if !isCursorPushed { NSCursor.pointingHand.push(); isCursorPushed = true }
  }
  public func popPointingCursor() {
    if isCursorPushed { NSCursor.pop(); isCursorPushed = false }
  }

  public override func viewDidMoveToWindow() {
    super.viewDidMoveToWindow()
    if window == nil { popPointingCursor() }
  }

  public override func layout() {
    super.layout()
    guard let text = customTitle, !text.isEmpty,
          let iv = iconImageView, let tf = titleLabel else { return }
    let font = tf.font ?? NSFont.systemFont(ofSize: 11, weight: .semibold)
    let textSize = (text as NSString).size(withAttributes: [.font: font])
    let iconW: CGFloat = 13
    let iconH: CGFloat = 13
    let gap: CGFloat = 5
    let textW = ceil(textSize.width)
    let textH = ceil(textSize.height)
    let totalW = iconW + gap + textW
    let startX = max(4, floor((bounds.width - totalW) / 2))

    iv.frame = NSRect(
      x: startX,
      y: floor((bounds.height - iconH) / 2),
      width: iconW,
      height: iconH
    )
    tf.frame = NSRect(
      x: startX + iconW + gap,
      y: floor((bounds.height - textH) / 2),
      width: textW + 2,
      height: textH
    )
  }

  public override func hitTest(_ point: NSPoint) -> NSView? {
    return bounds.contains(point) ? self : nil
  }

  public func setHoveredExplicitly(_ hovered: Bool) {
    guard isHovered != hovered else { return }
    isHovered = hovered
    let activeTint: NSColor? = hovered ? (hoverTintColor ?? normalTintColor) : normalTintColor
    if let tint = activeTint {
      contentTintColor = tint
      iconImageView?.contentTintColor = tint
      titleLabel?.textColor = tint
    }
    needsDisplay = true
  }

  public override func updateTrackingAreas() {
    super.updateTrackingAreas()
    if let oldArea = trackingArea { removeTrackingArea(oldArea) }
    guard managesOwnTracking else { return }
    let area = NSTrackingArea(
      rect: bounds,
      options: [.mouseEnteredAndExited, .cursorUpdate, .activeInActiveApp, .inVisibleRect],
      owner: self, userInfo: nil
    )
    addTrackingArea(area); self.trackingArea = area
  }

  public override func mouseEntered(with event: NSEvent) {
    super.mouseEntered(with: event)
    setHoveredExplicitly(true); pushPointingCursor()
  }

  public override func cursorUpdate(with event: NSEvent) {
    NSCursor.pointingHand.set()
  }

  public override func mouseExited(with event: NSEvent) {
    super.mouseExited(with: event)
    setHoveredExplicitly(false); isMouseDown = false; popPointingCursor()
  }

  public override func mouseDown(with event: NSEvent) {
    isMouseDown = true; needsDisplay = true
    super.mouseDown(with: event)
    isMouseDown = false; needsDisplay = true
  }

  public override func resetCursorRects() {
    super.resetCursorRects()
    addCursorRect(bounds, cursor: .pointingHand)
  }

  public override func draw(_ dirtyRect: NSRect) {
    let radius: CGFloat = isCapsule ? 5 : 4
    let pill = NSBezierPath(roundedRect: bounds.insetBy(dx: 0.5, dy: 0.5), xRadius: radius, yRadius: radius)

    if isCapsule {
      if isMouseDown {
        pressedBackgroundColor.setFill(); pill.fill()
      } else if isHovered {
        hoverBackgroundColor.setFill(); pill.fill()
        (normalTintColor ?? .controlAccentColor).withAlphaComponent(0.65).setStroke()
        pill.lineWidth = 1; pill.stroke()
      } else {
        (normalTintColor ?? .controlAccentColor).withAlphaComponent(0.08).setFill(); pill.fill()
        NSColor.separatorColor.withAlphaComponent(0.65).setStroke()
        pill.lineWidth = 0.8; pill.stroke()
      }
    } else {
      if isMouseDown {
        pressedBackgroundColor.setFill(); pill.fill()
      } else if isHovered {
        hoverBackgroundColor.setFill(); pill.fill()
      }
    }
    if customTitle == nil {
      super.draw(dirtyRect)
    }
  }
}
