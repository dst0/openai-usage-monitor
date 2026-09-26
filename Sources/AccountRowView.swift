import AppKit
import Foundation

/// Interactive identity row for accounts in drop-down menus, supporting hover highlights,
/// keyboard selection tracking, quick action buttons, pointing hand cursor, and context menu.
public final class AccountRowView: NSView {
  public static let standardInset: CGFloat = 28

  public static func truncateMiddle(_ text: String, font: NSFont, maxWidth: CGFloat) -> String {
    let currentWidth = (text as NSString).size(withAttributes: [.font: font]).width
    if currentWidth <= maxWidth || text.count <= 6 { return text }
    let ellipsis = "…"
    var low = 1, high = (text.count - 1) / 2
    var best = "\(text.prefix(1))\(ellipsis)\(text.suffix(1))"
    while low <= high {
      let mid = (low + high) / 2
      let candidate = "\(text.prefix(mid))\(ellipsis)\(text.suffix(mid))"
      if (candidate as NSString).size(withAttributes: [.font: font]).width <= maxWidth {
        best = candidate; low = mid + 1
      } else {
        high = mid - 1
      }
    }
    return best
  }

  public let accountId: String
  public let accountName: String?
  public let accountEmail: String
  public let tier: String?
  public let isCurrentActive: Bool
  public let isAppSession: Bool
  public let needsRelogin: Bool
  public let onDelete: (String, String) -> Void
  public let onRename: (String, String?, String) -> Void
  public let onSelect: (String) -> Void
  public let onRelogin: (String, String) -> Void

  public let titleLabel: NSTextField
  public var switchButton: NSButton?
  public var deleteButton: NSButton?
  public var managesOwnTracking: Bool = true
  public private(set) var isExternalHighlighted: Bool = false

  public func setExternalHighlight(_ highlighted: Bool) {
    guard isExternalHighlighted != highlighted else { return }
    isExternalHighlighted = highlighted
    needsDisplay = true
  }

  private var trackingArea: NSTrackingArea?
  public private(set) var isHovered: Bool = false

