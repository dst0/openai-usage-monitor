import AppKit
import Foundation

/// Card container enclosing reserve account groups with symmetrical padding,
/// organization header, quota gauges, reset controls, and switch affordances.
public final class AccountSectionCardView: NSView {
  public let box = NSBox()
  public let contentContainer = NSView()
  public let headerView: AccountSectionHeaderView
  public private(set) var accountRows: [AccountRowView] = []
  public private(set) var metricLabels: [NSTextField] = []
  public private(set) var switchButtons: [NSButton] = []
  public private(set) var resetButtons: [NSButton] = []
  public private(set) var reloginButtons: [NSButton] = []
  public let entries: [ReserveAccountSectionEntry]

  private let onSwitch: (String) -> Void
  private let onReset: (String, String) -> Void
  private let onRelogin: (String, String) -> Void
  private let horizontalInset: CGFloat = 10

  public static func preferredHeight(for entries: [ReserveAccountSectionEntry]) -> CGFloat {
    var height: CGFloat = 41
    for (index, entry) in entries.enumerated() {
      if index > 0 { height += 9 }
      height += 73 // 26 (Identity) + 18 (Sprint) + 24 (Action) + 5 (Bottom)
      if entry.account.weeklyPercentage != nil { height += 18 }
      if entry.account.credits > 0 { height += 18 }
      if let error = entry.account.error, !error.isEmpty { height += 32 }
    }
    return height
  }

