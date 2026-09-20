import AppKit
import Foundation

/// Card container enclosing reserve account groups with symmetrical padding,
/// organization header, quota gauges, reset controls, and dual CLI/APP switch affordances.
public final class AccountSectionCardView: NSView {
  public let box = NSBox()
  public let contentContainer = NSView()
  public let headerView: AccountSectionHeaderView
  public private(set) var accountRows: [AccountRowView] = []
  public private(set) var metricLabels: [NSTextField] = []
  public private(set) var switchButtonsViews: [AccountSwitchButtonsView] = []
  public var switchButtons: [NSButton] {
    switchButtonsViews.flatMap { [$0.cliButton, $0.appButton] }
  }
  public private(set) var resetButtons: [NSButton] = []
  public private(set) var reloginButtons: [NSButton] = []
  public let entries: [ReserveAccountSectionEntry]

  internal let onSwitchCli: (String) -> Void
  internal let onSwitchApp: (String) -> Void
  internal let onReset: (String, String) -> Void
  internal let onRelogin: (String, String) -> Void
  private let horizontalInset: CGFloat = 10

  internal var cardTrackingArea: NSTrackingArea?
  public internal(set) var isCardHovered: Bool = false
  public internal(set) var hoveredButton: NSButton? = nil
  internal var isCursorPushed = false

