import AppKit
import Foundation

/// Interactive drop-down menu row displaying rate-limit reset credits with instant reset trigger.
public final class ResetCreditsRowView: NSView {
  public let label: NSTextField
  public let resetButton: NSButton
  private let onReset: () -> Void

  public init(
    frame frameRect: NSRect,
    credits: Int,
    accountDisplayName: String,
    leftPadding: CGFloat = 20,
    onReset: @escaping () -> Void
  ) {
    self.onReset = onReset
    self.label = NSTextField(labelWithAttributedString: NSAttributedString(
      string: "✨ \(L10n.resetCredits): \(credits)",
      attributes: [
        .font: NSFont.systemFont(ofSize: 11, weight: .medium),
        .foregroundColor: NSColor.systemIndigo,
      ]))

    let btn = MenuIconButton(
      frame: NSRect(x: frameRect.width - 44, y: 1, width: 22, height: 18),
      symbolName: "arrow.counterclockwise",
      pointSize: 11,
      weight: .semibold,
      tintColor: .systemIndigo,
      hoverTintColor: .systemIndigo,
      tooltip: "\(L10n.resetAccountTooltip): \(accountDisplayName)",
      accessibilityLabel: "\(L10n.resetAccountTooltip): \(accountDisplayName)"
    )
    if btn.image == nil {
      btn.title = "↺"
      btn.font = NSFont.systemFont(ofSize: 12, weight: .bold)
    }
    self.resetButton = btn
    super.init(frame: frameRect)

    autoresizingMask = [.width]

    label.frame = NSRect(x: leftPadding, y: 1, width: max(100, frameRect.width - leftPadding - 50), height: 18)
    label.lineBreakMode = .byClipping
    label.autoresizingMask = [.width]
    addSubview(label)

    resetButton.target = self
    resetButton.action = #selector(handleResetClick)
    resetButton.autoresizingMask = [.minXMargin]
    addSubview(resetButton)
  }

  public required init?(coder: NSCoder) {
    fatalError("init(coder:) has not been implemented")
  }

  @objc private func handleResetClick() {
    enclosingMenuItem?.menu?.cancelTracking()
    onReset()
  }

  public override func resetCursorRects() {
    super.resetCursorRects()
    addCursorRect(resetButton.frame, cursor: .pointingHand)
  }

  public override func mouseUp(with event: NSEvent) {
    let point = convert(event.locationInWindow, from: nil)
    if resetButton.frame.contains(point) {
      handleResetClick()
      return
    }
    super.mouseUp(with: event)
  }
}