  public init(
    frame frameRect: NSRect,
    title: String,
    kind: AccountSectionHeaderView.Kind,
    entries: [ReserveAccountSectionEntry],
    onSwitch: @escaping (String) -> Void,
    onDelete: @escaping (String, String) -> Void,
    onRename: @escaping (String, String?, String) -> Void,
    onReset: @escaping (String, String) -> Void = { _, _ in },
    onRelogin: @escaping (String, String) -> Void = { _, _ in }
  ) {
    self.entries = entries
    self.headerView = AccountSectionHeaderView(
      frame: NSRect(x: 10, y: 0, width: max(0, frameRect.width - 20), height: 33),
      title: title, count: entries.count, kind: kind
    )
    self.onSwitch = onSwitch
    self.onReset = onReset
    self.onRelogin = onRelogin
    super.init(frame: frameRect)

    autoresizingMask = [.width]
    let contentWidth = max(0, frameRect.width - (horizontalInset * 2)), contentHeight = max(0, frameRect.height - 6)

    box.boxType = .custom; box.titlePosition = .noTitle; box.borderWidth = 1; box.cornerRadius = 9
    box.borderColor = NSColor.separatorColor.withAlphaComponent(0.65)
    box.fillColor = NSColor.controlBackgroundColor.withAlphaComponent(0.25); box.contentViewMargins = .zero
    box.frame = NSRect(x: horizontalInset, y: 3, width: contentWidth, height: contentHeight); box.autoresizingMask = [.width, .height]
    contentContainer.frame = NSRect(x: 0, y: 0, width: contentWidth, height: contentHeight)
    contentContainer.autoresizingMask = [.width, .height]; box.contentView = contentContainer; addSubview(box)

    headerView.frame = NSRect(x: 0, y: contentHeight - 33, width: contentWidth, height: 33)
    headerView.autoresizingMask = [.width, .minYMargin]; contentContainer.addSubview(headerView)

    var cursorY = contentHeight - 38
    for (entryIndex, entry) in entries.enumerated() {
      if entryIndex > 0 {
        let sep = NSBox(frame: NSRect(x: 12, y: cursorY - 5, width: max(0, contentWidth - 24), height: 1))
        sep.boxType = .separator; sep.autoresizingMask = [.width]; contentContainer.addSubview(sep); cursorY -= 9
      }

      let account = entry.account
      let statusTag = L10n.reserveSlot(index: entry.reserveIndex)
      let weeklyExhausted = MenuBarAppearanceHelper.isWeeklyExhausted(account.weeklyPercentage)
      let dotColor = MenuBarAppearanceHelper.dropdownColor(
        forPercentage: account.fiveHourPercentage, weeklyPercentage: account.weeklyPercentage, planMultiplier: account.planMultiplier
      )

      cursorY -= 26
      let accountRow = AccountRowView(
        frame: NSRect(x: 0, y: cursorY, width: contentWidth, height: 24),
        accountId: account.id, accountName: account.displayName, email: account.email,
        tier: account.planBadgeString, isCurrentActive: false, isAppSession: false,
        needsRelogin: account.needsRelogin, dotColor: dotColor, statusTag: statusTag,
        showsInlineSwitchButton: false, onDelete: onDelete, onRename: onRename, onSelect: onSwitch, onRelogin: onRelogin
      )
      accountRow.managesOwnTracking = false; contentContainer.addSubview(accountRow); accountRows.append(accountRow)

      let sprintPercentage = weeklyExhausted ? 0.0 : account.fiveHourPercentage
      let sprintString = weeklyExhausted ? "0%" : String(format: "%.0f%%", account.fiveHourPercentage)
      let sprintColor = MenuBarAppearanceHelper.dropdownColor(
        forPercentage: account.fiveHourPercentage, weeklyPercentage: account.weeklyPercentage, planMultiplier: account.planMultiplier
      )
      let sprintRich = MenuBarAppearanceHelper.makeColoredProgressBar(
        label: "⚡ 5h Sprint: \(sprintString) ", percentage: sprintPercentage,
        maxPercentage: 100.0 * account.planMultiplier, fillColor: sprintColor
      )
      let sprintReset = account.sprintTimeUntilResetString
      if !sprintReset.isEmpty && sprintReset != L10n.resetNow {
        sprintRich.append(NSAttributedString(string: " (\(sprintReset))", attributes: [
          .font: NSFont.systemFont(ofSize: 11), .foregroundColor: NSColor.secondaryLabelColor
        ]))
      }
      cursorY -= 18; addMetricLabel(sprintRich, at: cursorY)

      if let weeklyPercentage = account.weeklyPercentage {
        let weeklyColor = MenuBarAppearanceHelper.dropdownColor(forPercentage: weeklyPercentage, planMultiplier: account.planMultiplier)
        let weeklyRich = MenuBarAppearanceHelper.makeColoredProgressBar(
          label: String(format: "🗓️ Weekly: %.0f%% ", weeklyPercentage), percentage: weeklyPercentage,
          maxPercentage: 100.0 * account.planMultiplier, fillColor: weeklyColor
        )
        let weeklyReset = account.weeklyTimeUntilResetString
        if !weeklyReset.isEmpty && weeklyReset != L10n.resetNow {
          weeklyRich.append(NSAttributedString(string: " (\(weeklyReset))", attributes: [
            .font: NSFont.systemFont(ofSize: 11), .foregroundColor: NSColor.secondaryLabelColor
          ]))
        }
        cursorY -= 18; addMetricLabel(weeklyRich, at: cursorY)
      }

      if account.credits > 0 {
        cursorY -= 18
        let credits = NSAttributedString(string: "✨ \(L10n.resetCredits): \(account.credits)", attributes: [
          .font: NSFont.systemFont(ofSize: 11, weight: .medium), .foregroundColor: NSColor.systemIndigo
        ])
        addMetricLabel(credits, at: cursorY, marginRight: 36)

        let resetBtn = MenuIconButton(
          frame: NSRect(x: contentWidth - 28, y: cursorY - 1, width: 22, height: 18),
          symbolName: "arrow.counterclockwise", pointSize: 11, weight: .semibold,
          tintColor: .systemIndigo, hoverTintColor: .systemIndigo,
          tooltip: "\(L10n.resetAccountTooltip): \(account.displayName)",
          accessibilityLabel: "\(L10n.resetAccountTooltip): \(account.displayName)"
        )
        if resetBtn.image == nil { resetBtn.title = "↺"; resetBtn.font = NSFont.systemFont(ofSize: 12, weight: .bold) }
        resetBtn.identifier = NSUserInterfaceItemIdentifier(account.id)
        resetBtn.target = self; resetBtn.action = #selector(handleResetButton(_:))
        resetBtn.managesOwnTracking = false; resetBtn.autoresizingMask = [.minXMargin]
        contentContainer.addSubview(resetBtn); resetButtons.append(resetBtn)
      }

      if !account.needsRelogin {
        cursorY -= 24
        let switchButton = makeActionButton(
          y: cursorY, width: contentWidth, title: L10n.switchToAccount, symbol: "arrow.triangle.2.circlepath",
          color: .controlAccentColor, account: account, action: #selector(handleSwitchButton(_:))
        )
        contentContainer.addSubview(switchButton); switchButtons.append(switchButton)
      }

      if let error = account.error, !error.isEmpty {
        cursorY -= 32
        let errorLabel = NSTextField(wrappingLabelWithString: "⚠︎ \(error)")
        errorLabel.frame = NSRect(x: 29, y: cursorY, width: max(100, contentWidth - 58), height: 30)
        errorLabel.font = NSFont.systemFont(ofSize: 10.5, weight: .medium); errorLabel.textColor = .systemRed
        errorLabel.maximumNumberOfLines = 2; errorLabel.autoresizingMask = [.width]
        contentContainer.addSubview(errorLabel); metricLabels.append(errorLabel)
      }

      if account.needsRelogin {
        cursorY -= 24
        let reloginBtn = makeActionButton(
          y: cursorY, width: contentWidth, title: L10n.reloginToAccount, symbol: "arrow.clockwise.circle.fill",
          color: .systemOrange, account: account, action: #selector(handleReloginButton(_:))
        )
        contentContainer.addSubview(reloginBtn); reloginButtons.append(reloginBtn)
      }
      cursorY -= 5
    }
    setAccessibilityElement(false)
  }

