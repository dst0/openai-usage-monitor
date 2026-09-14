import AppKit
import Foundation

/// Inset separator line designed for clean section delineation within drop-down menus.
public final class InsetSeparatorView: NSView {
  public let horizontalInset: CGFloat
  public let lineColor: NSColor

  public init(
    frame frameRect: NSRect,
    horizontalInset: CGFloat = 16,
    lineColor: NSColor = NSColor.separatorColor
  ) {
    self.horizontalInset = horizontalInset
    self.lineColor = lineColor
    super.init(frame: frameRect)
    self.wantsLayer = true
  }

  public required init?(coder: NSCoder) {
    fatalError("init(coder:) has not been implemented")
  }

  public override func draw(_ dirtyRect: NSRect) {
    super.draw(dirtyRect)
    let rect = NSRect(
      x: horizontalInset,
      y: bounds.midY - 0.5,
      width: max(0, bounds.width - (horizontalInset * 2)),
      height: 1.0
    )
    lineColor.setFill()
    rect.fill()
  }
}