  public static func preferredHeight(for entries: [ReserveAccountSectionEntry]) -> CGFloat {
    var height: CGFloat = 41
    for (index, entry) in entries.enumerated() {
      if index > 0 { height += 9 }
      height += 73  // 26 (Identity) + 18 (Sprint) + 24 (Action) + 5 (Bottom)
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
    onSwitchCli: @escaping (String) -> Void = { _ in },
    onSwitchApp: @escaping (String) -> Void = { _ in },
    onSwitch: ((String) -> Void)? = nil,
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
    self.onSwitchCli = onSwitch ?? onSwitchCli
    self.onSwitchApp = onSwitchApp
    self.onReset = onReset
    self.onRelogin = onRelogin
    super.init(frame: frameRect)

    autoresizingMask = [.width]
    let contentWidth = max(0, frameRect.width - (horizontalInset * 2)),
      contentHeight = max(0, frameRect.height - 6)

    box.boxType = .custom
    box.titlePosition = .noTitle
    box.borderWidth = 1
    box.cornerRadius = 9
    box.borderColor = NSColor.separatorColor.withAlphaComponent(0.65)
    box.fillColor = NSColor.controlBackgroundColor.withAlphaComponent(0.25)
    box.contentViewMargins = .zero
    box.frame = NSRect(
      x: horizontalInset, y: 3, width: contentWidth, height: contentHeight)
    box.autoresizingMask = [.width, .height]
    contentContainer.frame = NSRect(
      x: 0, y: 0, width: contentWidth, height: contentHeight)
    contentContainer.autoresizingMask = [.width, .height]
    box.contentView = contentContainer
    addSubview(box)

    headerView.frame = NSRect(
      x: 0, y: contentHeight - 33, width: contentWidth, height: 33)
    headerView.autoresizingMask = [.width, .minYMargin]
    contentContainer.addSubview(headerView)

    var cursorY = contentHeight - 38
    for (entryIndex, entry) in entries.enumerated() {
      if entryIndex > 0 {
        let sep = NSBox(
          frame: NSRect(
            x: 12, y: cursorY - 5, width: max(0, contentWidth - 24), height: 1))
        sep.boxType = .separator
        sep.autoresizingMask = [.width]
        contentContainer.addSubview(sep)
        cursorY -= 9
      }

      let account = entry.account
      let statusTag = L10n.reserveSlot(index: entry.reserveIndex)
      let weeklyExhausted = MenuBarAppearanceHelper.isWeeklyExhausted(
        account.weeklyPercentage)
      let dotColor = MenuBarAppearanceHelper.dropdownColor(
        forPercentage: account.fiveHourPercentage,
        weeklyPercentage: account.weeklyPercentage,
        planMultiplier: account.planMultiplier
      )

      cursorY -= 26
      let accountRow = AccountRowView(
        frame: NSRect(x: 0, y: cursorY, width: contentWidth, height: 24),
        accountId: account.id, accountName: account.displayName,
        email: account.email,
        tier: account.planBadgeString, isCurrentActive: false,
        isAppSession: false,
        needsRelogin: account.needsRelogin, dotColor: dotColor,
        statusTag: statusTag,
        showsInlineSwitchButton: false, onDelete: onDelete, onRename: onRename,
        onSelect: self.onSwitchCli, onRelogin: onRelogin
      )
      accountRow.managesOwnTracking = false
      contentContainer.addSubview(accountRow)
      accountRows.append(accountRow)

      let sprintPercentage = weeklyExhausted ? 0.0 : account.fiveHourPercentage
      let sprintString =
        weeklyExhausted
        ? "0%" : String(format: "%.0f%%", account.fiveHourPercentage)
      let sprintColor = MenuBarAppearanceHelper.dropdownColor(
        forPercentage: account.fiveHourPercentage,
        weeklyPercentage: account.weeklyPercentage,
        planMultiplier: account.planMultiplier
      )
      let sprintRich = MenuBarAppearanceHelper.makeColoredProgressBar(
        label: "⚡ 5h Sprint: \(sprintString) ", percentage: sprintPercentage,
        maxPercentage: 100.0 * account.planMultiplier, fillColor: sprintColor
      )
      let sprintReset = account.sprintTimeUntilResetString
      if !sprintReset.isEmpty && sprintReset != L10n.resetNow {
        sprintRich.append(
          NSAttributedString(
            string: " (\(sprintReset))",
            attributes: [
              .font: NSFont.systemFont(ofSize: 11),
              .foregroundColor: NSColor.secondaryLabelColor,
            ]))
      }
      cursorY -= 18
      addMetricLabel(sprintRich, at: cursorY)

      if let weeklyPercentage = account.weeklyPercentage {
        let weeklyColor = MenuBarAppearanceHelper.dropdownColor(
          forPercentage: weeklyPercentage, planMultiplier: account.planMultiplier
        )
        let weeklyRich = MenuBarAppearanceHelper.makeColoredProgressBar(
          label: String(format: "🗓️ Weekly: %.0f%% ", weeklyPercentage),
          percentage: weeklyPercentage,
          maxPercentage: 100.0 * account.planMultiplier, fillColor: weeklyColor
        )
        let weeklyReset = account.weeklyTimeUntilResetString
        if !weeklyReset.isEmpty && weeklyReset != L10n.resetNow {
          weeklyRich.append(
            NSAttributedString(
              string: " (\(weeklyReset))",
              attributes: [
                .font: NSFont.systemFont(ofSize: 11),
                .foregroundColor: NSColor.secondaryLabelColor,
              ]))
        }
        cursorY -= 18
        addMetricLabel(weeklyRich, at: cursorY)
      }

      if account.credits > 0 {
        cursorY -= 18
        let credits = NSAttributedString(
          string: "✨ \(L10n.resetCredits): \(account.credits)",
          attributes: [
            .font: NSFont.systemFont(ofSize: 11, weight: .medium),
            .foregroundColor: NSColor.systemIndigo,
          ])
        addMetricLabel(credits, at: cursorY, marginRight: 36)

        let resetBtn = MenuIconButton(
          frame: NSRect(
            x: contentWidth - 28, y: cursorY - 1, width: 22, height: 18),
          symbolName: "arrow.counterclockwise", pointSize: 11, weight: .semibold,
          tintColor: .systemIndigo, hoverTintColor: .systemIndigo,
          tooltip: "\(L10n.resetAccountTooltip): \(account.displayName)",
          accessibilityLabel:
            "\(L10n.resetAccountTooltip): \(account.displayName)"
        )
        if resetBtn.image == nil {
          resetBtn.title = "↺"
          resetBtn.font = NSFont.systemFont(ofSize: 12, weight: .bold)
        }
        resetBtn.identifier = NSUserInterfaceItemIdentifier(account.id)
        resetBtn.target = self
        resetBtn.action = #selector(handleResetButton(_:))
        resetBtn.managesOwnTracking = false
        resetBtn.autoresizingMask = [.minXMargin]
        contentContainer.addSubview(resetBtn)
        resetButtons.append(resetBtn)
      }

      if !account.needsRelogin {
        cursorY -= 24
        let switchButtonsView = AccountSwitchButtonsView(
          frame: NSRect(
            x: 28, y: cursorY, width: max(0, contentWidth - 56), height: 22),
          accountId: account.id,
          accountEmail: account.email,
          isCliActive: entry.isCliActive,
          isAppActive: entry.isAppActive,
          onSwitchCli: self.onSwitchCli,
          onSwitchApp: self.onSwitchApp
        )
        contentContainer.addSubview(switchButtonsView)
        switchButtonsViews.append(switchButtonsView)
      }

      if let error = account.error, !error.isEmpty {
        cursorY -= 32
        let errorLabel = NSTextField(wrappingLabelWithString: "⚠︎ \(error)")
        errorLabel.frame = NSRect(
          x: 29, y: cursorY, width: max(100, contentWidth - 58), height: 30)
        errorLabel.font = NSFont.systemFont(ofSize: 10.5, weight: .medium)
        errorLabel.textColor = .systemRed
        errorLabel.maximumNumberOfLines = 2
        errorLabel.autoresizingMask = [.width]
        contentContainer.addSubview(errorLabel)
        metricLabels.append(errorLabel)
      }

      if account.needsRelogin {
        cursorY -= 24
        let reloginBtn = MenuIconButton(
          frame: NSRect(
            x: 28, y: cursorY, width: min(contentWidth - 56, 170), height: 22),
          title: L10n.reloginToAccount,
          symbolName: "arrow.clockwise.circle.fill",
          pointSize: 11, weight: .semibold,
          tintColor: .systemOrange, hoverTintColor: .systemOrange, isCapsule: true,
          tooltip: "\(L10n.reloginToAccount): \(account.email)",
          accessibilityLabel: "\(L10n.reloginToAccount): \(account.email)"
        )
        reloginBtn.identifier = NSUserInterfaceItemIdentifier(account.id)
        reloginBtn.target = self
        reloginBtn.action = #selector(handleReloginButton(_:))
        reloginBtn.managesOwnTracking = false
        contentContainer.addSubview(reloginBtn)
        reloginButtons.append(reloginBtn)
      }
      cursorY -= 5
    }
    setAccessibilityElement(false)
  }

  public required init?(coder: NSCoder) {
    fatalError("init(coder:) has not been implemented")
  }

  private func addMetricLabel(
    _ value: NSAttributedString, at y: CGFloat, marginRight: CGFloat = 29
  ) {
    let label = NSTextField(labelWithAttributedString: value)
    let width = max(50, contentContainer.bounds.width - 29 - marginRight)
    label.frame = NSRect(x: 29, y: y, width: width, height: 17)
    label.lineBreakMode = .byClipping
    label.autoresizingMask = [.width]
    contentContainer.addSubview(label)
    metricLabels.append(label)
  }
}