  public required init?(coder: NSCoder) {
    fatalError("init(coder:) has not been implemented")
  }

  private func makeActionButton(
    y: CGFloat, width: CGFloat, title: String, symbol: String, color: NSColor,
    account: AccountQuota, action: Selector
  ) -> MenuIconButton {
    let textWidth = (title as NSString).size(withAttributes: [.font: NSFont.systemFont(ofSize: 11, weight: .semibold)]).width
    let btnWidth = min(width - 56, ceil(textWidth) + 42)
    let btn = MenuIconButton(
      frame: NSRect(x: 28, y: y, width: btnWidth, height: 22),
      title: title, symbolName: symbol, pointSize: 11, weight: .semibold,
      tintColor: color, hoverTintColor: color, isCapsule: true,
      tooltip: "\(title): \(account.email)", accessibilityLabel: "\(title): \(account.email)"
    )
    btn.identifier = NSUserInterfaceItemIdentifier(account.id)
    btn.target = self; btn.action = action; btn.managesOwnTracking = false
    return btn
  }

  private func addMetricLabel(_ value: NSAttributedString, at y: CGFloat, marginRight: CGFloat = 29) {
    let label = NSTextField(labelWithAttributedString: value)
    let width = max(50, contentContainer.bounds.width - 29 - marginRight)
    label.frame = NSRect(x: 29, y: y, width: width, height: 17)
    label.lineBreakMode = .byClipping; label.autoresizingMask = [.width]
    contentContainer.addSubview(label); metricLabels.append(label)
  }

  public override func resetCursorRects() {
    super.resetCursorRects()
    for btn in resetButtons + switchButtons + reloginButtons { addCursorRect(convert(btn.frame, from: contentContainer), cursor: .pointingHand) }
  }

  public override func mouseUp(with event: NSEvent) {
    let point = contentContainer.convert(event.locationInWindow, from: nil)
    for btn in resetButtons where btn.frame.contains(point) { handleResetButton(btn); return }
    for btn in switchButtons where btn.frame.contains(point) { handleSwitchButton(btn); return }
    for btn in reloginButtons where btn.frame.contains(point) { handleReloginButton(btn); return }
    for row in accountRows where row.frame.contains(point) {
      enclosingMenuItem?.menu?.cancelTracking()
      if row.needsRelogin { onRelogin(row.accountId, row.accountEmail) } else { onSwitch(row.accountId) }
      return
    }
    super.mouseUp(with: event)
  }