  public init(
    frame: NSRect, accountId: String, accountName: String?, email: String, tier: String?,
    isCurrentActive: Bool, isAppSession: Bool = false, needsRelogin: Bool = false,
    dotColor: NSColor, statusTag: String, statusTagColor: NSColor? = nil,
    showsInlineSwitchButton: Bool = true,
    onDelete: @escaping (String, String) -> Void = { _, _ in },
    onRename: @escaping (String, String?, String) -> Void = { _, _, _ in },
    onSelect: @escaping (String) -> Void = { _ in },
    onRelogin: @escaping (String, String) -> Void = { _, _ in }
  ) {
    self.accountId = accountId
    self.accountName = accountName
    self.accountEmail = email
    self.tier = tier
    self.isCurrentActive = isCurrentActive
    self.isAppSession = isAppSession
    self.needsRelogin = needsRelogin
    self.onDelete = onDelete
    self.onRename = onRename
    self.onSelect = onSelect
    self.onRelogin = onRelogin

    let rightOffset: CGFloat = isAppSession ? 12 : (isCurrentActive || !showsInlineSwitchButton || needsRelogin ? 28 : 56)
    let labelWidth = max(50, frame.width - rightOffset - Self.standardInset)
    self.titleLabel = NSTextField(frame: NSRect(x: Self.standardInset, y: 1, width: labelWidth, height: 20))

    if !isAppSession && !isCurrentActive && showsInlineSwitchButton && !needsRelogin {
      let sb = MenuIconButton(
        frame: NSRect(x: frame.width - 50, y: 2, width: 22, height: 18),
        symbolName: "arrow.triangle.2.circlepath", pointSize: 11, weight: .semibold,
        tintColor: .systemBlue, hoverTintColor: .controlAccentColor,
        tooltip: L10n.switchToAccount, accessibilityLabel: "\(L10n.switchToAccount): \(email)"
      )
      if sb.image == nil { sb.title = "⇄"; sb.font = NSFont.systemFont(ofSize: 13, weight: .bold) }
      self.switchButton = sb
    } else {
      self.switchButton = nil
    }

    if !isAppSession {
      let db = MenuIconButton(
        frame: NSRect(x: frame.width - 24, y: 2, width: 20, height: 18),
        symbolName: "xmark", pointSize: 10, weight: .bold,
        tintColor: .secondaryLabelColor, hoverTintColor: .systemRed,
        tooltip: L10n.removeAccount, accessibilityLabel: "\(L10n.removeAccount): \(email)"
      )
      if db.image == nil { db.title = "✕"; db.font = NSFont.systemFont(ofSize: 12, weight: .bold) }
      self.deleteButton = db
    } else {
      self.deleteButton = nil
    }

    super.init(frame: frame)
    self.autoresizingMask = [.width]

    titleLabel.isBezeled = false; titleLabel.drawsBackground = false
    titleLabel.isEditable = false; titleLabel.isSelectable = false; titleLabel.lineBreakMode = .byClipping

    let dotFont = NSFont.systemFont(ofSize: 13, weight: .bold)
    let titleFont = isCurrentActive ? NSFont.boldSystemFont(ofSize: 13) : NSFont.systemFont(ofSize: 13)
    let tagFont = NSFont.systemFont(ofSize: 11, weight: .semibold)

    let displayTitle: String
    let cleanName = accountName?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
    let emailUsername = email.components(separatedBy: "@").first?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
    if !cleanName.isEmpty && cleanName.caseInsensitiveCompare(email) != .orderedSame && cleanName.caseInsensitiveCompare(emailUsername) != .orderedSame {
      displayTitle = "\(cleanName) (\(email))"
    } else {
      displayTitle = email
    }

    let planLabel = tier ?? "Team"
    let tagString = "  \(statusTag)  \(planLabel)"
    let dotWidth = ("● " as NSString).size(withAttributes: [.font: dotFont]).width
    let tagWidth = (tagString as NSString).size(withAttributes: [.font: tagFont]).width
    let maxTitleWidth = max(20, labelWidth - dotWidth - tagWidth - 4.0)
    let resolvedDisplayTitle = Self.truncateMiddle(displayTitle, font: titleFont, maxWidth: maxTitleWidth)

    let rich = NSMutableAttributedString()
    rich.append(NSAttributedString(string: "● ", attributes: [.font: dotFont, .foregroundColor: dotColor]))
    rich.append(NSAttributedString(string: resolvedDisplayTitle, attributes: [.font: titleFont, .foregroundColor: NSColor.labelColor]))
    let resolvedTagColor = statusTagColor ?? (isCurrentActive ? (isAppSession ? NSColor.systemTeal : NSColor.systemGreen) : NSColor.secondaryLabelColor)
    rich.append(NSAttributedString(string: tagString, attributes: [.font: tagFont, .foregroundColor: resolvedTagColor]))
    titleLabel.attributedStringValue = rich
    addSubview(titleLabel)

    if let sb = switchButton { sb.target = self; sb.action = #selector(handleSwitchClick); sb.autoresizingMask = [.minXMargin]; addSubview(sb) }
    if let db = deleteButton { db.target = self; db.action = #selector(handleDelete); db.autoresizingMask = [.minXMargin]; addSubview(db) }

    setAccessibilityElement(true)
    setAccessibilityRole(.menuItem)
    setAccessibilityLabel("\(resolvedDisplayTitle), \(planLabel), \(statusTag)")
  }

  required init?(coder: NSCoder) { fatalError("init(coder:) has not been implemented") }

  public override func updateTrackingAreas() {
    super.updateTrackingAreas()
    if let old = trackingArea { removeTrackingArea(old) }
    guard managesOwnTracking else { return }
    let area = NSTrackingArea(
      rect: bounds,
      options: [.mouseEnteredAndExited, .cursorUpdate, .activeAlways, .inVisibleRect],
      owner: self,
      userInfo: nil
    )
    addTrackingArea(area)
    self.trackingArea = area
  }

  public override func mouseEntered(with event: NSEvent) {
    super.mouseEntered(with: event)
    isHovered = true
    NSCursor.pointingHand.set()
    needsDisplay = true
  }

  public override func cursorUpdate(with event: NSEvent) {
    NSCursor.pointingHand.set()
  }

  public override func mouseExited(with event: NSEvent) {
    super.mouseExited(with: event)
    isHovered = false
    NSCursor.arrow.set()
    needsDisplay = true
  }

  public override func draw(_ dirtyRect: NSRect) {
    let isSelected = (isExternalHighlighted || (enclosingMenuItem?.isHighlighted ?? false) || isHovered) && managesOwnTracking
    if isSelected {
      let highlightRect = bounds.insetBy(dx: 4, dy: 1)
      let path = NSBezierPath(roundedRect: highlightRect, xRadius: 6, yRadius: 6)
      if isCurrentActive && !needsRelogin && !isAppSession {
        NSColor.controlAccentColor.withAlphaComponent(0.12).setFill()
        path.fill()
        NSColor.controlAccentColor.withAlphaComponent(0.30).setStroke()
        path.lineWidth = 0.8
        path.stroke()
      } else {
        NSColor.selectedContentBackgroundColor.withAlphaComponent(0.24).setFill()
        path.fill()
        NSColor.controlAccentColor.withAlphaComponent(0.40).setStroke()
        path.lineWidth = 0.8
        path.stroke()
      }
    }
    super.draw(dirtyRect)
  }

  @objc private func handleDelete() { onDelete(accountId, accountEmail) }
  @objc private func handleSwitchClick() { enclosingMenuItem?.menu?.cancelTracking(); onSelect(accountId) }

  public override func resetCursorRects() {
    super.resetCursorRects()
    addCursorRect(bounds, cursor: .pointingHand)
  }

  public override func mouseUp(with event: NSEvent) {
    let point = convert(event.locationInWindow, from: nil)
    if let db = deleteButton, db.frame.contains(point) { handleDelete(); return }
    if let sb = switchButton, sb.frame.contains(point) { handleSwitchClick(); return }
    if isAppSession {
      enclosingMenuItem?.menu?.cancelTracking()
      if let app = NSRunningApplication.runningApplications(withBundleIdentifier: "com.openai.codex").first
        ?? NSRunningApplication.runningApplications(withBundleIdentifier: "com.openai.chat").first {
        if #available(macOS 14.0, *) { app.activate() } else { app.activate(options: [.activateIgnoringOtherApps]) }
      } else {
        NSWorkspace.shared.open(URL(fileURLWithPath: "/Applications/ChatGPT.app"))
      }
      return
    }
    if needsRelogin { enclosingMenuItem?.menu?.cancelTracking(); onRelogin(accountId, accountEmail); return }
    if !isCurrentActive { enclosingMenuItem?.menu?.cancelTracking(); onSelect(accountId) }
  }

  public override func accessibilityPerformPress() -> Bool {
    if needsRelogin { enclosingMenuItem?.menu?.cancelTracking(); onRelogin(accountId, accountEmail); return true }
    if !isCurrentActive { enclosingMenuItem?.menu?.cancelTracking(); onSelect(accountId); return true }
    return false
  }

  public override func accessibilityCustomActions() -> [NSAccessibilityCustomAction]? {
    var actions: [NSAccessibilityCustomAction] = []
    if !isCurrentActive && !needsRelogin {
      actions.append(NSAccessibilityCustomAction(name: L10n.switchToAccount, target: self, selector: #selector(handleSwitchFromCtx)))
    }
    if needsRelogin {
      actions.append(NSAccessibilityCustomAction(name: L10n.reloginToAccount, target: self, selector: #selector(handleReloginFromCtx)))
    }
    actions.append(NSAccessibilityCustomAction(name: L10n.renameAccount, target: self, selector: #selector(handleRenameFromCtx)))
    actions.append(NSAccessibilityCustomAction(name: L10n.removeAccount, target: self, selector: #selector(handleDeleteFromCtx)))
    return actions
  }

  public override func menu(for event: NSEvent) -> NSMenu? {
    if isAppSession { return nil }
    let ctx = NSMenu(); ctx.autoenablesItems = false
    if !isCurrentActive && !needsRelogin {
      let it = NSMenuItem(title: L10n.switchToAccount, action: #selector(handleSwitchFromCtx), keyEquivalent: "")
      it.target = self; ctx.addItem(it); ctx.addItem(NSMenuItem.separator())
    }
    let rl = NSMenuItem(title: L10n.reloginAccount, action: #selector(handleReloginFromCtx), keyEquivalent: "")
    rl.target = self; ctx.addItem(rl)
    let rn = NSMenuItem(title: L10n.renameAccount, action: #selector(handleRenameFromCtx), keyEquivalent: "")
    rn.target = self; ctx.addItem(rn)
    let rm = NSMenuItem(title: L10n.removeAccount, action: #selector(handleDeleteFromCtx), keyEquivalent: "")
    rm.target = self; ctx.addItem(rm)
    return ctx
  }

  @objc private func handleSwitchFromCtx() { onSelect(accountId) }
  @objc private func handleReloginFromCtx() { onRelogin(accountId, accountEmail) }
  @objc private func handleRenameFromCtx() { onRename(accountId, accountName, accountEmail) }
  @objc private func handleDeleteFromCtx() { onDelete(accountId, accountEmail) }
}