  private var cardTrackingArea: NSTrackingArea?
  public private(set) var isCardHovered: Bool = false
  public private(set) var hoveredButton: NSButton? = nil
  private var isCursorPushed = false

  private func pushPointingCursor() {
    if !isCursorPushed { NSCursor.pointingHand.push(); isCursorPushed = true }
  }
  private func popPointingCursor() {
    if isCursorPushed { NSCursor.pop(); isCursorPushed = false }
  }

  public override func viewDidMoveToWindow() {
    super.viewDidMoveToWindow()
    if window == nil { popPointingCursor() }
  }

  public override func updateTrackingAreas() {
    super.updateTrackingAreas()
    if let old = cardTrackingArea { removeTrackingArea(old) }
    let area = NSTrackingArea(rect: bounds, options: [.mouseEnteredAndExited, .mouseMoved, .cursorUpdate, .activeInActiveApp, .inVisibleRect], owner: self, userInfo: nil)
    addTrackingArea(area); self.cardTrackingArea = area
  }

  public override func mouseEntered(with event: NSEvent) {
    super.mouseEntered(with: event)
    isCardHovered = true
    box.borderColor = NSColor.labelColor.withAlphaComponent(0.38)
    box.fillColor = NSColor.quaternaryLabelColor.withAlphaComponent(0.08)
    box.needsDisplay = true
  }

  public override func mouseMoved(with event: NSEvent) {
    super.mouseMoved(with: event)
    let pt = contentContainer.convert(event.locationInWindow, from: nil)
    let targetBtn = (resetButtons + switchButtons + reloginButtons).first(where: { $0.frame.contains(pt) })

    if hoveredButton !== targetBtn {
      (hoveredButton as? MenuIconButton)?.setHoveredExplicitly(false)
      (targetBtn as? MenuIconButton)?.setHoveredExplicitly(true)
      hoveredButton = targetBtn
      if targetBtn != nil { pushPointingCursor() } else { popPointingCursor() }
    }
  }

  public override func cursorUpdate(with event: NSEvent) {
    let pt = contentContainer.convert(event.locationInWindow, from: nil)
    let isOver = hoveredButton != nil || (resetButtons + switchButtons + reloginButtons).contains(where: { $0.frame.contains(pt) })
    if isOver { NSCursor.pointingHand.set() } else { NSCursor.arrow.set() }
  }

  public override func mouseExited(with event: NSEvent) {
    super.mouseExited(with: event); isCardHovered = false; popPointingCursor()
    (hoveredButton as? MenuIconButton)?.setHoveredExplicitly(false); hoveredButton = nil
    box.borderColor = NSColor.separatorColor.withAlphaComponent(0.65)
    box.fillColor = NSColor.controlBackgroundColor.withAlphaComponent(0.25)
    box.needsDisplay = true; NSCursor.arrow.set()
  }

  @objc private func handleSwitchButton(_ sender: NSButton) {
    guard let id = sender.identifier?.rawValue, !id.isEmpty else { return }
    enclosingMenuItem?.menu?.cancelTracking(); onSwitch(id)
  }
  @objc private func handleResetButton(_ sender: NSButton) {
    guard let id = sender.identifier?.rawValue, !id.isEmpty else { return }
    let email = entries.first(where: { $0.account.id == id })?.account.email ?? id
    enclosingMenuItem?.menu?.cancelTracking(); onReset(id, email)
  }
  @objc private func handleReloginButton(_ sender: NSButton) {
    guard let id = sender.identifier?.rawValue, !id.isEmpty else { return }
    let email = entries.first(where: { $0.account.id == id })?.account.email ?? ""
    enclosingMenuItem?.menu?.cancelTracking(); onRelogin(id, email)
  }
}
