import AppKit
import Foundation

// MARK: - Inset Separator View

public final class InsetSeparatorView: NSView {
  public let horizontalInset: CGFloat
  public let lineColor: NSColor

  public init(
    frame frameRect: NSRect, horizontalInset: CGFloat = 16,
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

// MARK: - Native Menu Section Views

public final class PrimaryMenuSectionHeaderView: NSVisualEffectView {
  public let titleLabel: NSTextField
  private let iconView = NSImageView()

  public init(frame frameRect: NSRect, title: String, symbolName: String) {
    self.titleLabel = NSTextField(labelWithString: title)
    super.init(frame: frameRect)

    material = .headerView
    blendingMode = .withinWindow
    state = .followsWindowActiveState

    let symbolConfig = NSImage.SymbolConfiguration(pointSize: 13, weight: .semibold)
    iconView.image = NSImage(systemSymbolName: symbolName, accessibilityDescription: title)?
      .withSymbolConfiguration(symbolConfig)
    iconView.imageScaling = .scaleProportionallyDown
    iconView.contentTintColor = .secondaryLabelColor
    addSubview(iconView)

    titleLabel.font = NSFont.systemFont(ofSize: 12.5, weight: .semibold)
    titleLabel.textColor = .labelColor
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
    iconView.frame = NSRect(x: 16, y: 8, width: 16, height: 16)
    titleLabel.frame = NSRect(x: 40, y: 7, width: max(0, bounds.width - 56), height: 18)
  }
}

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
    countLabel.textColor = .tertiaryLabelColor
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
    countLabel.frame = NSRect(x: bounds.width - 38, y: 8, width: 22, height: 16)
    titleLabel.frame = NSRect(
      x: 38,
      y: 7,
      width: max(0, countLabel.frame.minX - 46),
      height: 18
    )
  }

  public override func draw(_ dirtyRect: NSRect) {
    super.draw(dirtyRect)
    NSColor.separatorColor.withAlphaComponent(0.55).setFill()
    NSRect(x: 12, y: 0, width: max(0, bounds.width - 24), height: 1).fill()
  }
}

// MARK: - Account Row View with ✕ Delete Button

public final class AccountRowView: NSView {
  public static let standardInset: CGFloat = 28

  public static func truncateMiddle(_ text: String, font: NSFont, maxWidth: CGFloat) -> String {
    let currentWidth = (text as NSString).size(withAttributes: [.font: font]).width
    if currentWidth <= maxWidth || text.count <= 6 {
      return text
    }
    let ellipsis = "…"
    let maxCharsHalf = (text.count - 1) / 2
    var low = 1
    var high = maxCharsHalf
    var best = "\(text.prefix(1))\(ellipsis)\(text.suffix(1))"

    while low <= high {
      let mid = (low + high) / 2
      let prefix = text.prefix(mid)
      let suffix = text.suffix(mid)
      let candidate = "\(prefix)\(ellipsis)\(suffix)"
      let w = (candidate as NSString).size(withAttributes: [.font: font]).width
      if w <= maxWidth {
        best = candidate
        low = mid + 1
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
  public let onDelete: (String, String) -> Void
  public let onRename: (String, String?, String) -> Void
  public let onSelect: (String) -> Void

  public let titleLabel: NSTextField
  public var switchButton: NSButton?
  public var deleteButton: NSButton?

  public init(
    frame: NSRect,
    accountId: String,
    accountName: String?,
    email: String,
    tier: String?,
    isCurrentActive: Bool,
    isAppSession: Bool = false,
    dotColor: NSColor,
    statusTag: String,
    statusTagColor: NSColor? = nil,
    showsInlineSwitchButton: Bool = true,
    onDelete: @escaping (String, String) -> Void = { _, _ in },
    onRename: @escaping (String, String?, String) -> Void = { _, _, _ in },
    onSelect: @escaping (String) -> Void = { _ in }
  ) {
    self.accountId = accountId
    self.accountName = accountName
    self.accountEmail = email
    self.tier = tier
    self.isCurrentActive = isCurrentActive
    self.isAppSession = isAppSession
    self.onDelete = onDelete
    self.onRename = onRename
    self.onSelect = onSelect

    let rightOffset: CGFloat =
      isAppSession ? 12 : (isCurrentActive || !showsInlineSwitchButton ? 28 : 56)
    let labelWidth = max(50, frame.width - rightOffset - Self.standardInset)
    self.titleLabel = NSTextField(
      frame: NSRect(x: Self.standardInset, y: 1, width: labelWidth, height: 20))

    if !isAppSession && !isCurrentActive && showsInlineSwitchButton {
      let sb = NSButton(frame: NSRect(x: frame.width - 50, y: 2, width: 22, height: 18))
      sb.isBordered = false
      sb.title = "⇄"
      sb.font = NSFont.systemFont(ofSize: 13, weight: .bold)
      sb.contentTintColor = NSColor.systemBlue
      sb.toolTip = L10n.switchToAccount
      self.switchButton = sb
    } else {
      self.switchButton = nil
    }

    if !isAppSession {
      let db = NSButton(frame: NSRect(x: frame.width - 26, y: 2, width: 20, height: 18))
      db.isBordered = false
      db.title = "✕"
      db.font = NSFont.systemFont(ofSize: 12, weight: .bold)
      db.contentTintColor = NSColor.secondaryLabelColor
      db.toolTip = L10n.removeAccount
      self.deleteButton = db
    } else {
      self.deleteButton = nil
    }

    super.init(frame: frame)
    self.autoresizingMask = [.width]

    // Setup Title Label
    titleLabel.isBezeled = false
    titleLabel.drawsBackground = false
    titleLabel.isEditable = false
    titleLabel.isSelectable = false
    titleLabel.lineBreakMode = .byClipping

    let dotFont = NSFont.systemFont(ofSize: 13, weight: .bold)
    let titleFont =
      isCurrentActive ? NSFont.boldSystemFont(ofSize: 13) : NSFont.systemFont(ofSize: 13)
    let tagFont = NSFont.systemFont(ofSize: 11, weight: .semibold)

    let displayTitle: String
    let cleanName = accountName?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
    let emailUsername =
      email.components(separatedBy: "@").first?.trimmingCharacters(in: .whitespacesAndNewlines)
      ?? ""
    if !cleanName.isEmpty
      && cleanName.caseInsensitiveCompare(email) != .orderedSame
      && cleanName.caseInsensitiveCompare(emailUsername) != .orderedSame
    {
      displayTitle = "\(cleanName) (\(email))"
    } else {
      displayTitle = email
    }

    let planLabel = tier ?? "Team"
    let tagString = "  \(statusTag)  \(planLabel)"

    let dotWidth = ("● " as NSString).size(withAttributes: [.font: dotFont]).width
    let tagWidth = (tagString as NSString).size(withAttributes: [.font: tagFont]).width
    let maxTitleWidth = max(20, labelWidth - dotWidth - tagWidth - 4.0)
    let resolvedDisplayTitle = Self.truncateMiddle(
      displayTitle, font: titleFont, maxWidth: maxTitleWidth)

    let rich = NSMutableAttributedString()
    rich.append(
      NSAttributedString(
        string: "● ",
        attributes: [
          .font: dotFont,
          .foregroundColor: dotColor,
        ]))
    rich.append(
      NSAttributedString(
        string: resolvedDisplayTitle,
        attributes: [
          .font: titleFont,
          .foregroundColor: NSColor.labelColor,
        ]))
    let resolvedTagColor =
      statusTagColor
      ?? (isCurrentActive
        ? (isAppSession ? NSColor.systemTeal : NSColor.systemGreen) : NSColor.secondaryLabelColor)
    rich.append(
      NSAttributedString(
        string: tagString,
        attributes: [
          .font: tagFont,
          .foregroundColor: resolvedTagColor,
        ]))
    titleLabel.attributedStringValue = rich
    addSubview(titleLabel)

    // Setup Switch Button if reserve
    if let sb = switchButton {
      sb.target = self
      sb.action = #selector(handleSwitchClick)
      sb.autoresizingMask = [.minXMargin]
      sb.setAccessibilityLabel("\(L10n.switchToAccount): \(accountEmail)")
      addSubview(sb)
    }

    // Setup Delete Button (✕)
    if let db = deleteButton {
      db.target = self
      db.action = #selector(handleDelete)
      db.autoresizingMask = [.minXMargin]
      db.setAccessibilityLabel("\(L10n.removeAccount): \(accountEmail)")
      addSubview(db)
    }
  }

  required init?(coder: NSCoder) {
    fatalError("init(coder:) has not been implemented")
  }

  @objc private func handleDelete() {
    onDelete(accountId, accountEmail)
  }

  @objc private func handleSwitchClick() {
    enclosingMenuItem?.menu?.cancelTracking()
    onSelect(accountId)
  }

  public override func resetCursorRects() {
    super.resetCursorRects()
    if !isCurrentActive || isAppSession {
      addCursorRect(bounds, cursor: .pointingHand)
    }
  }

  public override func mouseUp(with event: NSEvent) {
    let point = convert(event.locationInWindow, from: nil)
    if let db = deleteButton, db.frame.contains(point) {
      handleDelete()
      return
    }
    if let sb = switchButton, sb.frame.contains(point) {
      handleSwitchClick()
      return
    }
    if isAppSession {
      enclosingMenuItem?.menu?.cancelTracking()
      if let chatGPTApp = NSRunningApplication.runningApplications(
        withBundleIdentifier: "com.openai.chat"
      ).first {
        if #available(macOS 14.0, *) {
          chatGPTApp.activate()
        } else {
          chatGPTApp.activate(options: [.activateIgnoringOtherApps])
        }
      } else {
        NSWorkspace.shared.open(URL(fileURLWithPath: "/Applications/ChatGPT.app"))
      }
      return
    }
    if !isCurrentActive {
      enclosingMenuItem?.menu?.cancelTracking()
      onSelect(accountId)
    }
  }

  public override func menu(for event: NSEvent) -> NSMenu? {
    if isAppSession {
      return nil
    }
    let ctxMenu = NSMenu()
    ctxMenu.autoenablesItems = false

    if !isCurrentActive {
      let switchItem = NSMenuItem(
        title: L10n.switchToAccount, action: #selector(handleSwitchFromCtx), keyEquivalent: "")
      switchItem.target = self
      ctxMenu.addItem(switchItem)
      ctxMenu.addItem(NSMenuItem.separator())
    }

    let renameItem = NSMenuItem(
      title: L10n.renameAccount, action: #selector(handleRenameFromCtx), keyEquivalent: "")
    renameItem.target = self
    ctxMenu.addItem(renameItem)

    let removeItem = NSMenuItem(
      title: L10n.removeAccount, action: #selector(handleDeleteFromCtx), keyEquivalent: "")
    removeItem.target = self
    ctxMenu.addItem(removeItem)

    return ctxMenu
  }

  @objc private func handleSwitchFromCtx() {
    onSelect(accountId)
  }

  @objc private func handleRenameFromCtx() {
    onRename(accountId, accountName, accountEmail)
  }

  @objc private func handleDeleteFromCtx() {
    onDelete(accountId, accountEmail)
  }
}

public struct ReserveAccountSectionEntry {
  public let account: AccountQuota
  public let reserveIndex: Int

  public init(account: AccountQuota, reserveIndex: Int) {
    self.account = account
    self.reserveIndex = reserveIndex
  }
}

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
    self.resetButton = NSButton(frame: NSRect(x: frameRect.width - 44, y: 1, width: 22, height: 18))
    super.init(frame: frameRect)

    autoresizingMask = [.width]

    label.frame = NSRect(x: leftPadding, y: 1, width: max(100, frameRect.width - leftPadding - 50), height: 18)
    label.lineBreakMode = .byClipping
    label.autoresizingMask = [.width]
    addSubview(label)

    resetButton.isBordered = false
    resetButton.target = self
    resetButton.action = #selector(handleResetClick)
    resetButton.contentTintColor = .systemIndigo
    resetButton.toolTip = "\(L10n.resetAccountTooltip): \(accountDisplayName)"
    resetButton.setAccessibilityLabel("\(L10n.resetAccountTooltip): \(accountDisplayName)")
    let symbolConfig = NSImage.SymbolConfiguration(pointSize: 11, weight: .semibold)
    if let icon = NSImage(systemSymbolName: "arrow.counterclockwise", accessibilityDescription: L10n.resetAccountTooltip) {
      resetButton.image = icon.withSymbolConfiguration(symbolConfig)
      resetButton.imagePosition = .imageOnly
    } else {
      resetButton.title = "↺"
      resetButton.font = NSFont.systemFont(ofSize: 12, weight: .bold)
    }
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

public final class AccountSectionCardView: NSView {
  public let box = NSBox()
  public let contentContainer = NSView()
  public let headerView: AccountSectionHeaderView
  public private(set) var accountRows: [AccountRowView] = []
  public private(set) var metricLabels: [NSTextField] = []
  public private(set) var switchButtons: [NSButton] = []
  public private(set) var resetButtons: [NSButton] = []
  public let entries: [ReserveAccountSectionEntry]

  private let onSwitch: (String) -> Void
  private let onReset: (String, String) -> Void
  private let horizontalInset: CGFloat = 10

  public static func preferredHeight(for entries: [ReserveAccountSectionEntry]) -> CGFloat {
    let headerHeight: CGFloat = 33
    let bottomPadding: CGFloat = 8
    var height = headerHeight + bottomPadding
    for (index, entry) in entries.enumerated() {
      if index > 0 { height += 9 }
      height += 26  // Account identity row
      height += 18  // Sprint row
      if entry.account.weeklyPercentage != nil { height += 18 }
      if entry.account.credits > 0 { height += 18 }
      height += 24  // Native inline switch action
      if let error = entry.account.error, !error.isEmpty { height += 32 }
      height += 5
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
    onReset: @escaping (String, String) -> Void = { _, _ in }
  ) {
    self.entries = entries
    self.headerView = AccountSectionHeaderView(
      frame: NSRect(x: 10, y: 0, width: max(0, frameRect.width - 20), height: 33),
      title: title,
      count: entries.count,
      kind: kind
    )
    self.onSwitch = onSwitch
    self.onReset = onReset
    super.init(frame: frameRect)

    autoresizingMask = [.width]
    let contentWidth = max(0, frameRect.width - (horizontalInset * 2))
    let contentHeight = max(0, frameRect.height - 6)

    box.boxType = .custom
    box.titlePosition = .noTitle
    box.borderWidth = 1
    box.cornerRadius = 9
    box.borderColor = .separatorColor
    box.fillColor = NSColor.controlBackgroundColor.withAlphaComponent(0.30)
    box.contentViewMargins = .zero
    box.frame = NSRect(x: horizontalInset, y: 3, width: contentWidth, height: contentHeight)
    box.autoresizingMask = [.width, .height]
    contentContainer.frame = NSRect(x: 0, y: 0, width: contentWidth, height: contentHeight)
    contentContainer.autoresizingMask = [.width, .height]
    box.contentView = contentContainer
    addSubview(box)

    headerView.frame.origin = NSPoint(x: 0, y: contentHeight - 33)
    headerView.frame.size.width = contentWidth
    headerView.autoresizingMask = [.width, .minYMargin]
    contentContainer.addSubview(headerView)

    var cursorY = contentHeight - 38
    for (entryIndex, entry) in entries.enumerated() {
      if entryIndex > 0 {
        let separator = NSBox(
          frame: NSRect(x: 12, y: cursorY - 5, width: max(0, contentWidth - 24), height: 1))
        separator.boxType = .separator
        separator.autoresizingMask = [.width]
        contentContainer.addSubview(separator)
        cursorY -= 9
      }

      let account = entry.account
      let statusTag = L10n.reserveSlot(index: entry.reserveIndex)
      let weeklyExhausted = MenuBarAppearanceHelper.isWeeklyExhausted(account.weeklyPercentage)
      let dotColor = MenuBarAppearanceHelper.dropdownColor(
        forPercentage: account.fiveHourPercentage,
        weeklyPercentage: account.weeklyPercentage,
        planMultiplier: account.planMultiplier
      )

      cursorY -= 26
      let accountRow = AccountRowView(
        frame: NSRect(x: 0, y: cursorY, width: contentWidth, height: 24),
        accountId: account.id,
        accountName: account.displayName,
        email: account.email,
        tier: account.planBadgeString,
        isCurrentActive: false,
        isAppSession: false,
        dotColor: dotColor,
        statusTag: statusTag,
        showsInlineSwitchButton: false,
        onDelete: onDelete,
        onRename: onRename,
        onSelect: onSwitch
      )
      contentContainer.addSubview(accountRow)
      accountRows.append(accountRow)

      let sprintPercentage = weeklyExhausted ? 0.0 : account.fiveHourPercentage
      let sprintString = weeklyExhausted ? "0%" : String(format: "%.0f%%", account.fiveHourPercentage)
      let sprintColor = MenuBarAppearanceHelper.dropdownColor(
        forPercentage: account.fiveHourPercentage,
        weeklyPercentage: account.weeklyPercentage,
        planMultiplier: account.planMultiplier
      )
      let sprintRich = MenuBarAppearanceHelper.makeColoredProgressBar(
        label: "⚡ 5h Sprint: \(sprintString) ",
        percentage: sprintPercentage,
        maxPercentage: 100.0 * account.planMultiplier,
        fillColor: sprintColor
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
        let weeklyString = String(format: "%.0f%%", weeklyPercentage)
        let weeklyColor = MenuBarAppearanceHelper.dropdownColor(
          forPercentage: weeklyPercentage, planMultiplier: account.planMultiplier)
        let weeklyRich = MenuBarAppearanceHelper.makeColoredProgressBar(
          label: "🗓️ Weekly: \(weeklyString) ",
          percentage: weeklyPercentage,
          maxPercentage: 100.0 * account.planMultiplier,
          fillColor: weeklyColor
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
        addMetricLabel(credits, at: cursorY, marginRight: 52)

        let resetBtn = NSButton(
          frame: NSRect(x: contentWidth - 44, y: cursorY - 1, width: 22, height: 18))
        resetBtn.isBordered = false
        resetBtn.identifier = NSUserInterfaceItemIdentifier(account.id)
        resetBtn.target = self
        resetBtn.action = #selector(handleResetButton(_:))
        resetBtn.contentTintColor = .systemIndigo
        resetBtn.toolTip = "\(L10n.resetAccountTooltip): \(account.displayName)"
        resetBtn.setAccessibilityLabel("\(L10n.resetAccountTooltip): \(account.displayName)")
        let symbolConfig = NSImage.SymbolConfiguration(pointSize: 11, weight: .semibold)
        if let icon = NSImage(
          systemSymbolName: "arrow.counterclockwise",
          accessibilityDescription: L10n.resetAccountTooltip)
        {
          resetBtn.image = icon.withSymbolConfiguration(symbolConfig)
          resetBtn.imagePosition = .imageOnly
        } else {
          resetBtn.title = "↺"
          resetBtn.font = NSFont.systemFont(ofSize: 12, weight: .bold)
        }
        resetBtn.autoresizingMask = [.minXMargin]
        contentContainer.addSubview(resetBtn)
        resetButtons.append(resetBtn)
      }

      cursorY -= 24
      let switchButton = NSButton(
        frame: NSRect(x: 23, y: cursorY, width: max(120, contentWidth - 46), height: 22))
      switchButton.title = L10n.switchToAccount
      switchButton.identifier = NSUserInterfaceItemIdentifier(account.id)
      switchButton.target = self
      switchButton.action = #selector(handleSwitchButton(_:))
      switchButton.bezelStyle = .inline
      switchButton.isBordered = false
      switchButton.alignment = .left
      switchButton.font = NSFont.systemFont(ofSize: 12, weight: .medium)
      switchButton.contentTintColor = .controlAccentColor
      switchButton.image = NSImage(
        systemSymbolName: "arrow.triangle.2.circlepath",
        accessibilityDescription: L10n.switchToAccount)
      switchButton.imagePosition = .imageLeading
      switchButton.toolTip = "\(L10n.switchToAccount): \(account.email)"
      switchButton.setAccessibilityLabel("\(L10n.switchToAccount): \(account.email)")
      switchButton.autoresizingMask = [.width]
      contentContainer.addSubview(switchButton)
      switchButtons.append(switchButton)

      if let error = account.error, !error.isEmpty {
        cursorY -= 32
        let errorLabel = NSTextField(wrappingLabelWithString: "⚠︎ \(error)")
        errorLabel.frame = NSRect(x: 29, y: cursorY, width: max(100, contentWidth - 58), height: 30)
        errorLabel.font = NSFont.systemFont(ofSize: 10.5, weight: .medium)
        errorLabel.textColor = .systemRed
        errorLabel.maximumNumberOfLines = 2
        errorLabel.autoresizingMask = [.width]
        contentContainer.addSubview(errorLabel)
        metricLabels.append(errorLabel)
      }
      cursorY -= 5
    }

    setAccessibilityElement(false)
  }

  public required init?(coder: NSCoder) {
    fatalError("init(coder:) has not been implemented")
  }

  private func addMetricLabel(_ value: NSAttributedString, at y: CGFloat, marginRight: CGFloat = 29) {
    let label = NSTextField(labelWithAttributedString: value)
    let width = max(50, contentContainer.bounds.width - 29 - marginRight)
    label.frame = NSRect(x: 29, y: y, width: width, height: 17)
    label.lineBreakMode = .byClipping
    label.autoresizingMask = [.width]
    contentContainer.addSubview(label)
    metricLabels.append(label)
  }

  public override func resetCursorRects() {
    super.resetCursorRects()
    for btn in resetButtons {
      let rectInSelf = convert(btn.frame, from: contentContainer)
      addCursorRect(rectInSelf, cursor: .pointingHand)
    }
    for btn in switchButtons {
      let rectInSelf = convert(btn.frame, from: contentContainer)
      addCursorRect(rectInSelf, cursor: .pointingHand)
    }
  }

  public override func mouseUp(with event: NSEvent) {
    let pointInSelf = convert(event.locationInWindow, from: nil)
    let pointInContainer = contentContainer.convert(pointInSelf, from: self)
    for btn in resetButtons {
      if btn.frame.contains(pointInContainer) {
        handleResetButton(btn)
        return
      }
    }
    for btn in switchButtons {
      if btn.frame.contains(pointInContainer) {
        handleSwitchButton(btn)
        return
      }
    }
    super.mouseUp(with: event)
  }

  @objc private func handleSwitchButton(_ sender: NSButton) {
    guard let accountId = sender.identifier?.rawValue, !accountId.isEmpty else { return }
    enclosingMenuItem?.menu?.cancelTracking()
    onSwitch(accountId)
  }

  @objc private func handleResetButton(_ sender: NSButton) {
    guard let accountId = sender.identifier?.rawValue, !accountId.isEmpty else { return }
    let entry = entries.first(where: { $0.account.id == accountId })
    let email = entry?.account.email ?? accountId
    enclosingMenuItem?.menu?.cancelTracking()
    onReset(accountId, email)
  }
}

// MARK: - Main Application Delegate

public final class AppDelegate: NSObject, NSApplicationDelegate, NSMenuDelegate {
  public static let refreshIntervalKey = "codex_refresh_interval"
  public static let defaultMenuWidth: CGFloat = 440

  var statusItem: NSStatusItem!
  private var refreshTimer: Timer?
  private var lastSnapshot: MultiAccountSnapshot?
  private var menuBarIcon: NSImage?
  private var menuBarIconActive: NSImage?
  private var menuBarIconInactive: NSImage?
  private var lastKnownScreenActive: Bool = true

  private var fileWatcherSource: DispatchSourceFileSystemObject?
  private var fileWatcherFD: Int32 = -1
  private var authWatcherSource: DispatchSourceFileSystemObject?
  private var authWatcherFD: Int32 = -1

  private var statusUpdateWorkItem: DispatchWorkItem?
  private var statusRestartWorkItem: DispatchWorkItem?
  private var authRefreshWorkItem: DispatchWorkItem?
  private var authRestartWorkItem: DispatchWorkItem?

  private var detectedCLIVersion: String? = "0.1.0"
  private var isCLIUpdateAvailable: Bool = false
  private var availableCLIVersion: String? = nil

  private var refreshInterval: TimeInterval = {
    let saved = UserDefaults.standard.double(forKey: AppDelegate.refreshIntervalKey)
    return saved > 0 ? saved : 60.0  // 1 minute default
  }()

  private let client = CodexClient.shared
  private let singleGuard = SingleInstanceGuard()
  private let autoLaunchManager = AutoLaunchManager.shared
  private var ownsBackgroundAutomation = false

  private var accountsSeparatorTop: NSMenuItem?
  private var accountsSeparatorBottom: NSMenuItem?
  private var dynamicAccountItems: [NSMenuItem] = []
  private var lastUpdatedMenuItem: NSMenuItem?
  private var updateCLIItem: NSMenuItem?
  private var launchAtLoginItem: NSMenuItem?
  private var stackPercentagesItem: NSMenuItem?
  private var autoSwitchItem: NSMenuItem?
  private var autoSwitchBusinessOnlyItem: NSMenuItem?
  private var autoSwitchBusinessPriorityItem: NSMenuItem?
  private var autoResetWeeklyItem: NSMenuItem?
  private var autoResetWeeklyStatusItem: NSMenuItem?
  private var autoResetWeeklyThresholdItems: [NSMenuItem] = []
  private var autoResetWeeklyCustomThresholdItem: NSMenuItem?
  private var isRefreshing: Bool = false
  private var refreshPendingWhileBusy: Bool = false

  private static let timeOfDayFormatter: DateFormatter = {
    let fmt = DateFormatter()
    fmt.dateFormat = "HH:mm:ss"
    return fmt
  }()

  // MARK: - Lifecycle

  public func applicationDidFinishLaunching(_ notification: Notification) {
    if !singleGuard.tryAcquire() || SingleInstanceGuard.isAnotherInstanceRunning() {
      print("Another instance of Codex Monitor is already running.")
      NSApp.terminate(nil)
      return
    }
    ownsBackgroundAutomation = true
    client.startBackgroundAutomation()

    loadMenuBarIcon()

    statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
    statusItem.button?.imagePosition = .imageLeading

    buildMenu()
    setupScreenObservers()
    startStatusFileWatcher()
    startAuthFileWatcher()
    refreshCLIVersion()

    // Initial snapshot from cache
    if let snap = client.loadCachedSnapshot() {
      self.lastSnapshot = snap
      updateUI(with: snap)
    } else {
      // Placeholder status bar display while initial load happens
      updateStatusBarDisplay(
        fiveHPct: "100%",
        fiveHColor: MenuBarAppearanceHelper.menuBarColor(
          forPercentage: 100.0, isScreenActive: true),
        weeklyPct: "100%",
        weeklyColor: MenuBarAppearanceHelper.menuBarColor(
          forPercentage: 100.0, isScreenActive: true),
        accounts: [],
        isScreenActive: true
      )
    }

    // Live refresh
    refreshNow()
    startTimer()
  }

  public func applicationWillTerminate(_ notification: Notification) {
    if ownsBackgroundAutomation {
      client.stopBackgroundAutomation()
      ownsBackgroundAutomation = false
    }
    singleGuard.release()
    stopStatusFileWatcher()
    stopAuthFileWatcher()
    refreshTimer?.invalidate()
  }

  // MARK: - Menu Bar Icon Loading & Contrast Boosting

  private func loadMenuBarIcon() {
    let bundle = Bundle.main
    if let image = bundle.image(forResource: "statusbar_icon") {
      menuBarIcon = image
    } else if let iconPath = bundle.path(forResource: "statusbar_icon", ofType: "png") {
      menuBarIcon = NSImage(contentsOfFile: iconPath)
    }

    if menuBarIcon == nil {
      let searchDirs: [URL] = [
        bundle.resourceURL,
        URL(fileURLWithPath: ProcessInfo.processInfo.arguments[0]).deletingLastPathComponent()
          .appendingPathComponent("../Resources"),
        FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent(
          "dev/openai-usage-monitor/resources"),
      ].compactMap { $0 }

      for dir in searchDirs {
        let p1x = dir.appendingPathComponent("statusbar_icon.png")
        let p2x = dir.appendingPathComponent("statusbar_icon@2x.png")
        let p3x = dir.appendingPathComponent("statusbar_icon@3x.png")

        if FileManager.default.fileExists(atPath: p1x.path) {
          let image = NSImage(size: NSSize(width: 22, height: 22))
          if let rep3 = NSImageRep(contentsOf: p3x) { image.addRepresentation(rep3) }
          if let rep2 = NSImageRep(contentsOf: p2x) { image.addRepresentation(rep2) }
          if let rep1 = NSImageRep(contentsOf: p1x) { image.addRepresentation(rep1) }
          if !image.representations.isEmpty {
            menuBarIcon = image
            break
          }
        }
      }
    }

    if menuBarIcon == nil {
      let lightIcon = URL(
        fileURLWithPath: "/Applications/ChatGPT.app/Contents/Resources/icon-codex-light.png")
      if FileManager.default.fileExists(atPath: lightIcon.path) {
        menuBarIcon = NSImage(contentsOf: lightIcon)
      } else {
        let darkIcon = URL(
          fileURLWithPath: "/Applications/ChatGPT.app/Contents/Resources/icon-codex-dark-color.png")
        menuBarIcon = NSImage(contentsOf: darkIcon)
      }
    }

    if let icon = menuBarIcon {
      icon.size = NSSize(width: 22, height: 22)
      icon.isTemplate = false
      menuBarIconActive = icon
      menuBarIconInactive = icon
    }
  }

  // MARK: - Screen Parameter Observers

  private func setupScreenObservers() {
    let dCenter = NotificationCenter.default
    dCenter.addObserver(
      self, selector: #selector(handleScreenParametersChanged),
      name: NSApplication.didChangeScreenParametersNotification, object: nil)
  }

  private func isCurrentScreenActive() -> Bool {
    return true
  }

  @objc private func handleScreenParametersChanged() {
    DispatchQueue.main.async { [weak self] in
      guard let self = self else { return }
      if let snap = self.lastSnapshot {
        self.updateStatusBar(with: snap)
      }
    }
  }

  // MARK: - Status Bar Display Construction

  private func updateStatusBar(with snapshot: MultiAccountSnapshot) {
    let isScreenActive = true

    let cliMult = snapshot.cliAccount?.planMultiplier ?? snapshot.planMultiplier
    let cli5h = snapshot.fiveHourPercentage
    let cliW = snapshot.weeklyPercentage ?? cli5h
    let cliWeeklyExhausted = MenuBarAppearanceHelper.isWeeklyExhausted(snapshot.weeklyPercentage)
    let cli5hStr = cliWeeklyExhausted ? "0%" : String(format: "%.0f%%", cli5h)
    let cliWStr = String(format: "%.0f%%", cliW)
    let cli5hColor = MenuBarAppearanceHelper.menuBarColor(
      forPercentage: cli5h,
      weeklyPercentage: snapshot.weeklyPercentage,
      isScreenActive: isScreenActive,
      planMultiplier: cliMult
    )
    let cliWColor = MenuBarAppearanceHelper.menuBarColor(
      forPercentage: cliW,
      weeklyPercentage: nil,
      isScreenActive: isScreenActive,
      planMultiplier: cliMult
    )

    let cliSession = (
      fiveHPct: cli5hStr,
      fiveHColor: cli5hColor,
      weeklyPct: cliWStr,
      weeklyColor: cliWColor
    )

    var appSession:
      (fiveHPct: String, fiveHColor: NSColor, weeklyPct: String, weeklyColor: NSColor)? = nil
    if snapshot.isAppRunning, let app = snapshot.appAccount {
      let appMult = app.planMultiplier
      let app5h = app.fiveHourPercentage
      let appW = app.weeklyPercentage ?? app5h
      let appWeeklyExhausted = MenuBarAppearanceHelper.isWeeklyExhausted(app.weeklyPercentage)
      let app5hStr = appWeeklyExhausted ? "0%" : String(format: "%.0f%%", app5h)
      let appWStr = String(format: "%.0f%%", appW)
      let app5hColor = MenuBarAppearanceHelper.menuBarColor(
        forPercentage: app5h,
        weeklyPercentage: app.weeklyPercentage,
        isScreenActive: isScreenActive,
        planMultiplier: appMult
      )
      let appWColor = MenuBarAppearanceHelper.menuBarColor(
        forPercentage: appW,
        weeklyPercentage: nil,
        isScreenActive: isScreenActive,
        planMultiplier: appMult
      )
      appSession = (
        fiveHPct: app5hStr,
        fiveHColor: app5hColor,
        weeklyPct: appWStr,
        weeklyColor: appWColor
      )
    }

    guard let button = statusItem?.button else { return }
    let currentIcon =
      isScreenActive ? (menuBarIconActive ?? menuBarIcon) : (menuBarIconInactive ?? menuBarIcon)
    let stackPercentages = UserDefaults.standard.object(forKey: "stackPercentages") as? Bool ?? true

    let attributedTitle = AppDelegate.buildStatusBarAttributedString(
      icon: currentIcon,
      appSession: appSession,
      cliSession: cliSession,
      accounts: snapshot.accounts,
      isScreenActive: isScreenActive,
      useQuotaIcons: true,
      stackPercentages: stackPercentages
    )
    let compositeImage = AppDelegate.renderCompositeImage(from: attributedTitle)
    button.image = compositeImage
    button.imagePosition = .imageOnly
    button.attributedTitle = NSAttributedString()

    // Detailed Tooltip
    var tipParts: [String] = []
    if snapshot.isAppRunning, let app = snapshot.appAccount {
      var lines = ["🖥️ Codex Desktop App (\(app.email)):"]
      let sprintReset = app.sprintTimeUntilResetString
      let sprintResetStr =
        (!sprintReset.isEmpty && sprintReset != L10n.resetNow) ? " (resets: \(sprintReset))" : ""
      lines.append(
        "  • 5h Sprint: \(String(format: "%.0f%%", app.fiveHourPercentage))\(sprintResetStr)")
      if let w = app.weeklyPercentage {
        let wReset = app.weeklyTimeUntilResetString
        let wResetStr = (!wReset.isEmpty && wReset != L10n.resetNow) ? " (resets: \(wReset))" : ""
        lines.append("  • Weekly Limit: \(String(format: "%.0f%%", w))\(wResetStr)")
      }
      if app.credits > 0 {
        lines.append("  • Reset Credits: \(app.credits)")
      }
      tipParts.append(lines.joined(separator: "\n"))
    }

    if let cli = snapshot.cliAccount ?? snapshot.accounts.first(where: { $0.isCurrentActive }) {
      var lines = ["💻 Codex CLI (\(cli.email)):"]
      let sprintReset = cli.sprintTimeUntilResetString
      let sprintResetStr =
        (!sprintReset.isEmpty && sprintReset != L10n.resetNow) ? " (resets: \(sprintReset))" : ""
      lines.append(
        "  • 5h Sprint: \(String(format: "%.0f%%", cli.fiveHourPercentage))\(sprintResetStr)")
      if let w = cli.weeklyPercentage {
        let wReset = cli.weeklyTimeUntilResetString
        let wResetStr = (!wReset.isEmpty && wReset != L10n.resetNow) ? " (resets: \(wReset))" : ""
        lines.append("  • Weekly Limit: \(String(format: "%.0f%%", w))\(wResetStr)")
      }
      if cli.credits > 0 {
        lines.append("  • Reset Credits: \(cli.credits)")
      }
      tipParts.append(lines.joined(separator: "\n"))
    }

    button.toolTip =
      tipParts.isEmpty ? "OpenAI Codex Quota Monitor" : tipParts.joined(separator: "\n\n")
  }

  public static func buildStatusBarAttributedString(
    icon: NSImage?,
    appSession: (fiveHPct: String, fiveHColor: NSColor, weeklyPct: String, weeklyColor: NSColor)? =
      nil,
    cliSession: (fiveHPct: String, fiveHColor: NSColor, weeklyPct: String, weeklyColor: NSColor),
    accounts: [AccountQuota],
    isScreenActive: Bool = true,
    useQuotaIcons: Bool = true,
    stackPercentages: Bool = true
  ) -> NSAttributedString {
    let attributed = NSMutableAttributedString()

    // 1. [Codex Blue Logo] via NSTextAttachment
    if let icon = icon {
      let attachment = NSTextAttachment()
      attachment.image = icon
      let iconSize = icon.size.width > 0 ? icon.size : NSSize(width: 22, height: 22)
      let yOffset = iconSize.height >= 22.0 ? -7.0 : (iconSize.height >= 20.0 ? -6.5 : -6.0)
      attachment.bounds = CGRect(x: 0, y: yOffset, width: iconSize.width, height: iconSize.height)
      attributed.append(NSAttributedString(attachment: attachment))
      attributed.append(NSAttributedString(string: "  "))
    }

    // Fonts & Typography
    let numberFont = MenuBarAppearanceHelper.numberFont(isScreenActive: isScreenActive)
    let labelFont = MenuBarAppearanceHelper.labelFont(isScreenActive: isScreenActive)
    let sepFont = MenuBarAppearanceHelper.separatorFont(isScreenActive: isScreenActive)
    let bracketFont = MenuBarAppearanceHelper.bracketFont(isScreenActive: isScreenActive)
    let bracketShadow = MenuBarAppearanceHelper.bracketShadow(isScreenActive: isScreenActive)

    let textShadow = MenuBarAppearanceHelper.textShadow(isScreenActive: isScreenActive)
    let sepColor = MenuBarAppearanceHelper.separatorColor(isScreenActive: isScreenActive)
    let bracketColor = NSColor.white
    let kernValue: CGFloat = 0.3

    let appendSession = {
      (
        tag: String, tagColor: NSColor, fiveHPct: String, fiveHColor: NSColor, weeklyPct: String,
        weeklyColor: NSColor
      ) in
      // Tag (e.g. "APP " or "CLI ")
      attributed.append(
        NSAttributedString(
          string: tag,
          attributes: [
            .font: NSFont.systemFont(ofSize: 10, weight: .heavy),
            .foregroundColor: tagColor,
            .shadow: textShadow,
            .kern: 0.2,
            .baselineOffset: 0.5,
          ]))

      if stackPercentages {
        let attach = MenuBarAppearanceHelper.makeStackedValuesAttachment(
          fiveHPct: fiveHPct,
          fiveHColor: fiveHColor,
          weeklyPct: weeklyPct,
          weeklyColor: weeklyColor,
          isScreenActive: isScreenActive,
          useQuotaIcons: useQuotaIcons
        )
        attributed.append(NSAttributedString(attachment: attach))
      } else {
        // Horizontal layout: [⚡] 48%  [📅] 44%  or 5h: 48%  Wk: 44%
        if useQuotaIcons {
          let sprintIcon = MenuBarAppearanceHelper.makeSprintIcon(
            size: 8.0, isScreenActive: isScreenActive)
          let attach = NSTextAttachment()
          attach.image = sprintIcon
          let sprintW =
            sprintIcon.size.height > 0
            ? sprintIcon.size.width * (8.0 / sprintIcon.size.height) : 8.0
          attach.bounds = CGRect(x: 0, y: -0.5, width: sprintW, height: 8.0)
          attributed.append(NSAttributedString(attachment: attach))
          attributed.append(
            NSAttributedString(string: " ", attributes: [.font: NSFont.systemFont(ofSize: 2.5)]))
        } else {
          attributed.append(
            NSAttributedString(
              string: "5h: ",
              attributes: [
                .font: labelFont,
                .foregroundColor: NSColor.white,
                .shadow: textShadow,
                .kern: 0.2,
                .baselineOffset: 0.0,
              ]))
        }

        attributed.append(
          NSAttributedString(
            string: fiveHPct,
            attributes: [
              .font: numberFont,
              .foregroundColor: fiveHColor,
              .shadow: textShadow,
              .kern: kernValue,
              .baselineOffset: 0.0,
            ]))

        attributed.append(
          NSAttributedString(
            string: " ",
            attributes: [
              .font: NSFont.systemFont(ofSize: 4),
              .baselineOffset: 0.0,
            ]))

        if useQuotaIcons {
          let weeklyIcon = MenuBarAppearanceHelper.makeWeeklyIcon(
            size: 9.5, isScreenActive: isScreenActive)
          let attach = NSTextAttachment()
          attach.image = weeklyIcon
          attach.bounds = CGRect(x: 0, y: -0.5, width: 9.5, height: 9.5)
          attributed.append(NSAttributedString(attachment: attach))
          attributed.append(
            NSAttributedString(string: " ", attributes: [.font: NSFont.systemFont(ofSize: 2.5)]))
        } else {
          attributed.append(
            NSAttributedString(
              string: "Wk: ",
              attributes: [
                .font: labelFont,
                .foregroundColor: NSColor.white,
                .shadow: textShadow,
                .kern: 0.2,
                .baselineOffset: 0.0,
              ]))
        }

        attributed.append(
          NSAttributedString(
            string: weeklyPct,
            attributes: [
              .font: numberFont,
              .foregroundColor: weeklyColor,
              .shadow: textShadow,
              .kern: kernValue,
              .baselineOffset: 0.0,
            ]))
      }
    }

    let appTagColor =
      isScreenActive
      ? NSColor(red: 0.35, green: 0.85, blue: 1.0, alpha: 1.0)
      : NSColor(red: 0.30, green: 0.75, blue: 0.90, alpha: 1.0)
    let cliTagColor =
      isScreenActive
      ? NSColor(red: 0.65, green: 0.95, blue: 0.65, alpha: 1.0)
      : NSColor(red: 0.55, green: 0.82, blue: 0.55, alpha: 1.0)

    if let app = appSession {
      appendSession(
        "APP ", appTagColor, app.fiveHPct, app.fiveHColor, app.weeklyPct, app.weeklyColor)
      attributed.append(
        NSAttributedString(
          string: " │ ",
          attributes: [
            .font: sepFont,
            .foregroundColor: sepColor,
            .shadow: textShadow,
            .baselineOffset: 0.0,
          ]))
      appendSession(
        "CLI ", cliTagColor, cliSession.fiveHPct, cliSession.fiveHColor, cliSession.weeklyPct,
        cliSession.weeklyColor)
    } else {
      appendSession(
        "CLI ", cliTagColor, cliSession.fiveHPct, cliSession.fiveHColor, cliSession.weeklyPct,
        cliSession.weeklyColor)
    }

    let bracketBaselineOffset = MenuBarAppearanceHelper.bracketBaselineOffset(
      isScreenActive: isScreenActive)

    // 3D Hybrid Quota Indicators: [ 🛡️ ] 🛡️ 🛡️ (Active in brackets, reserves outside)
    if accounts.isEmpty {
      attributed.append(
        NSAttributedString(
          string: "  [",
          attributes: [
            .font: bracketFont,
            .foregroundColor: bracketColor,
            .shadow: bracketShadow,
            .baselineOffset: bracketBaselineOffset,
          ]))
      attributed.append(
        NSAttributedString(string: " ", attributes: [.font: NSFont.systemFont(ofSize: 2.5)]))
      let singleBadge = MenuBarAppearanceHelper.makeHybridQuotaIndicator(
        fiveHour: 100.0,
        weekly: 100.0,
        credits: 0,
        width: 6.5,
        height: 16.5,
        isScreenActive: isScreenActive
      )
      let attach = NSTextAttachment()
      attach.image = singleBadge
      attach.bounds = CGRect(x: 0, y: -5.0, width: 6.5, height: 16.5)
      attributed.append(NSAttributedString(attachment: attach))
      attributed.append(
        NSAttributedString(string: " ", attributes: [.font: NSFont.systemFont(ofSize: 2.5)]))
      attributed.append(
        NSAttributedString(
          string: "]",
          attributes: [
            .font: bracketFont,
            .foregroundColor: bracketColor,
            .shadow: bracketShadow,
            .baselineOffset: bracketBaselineOffset,
          ]))
    } else {
      let activeAcc = accounts.first(where: { $0.isCurrentActive }) ?? accounts[0]
      let reserveAccs = accounts.filter { $0.id != activeAcc.id }

      // Active account in brackets: [ badge ]
      attributed.append(
        NSAttributedString(
          string: "  [",
          attributes: [
            .font: bracketFont,
            .foregroundColor: bracketColor,
            .shadow: bracketShadow,
            .baselineOffset: bracketBaselineOffset,
          ]))
      attributed.append(
        NSAttributedString(string: " ", attributes: [.font: NSFont.systemFont(ofSize: 2.5)]))

      let f5h = activeAcc.fiveHourPercentage
      let w = activeAcc.weeklyPercentage ?? f5h
      let cr = activeAcc.credits

      let activeBadge = MenuBarAppearanceHelper.makeHybridQuotaIndicator(
        fiveHour: f5h,
        weekly: w,
        credits: cr,
        width: 6.5,
        height: 16.5,
        isScreenActive: isScreenActive
      )
      let activeAttach = NSTextAttachment()
      activeAttach.image = activeBadge
      activeAttach.bounds = CGRect(x: 0, y: -5.0, width: 6.5, height: 16.5)
      attributed.append(NSAttributedString(attachment: activeAttach))
      attributed.append(
        NSAttributedString(string: " ", attributes: [.font: NSFont.systemFont(ofSize: 2.5)]))
      attributed.append(
        NSAttributedString(
          string: "]",
          attributes: [
            .font: bracketFont,
            .foregroundColor: bracketColor,
            .shadow: bracketShadow,
            .baselineOffset: bracketBaselineOffset,
          ]))

      if !reserveAccs.isEmpty {
        attributed.append(
          NSAttributedString(string: " ", attributes: [.font: NSFont.systemFont(ofSize: 7.0)]))
      }

      // Reserve accounts outside brackets
      for (idx, acc) in reserveAccs.enumerated() {
        if idx > 0 {
          attributed.append(
            NSAttributedString(string: " ", attributes: [.font: NSFont.systemFont(ofSize: 6.5)]))
        }

        let r5h = acc.fiveHourPercentage
        let rW = acc.weeklyPercentage ?? r5h
        let rCr = acc.credits

        let reserveBadge = MenuBarAppearanceHelper.makeHybridQuotaIndicator(
          fiveHour: r5h,
          weekly: rW,
          credits: rCr,
          width: 6.5,
          height: 16.5,
          isScreenActive: isScreenActive
        )
        let attach = NSTextAttachment()
        attach.image = reserveBadge
        attach.bounds = CGRect(x: 0, y: -5.0, width: 6.5, height: 16.5)
        attributed.append(NSAttributedString(attachment: attach))
      }
    }

    return attributed
  }

  /// Overload with useModelIcons argument label for cross-project compatibility.
  public static func buildStatusBarAttributedString(
    icon: NSImage?,
    appSession: (fiveHPct: String, fiveHColor: NSColor, weeklyPct: String, weeklyColor: NSColor)? =
      nil,
    cliSession: (fiveHPct: String, fiveHColor: NSColor, weeklyPct: String, weeklyColor: NSColor),
    accounts: [AccountQuota],
    isScreenActive: Bool = true,
    useModelIcons: Bool,
    stackPercentages: Bool = true
  ) -> NSAttributedString {
    return buildStatusBarAttributedString(
      icon: icon,
      appSession: appSession,
      cliSession: cliSession,
      accounts: accounts,
      isScreenActive: isScreenActive,
      useQuotaIcons: useModelIcons,
      stackPercentages: stackPercentages
    )
  }

  /// Single session convenience overload.
  public static func buildStatusBarAttributedString(
    icon: NSImage?,
    fiveHPct: String,
    fiveHColor: NSColor,
    weeklyPct: String,
    weeklyColor: NSColor,
    accounts: [AccountQuota] = [],
    isScreenActive: Bool = true,
    useQuotaIcons: Bool = true,
    stackPercentages: Bool = true
  ) -> NSAttributedString {
    return buildStatusBarAttributedString(
      icon: icon,
      appSession: nil,
      cliSession: (
        fiveHPct: fiveHPct, fiveHColor: fiveHColor, weeklyPct: weeklyPct, weeklyColor: weeklyColor
      ),
      accounts: accounts,
      isScreenActive: isScreenActive,
      useQuotaIcons: useQuotaIcons,
      stackPercentages: stackPercentages
    )
  }

  /// Single session convenience overload with useModelIcons.
  public static func buildStatusBarAttributedString(
    icon: NSImage?,
    fiveHPct: String,
    fiveHColor: NSColor,
    weeklyPct: String,
    weeklyColor: NSColor,
    accounts: [AccountQuota] = [],
    isScreenActive: Bool = true,
    useModelIcons: Bool,
    stackPercentages: Bool = true
  ) -> NSAttributedString {
    return buildStatusBarAttributedString(
      icon: icon,
      appSession: nil,
      cliSession: (
        fiveHPct: fiveHPct, fiveHColor: fiveHColor, weeklyPct: weeklyPct, weeklyColor: weeklyColor
      ),
      accounts: accounts,
      isScreenActive: isScreenActive,
      useQuotaIcons: useModelIcons,
      stackPercentages: stackPercentages
    )
  }

  /// Backward compatibility overload for previous 3-tuple session arguments.
  public static func buildStatusBarAttributedString(
    icon: NSImage?,
    appSession: (pct: String, color: NSColor, resetDesc: String)?,
    cliSession: (pct: String, color: NSColor, resetDesc: String),
    accounts: [AccountQuota],
    isScreenActive: Bool = true
  ) -> NSAttributedString {
    let appTuple:
      (fiveHPct: String, fiveHColor: NSColor, weeklyPct: String, weeklyColor: NSColor)? =
        appSession.map {
          ($0.pct, $0.color, $0.pct, $0.color)
        }
    let cliTuple = (cliSession.pct, cliSession.color, cliSession.pct, cliSession.color)
    let stackPref = UserDefaults.standard.object(forKey: "stackPercentages") as? Bool ?? true
    return buildStatusBarAttributedString(
      icon: icon,
      appSession: appTuple,
      cliSession: cliTuple,
      accounts: accounts,
      isScreenActive: isScreenActive,
      useQuotaIcons: true,
      stackPercentages: stackPref
    )
  }

  // MARK: - Composite Image Rendering (Bypasses NSStatusBarButton text vibrancy filter)

  public static func renderCompositeImage(from attributedTitle: NSAttributedString) -> NSImage {
    let titleSize = attributedTitle.size()
    let compositeWidth = max(1.0, ceil(titleSize.width))
    let compositeHeight: CGFloat = 22.0
    let compositeImage = NSImage(
      size: NSSize(width: compositeWidth, height: compositeHeight), flipped: false
    ) { rect in
      // Draw at (0, 0.0) so the baseline established by AppKit (y = 6.0pt) preserves
      // the calibrated bounds of all attachments (icon, stacked numbers, brackets, badges).
      attributedTitle.draw(at: NSPoint(x: 0, y: 0.0))
      return true
    }
    compositeImage.isTemplate = false
    return compositeImage
  }

  private func updateStatusBarDisplay(
    fiveHPct: String,
    fiveHColor: NSColor,
    weeklyPct: String,
    weeklyColor: NSColor,
    accounts: [AccountQuota],
    isScreenActive: Bool = true
  ) {
    guard let button = statusItem?.button else { return }
    let currentIcon =
      isScreenActive ? (menuBarIconActive ?? menuBarIcon) : (menuBarIconInactive ?? menuBarIcon)
    let stackPref = UserDefaults.standard.object(forKey: "stackPercentages") as? Bool ?? true
    let attributedTitle = AppDelegate.buildStatusBarAttributedString(
      icon: currentIcon,
      appSession: nil,
      cliSession: (
        fiveHPct: fiveHPct, fiveHColor: fiveHColor, weeklyPct: weeklyPct, weeklyColor: weeklyColor
      ),
      accounts: accounts,
      isScreenActive: isScreenActive,
      useQuotaIcons: true,
      stackPercentages: stackPref
    )
    let compositeImage = AppDelegate.renderCompositeImage(from: attributedTitle)
    button.image = compositeImage
    button.imagePosition = .imageOnly
    button.attributedTitle = NSAttributedString()
  }

  // MARK: - Menu Construction

  @discardableResult
  func buildMenu() -> NSMenu {
    let menu = NSMenu()
    menu.autoenablesItems = false

    let appVersion = Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String ?? "0.1.0"
    let fullTitle = VersionHelper.formatMenuTitle(baseTitle: L10n.menuTitle, version: appVersion)
    let titleItem = NSMenuItem(title: fullTitle, action: #selector(noop), keyEquivalent: "")
    titleItem.target = self
    titleItem.attributedTitle = NSAttributedString(
      string: fullTitle,
      attributes: [
        .font: NSFont.boldSystemFont(ofSize: 13),
        .foregroundColor: NSColor.labelColor,
      ])
    menu.addItem(titleItem)

    // Legend Item with 3D Glossy Shield Sample
    let legendItem = NSMenuItem(
      title: "🛡️ " + L10n.legendCircles, action: #selector(noop), keyEquivalent: "")
    legendItem.target = self
    let legendAttr = NSMutableAttributedString()
    let sampleBadge = MenuBarAppearanceHelper.makeHybridQuotaIndicator(
      fiveHour: 100.0,
      weekly: 80.0,
      credits: 2,
      width: 6.5,
      height: 16.5,
      isScreenActive: true
    )
    let attach = NSTextAttachment()
    attach.image = sampleBadge
    attach.bounds = CGRect(x: 0, y: -5.0, width: 6.5, height: 16.5)
    legendAttr.append(NSAttributedString(attachment: attach))
    legendAttr.append(
      NSAttributedString(
        string: "  " + L10n.legendCircles,
        attributes: [
          .font: NSFont.systemFont(ofSize: 11),
          .foregroundColor: NSColor.secondaryLabelColor,
        ]))
    legendItem.attributedTitle = legendAttr
    menu.addItem(legendItem)

    menu.addItem(NSMenuItem.separator())
    accountsSeparatorTop = menu.items.last!

    let accPlaceholder = NSMenuItem(
      title: L10n.tr("loading_accounts"), action: #selector(noop), keyEquivalent: "")
    accPlaceholder.target = self
    menu.addItem(accPlaceholder)
    dynamicAccountItems = [accPlaceholder]

    menu.addItem(NSMenuItem.separator())
    accountsSeparatorBottom = menu.items.last!

    lastUpdatedMenuItem = NSMenuItem(
      title: L10n.lastUpdated(time: "..."), action: #selector(noop), keyEquivalent: "")
    lastUpdatedMenuItem?.target = self
    menu.addItem(lastUpdatedMenuItem!)

    let refreshItem = NSMenuItem(
      title: L10n.refreshNow, action: #selector(refreshNow), keyEquivalent: "r")
    refreshItem.target = self
    menu.addItem(refreshItem)

    // Refresh Interval Submenu
    let intervalItem = NSMenuItem(title: L10n.refreshInterval, action: nil, keyEquivalent: "")
    let intervalSubmenu = NSMenu()
    intervalSubmenu.autoenablesItems = false
    for (label, seconds) in [
      (L10n.minutesShort(count: 1), 60.0),
      (L10n.minutesShort(count: 5), 300.0),
      (L10n.minutesShort(count: 15), 900.0),
      (L10n.minutesShort(count: 30), 1800.0),
    ] {
      let it = NSMenuItem(title: label, action: #selector(changeInterval(_:)), keyEquivalent: "")
      it.target = self
      it.tag = Int(seconds)
      if seconds == refreshInterval { it.state = .on }
      intervalSubmenu.addItem(it)
    }
    intervalItem.submenu = intervalSubmenu
    menu.addItem(intervalItem)

    // Restart Codex App
    let restartAppItem = NSMenuItem(
      title: L10n.restartApp, action: #selector(handleRestartApp), keyEquivalent: "")
    restartAppItem.target = self
    menu.addItem(restartAppItem)

    // Restart Codex App on Switch Toggle
    let restartOnSwitch = client.getRestartAppOnSwitch()
    let restartOnSwitchItem = NSMenuItem(
      title: L10n.restartAppOnSwitch,
      action: #selector(toggleRestartAppOnSwitch(_:)),
      keyEquivalent: ""
    )
    restartOnSwitchItem.target = self
    restartOnSwitchItem.state = restartOnSwitch ? .on : .off
    menu.addItem(restartOnSwitchItem)

    // Auto-switch on Quota Depletion Toggle
    let autoSwitch = client.getAutoSwitchEnabled()
    let autoSwitchItem = NSMenuItem(
      title: L10n.autoSwitchOnLimit,
      action: #selector(toggleAutoSwitchOnLimit(_:)),
      keyEquivalent: ""
    )
    autoSwitchItem.target = self
    autoSwitchItem.state = autoSwitch ? .on : .off
    self.autoSwitchItem = autoSwitchItem
    menu.addItem(autoSwitchItem)

    // Auto-switch: Business Accounts Only Toggle
    let autoSwitchBusinessOnly = client.getAutoSwitchBusinessOnly()
    let autoSwitchBusinessOnlyItem = NSMenuItem(
      title: L10n.autoSwitchBusinessOnly,
      action: #selector(toggleAutoSwitchBusinessOnly(_:)),
      keyEquivalent: ""
    )
    autoSwitchBusinessOnlyItem.target = self
    autoSwitchBusinessOnlyItem.state = (autoSwitch && autoSwitchBusinessOnly) ? .on : .off
    self.autoSwitchBusinessOnlyItem = autoSwitchBusinessOnlyItem
    menu.addItem(autoSwitchBusinessOnlyItem)

    // Auto-switch: Business Accounts Priority Toggle
    let autoSwitchBusinessPriority = client.getAutoSwitchBusinessPriority()
    let autoSwitchBusinessPriorityItem = NSMenuItem(
      title: L10n.autoSwitchBusinessPriority,
      action: #selector(toggleAutoSwitchBusinessPriority(_:)),
      keyEquivalent: ""
    )
    autoSwitchBusinessPriorityItem.target = self
    autoSwitchBusinessPriorityItem.state = (autoSwitch && autoSwitchBusinessPriority) ? .on : .off
    self.autoSwitchBusinessPriorityItem = autoSwitchBusinessPriorityItem
    menu.addItem(autoSwitchBusinessPriorityItem)

    // Weekly reset credits are a separate, explicit opt-in policy. A submenu
    // makes the checkbox and the "time remaining" threshold visible together.
    let autoResetConfig = client.getAutoResetWeeklyConfiguration()
    let autoResetContainer = NSMenuItem(title: L10n.autoResetWeekly, action: nil, keyEquivalent: "")
    let autoResetSubmenu = NSMenu()
    autoResetSubmenu.autoenablesItems = false
    let autoResetWeeklyItem = NSMenuItem(
      title: L10n.autoResetWeekly,
      action: #selector(toggleAutoResetWeekly(_:)),
      keyEquivalent: ""
    )
    autoResetWeeklyItem.target = self
    autoResetWeeklyItem.state = autoResetConfig.enabled ? .on : .off
    self.autoResetWeeklyItem = autoResetWeeklyItem
    autoResetSubmenu.addItem(autoResetWeeklyItem)

    let autoResetStatus = NSMenuItem(
      title: L10n.autoResetWeeklyStatus(autoResetMenuStatus("disabled")),
      action: #selector(noop),
      keyEquivalent: ""
    )
    autoResetStatus.isEnabled = false
    self.autoResetWeeklyStatusItem = autoResetStatus
    autoResetSubmenu.addItem(autoResetStatus)
    autoResetSubmenu.addItem(NSMenuItem.separator())

    let thresholdTitle = NSMenuItem(
      title: L10n.autoResetWeeklyThreshold,
      action: #selector(noop),
      keyEquivalent: ""
    )
    thresholdTitle.isEnabled = false
    autoResetSubmenu.addItem(thresholdTitle)
    for (title, hours) in [
      (L10n.autoResetWeeklyAlways, 0),
      (L10n.autoResetWeeklyMoreThan(days: 1), 24),
      (L10n.autoResetWeeklyMoreThan(days: 2), 48),
      (L10n.autoResetWeeklyMoreThan(days: 3), 72),
      (L10n.autoResetWeeklyMoreThan(days: 5), 120),
    ] {
      let item = NSMenuItem(title: title, action: #selector(changeAutoResetWeeklyThreshold(_:)), keyEquivalent: "")
      item.target = self
      item.tag = hours
      item.state = autoResetConfig.minRemainingHours == hours ? .on : .off
      autoResetWeeklyThresholdItems.append(item)
      autoResetSubmenu.addItem(item)
    }
    let fixedHours = Set(autoResetWeeklyThresholdItems.map { $0.tag })
    let customThreshold = NSMenuItem(
      title: fixedHours.contains(autoResetConfig.minRemainingHours)
        ? L10n.autoResetWeeklyCustom
        : L10n.autoResetWeeklyCustomValue(hours: autoResetConfig.minRemainingHours),
      action: #selector(changeAutoResetWeeklyThreshold(_:)),
      keyEquivalent: ""
    )
    customThreshold.target = self
    customThreshold.tag = -1
    self.autoResetWeeklyCustomThresholdItem = customThreshold
    customThreshold.state = fixedHours.contains(autoResetConfig.minRemainingHours) ? .off : .on
    autoResetSubmenu.addItem(customThreshold)
    autoResetContainer.submenu = autoResetSubmenu
    menu.addItem(autoResetContainer)

    // Help Guide
    let helpItem = NSMenuItem(
      title: L10n.helpGuide, action: #selector(openHelpPage), keyEquivalent: "?")
    helpItem.target = self
    menu.addItem(helpItem)

    // CLI Status / Update
    let cliStatusTitle = VersionHelper.formatCLIStatusTitle(
      currentVersion: detectedCLIVersion,
      isUpdateAvailable: isCLIUpdateAvailable,
      availableVersion: availableCLIVersion
    )
    updateCLIItem = NSMenuItem(
      title: cliStatusTitle,
      action: isCLIUpdateAvailable ? #selector(updateCodexCLI) : #selector(noop),
      keyEquivalent: isCLIUpdateAvailable ? "u" : ""
    )
    updateCLIItem?.target = self
    menu.addItem(updateCLIItem!)

    menu.addItem(NSMenuItem.separator())

    // Stack percentages (2-Row)
    let isStacked = UserDefaults.standard.object(forKey: "stackPercentages") as? Bool ?? true
    let stackItem = NSMenuItem(
      title: L10n.stackPercentages,
      action: #selector(toggleStackPercentages),
      keyEquivalent: ""
    )
    stackItem.target = self
    stackItem.state = isStacked ? .on : .off
    stackPercentagesItem = stackItem
    menu.addItem(stackItem)

    // Launch at login
    let autostart = NSMenuItem(
      title: L10n.launchAtLogin,
      action: #selector(toggleLaunchAtLogin),
      keyEquivalent: ""
    )
    autostart.target = self
    autostart.state = autoLaunchManager.isEnabled ? .on : .off
    launchAtLoginItem = autostart
    menu.addItem(autostart)

    // Quit
    let quitItem = NSMenuItem(title: L10n.quit, action: #selector(quitApp), keyEquivalent: "q")
    quitItem.target = self
    menu.addItem(quitItem)

    statusItem?.menu = menu
    return menu
  }

  // MARK: - Update Dynamic Menu Items

  func updateUI(with snapshot: MultiAccountSnapshot) {
    updateStatusBar(with: snapshot)

    autoSwitchItem?.state = snapshot.autoSwitchEnabled ? .on : .off
    autoSwitchBusinessOnlyItem?.state =
      (snapshot.autoSwitchEnabled && snapshot.autoSwitchBusinessOnly) ? .on : .off
    autoSwitchBusinessPriorityItem?.state =
      (snapshot.autoSwitchEnabled && snapshot.autoSwitchBusinessPriority) ? .on : .off
    autoResetWeeklyItem?.state = snapshot.autoResetWeeklyEnabled ? .on : .off
    autoResetWeeklyStatusItem?.title = L10n.autoResetWeeklyStatus(
      autoResetMenuStatus(snapshot.autoResetState))
    let thresholdHours = snapshot.autoResetWeeklyMinRemainingSeconds / 3600
    for item in autoResetWeeklyThresholdItems {
      item.state = item.tag == thresholdHours ? .on : .off
    }
    autoResetWeeklyCustomThresholdItem?.state =
      autoResetWeeklyThresholdItems.contains(where: { $0.tag == thresholdHours }) ? .off : .on
    autoResetWeeklyCustomThresholdItem?.title =
      autoResetWeeklyThresholdItems.contains(where: { $0.tag == thresholdHours })
      ? L10n.autoResetWeeklyCustom
      : L10n.autoResetWeeklyCustomValue(hours: thresholdHours)

    guard let menu = statusItem?.menu else { return }

    for it in dynamicAccountItems {
      menu.removeItem(it)
    }
    dynamicAccountItems.removeAll()

    guard let topIdx = menu.items.firstIndex(of: accountsSeparatorTop!) else { return }
    var insertIdx = topIdx + 1
    let isRu = LocalizationManager.shared.currentLanguage == .ru

    // ====================================================================
    // BLOCK 1: 🖥️ Codex Desktop App (ChatGPT.app)
    // ====================================================================
    let appHeaderTitle =
      isRu ? "Codex Desktop App (сессия ChatGPT.app)" : "Codex Desktop App (ChatGPT.app)"
    let appHeader = AppDelegate.makePrimarySectionHeaderItem(
      title: appHeaderTitle, symbolName: "desktopcomputer")
    menu.insertItem(appHeader, at: insertIdx)
    dynamicAccountItems.append(appHeader)
    insertIdx += 1

    if snapshot.isAppRunning, let appAcc = snapshot.appAccount {
      let appStatusTag = isRu ? "[АКТИВЕН В APP]" : "[ACTIVE IN APP]"
      let appWeeklyExhausted = MenuBarAppearanceHelper.isWeeklyExhausted(appAcc.weeklyPercentage)
      let dotColor = MenuBarAppearanceHelper.dropdownColor(
        forPercentage: appAcc.fiveHourPercentage,
        weeklyPercentage: appAcc.weeklyPercentage,
        planMultiplier: appAcc.planMultiplier
      )

      let appItem = NSMenuItem(
        title: "● \(appAcc.email)  \(appStatusTag)", action: #selector(noop), keyEquivalent: "")
      appItem.target = self
      let rowView = AccountRowView(
        frame: NSRect(x: 0, y: 0, width: AppDelegate.defaultMenuWidth, height: 24),
        accountId: "desktop-app",
        accountName: nil,
        email: appAcc.email,
        tier: appAcc.planBadgeString,
        isCurrentActive: true,
        isAppSession: true,
        dotColor: dotColor,
        statusTag: appStatusTag,
        statusTagColor: NSColor.systemTeal
      )
      appItem.view = rowView
      menu.insertItem(appItem, at: insertIdx)
      dynamicAccountItems.append(appItem)
      insertIdx += 1

      // 5-Hour sprint bar
      let pPct = appAcc.fiveHourPercentage
      let pStr = appWeeklyExhausted ? "0%" : String(format: "%.0f%%", pPct)
      let pColor = MenuBarAppearanceHelper.dropdownColor(
        forPercentage: pPct,
        weeklyPercentage: appAcc.weeklyPercentage,
        planMultiplier: appAcc.planMultiplier
      )
      let pRich = MenuBarAppearanceHelper.makeColoredProgressBar(
        label: "  ⚡ 5h Sprint: \(pStr) ",
        percentage: appWeeklyExhausted ? 0.0 : pPct,
        maxPercentage: 100.0 * appAcc.planMultiplier,
        fillColor: pColor
      )
      let resetDesc = appAcc.sprintTimeUntilResetString
      if !resetDesc.isEmpty && resetDesc != L10n.resetNow {
        pRich.append(
          NSAttributedString(
            string: " (\(resetDesc))",
            attributes: [
              .font: NSFont.systemFont(ofSize: 11),
              .foregroundColor: NSColor.secondaryLabelColor,
            ]))
      }
      let bar5hItem = NSMenuItem(
        title: "  ⚡ 5h Sprint: \(pStr)", action: #selector(noop), keyEquivalent: "")
      bar5hItem.target = self
      bar5hItem.attributedTitle = pRich
      menu.insertItem(bar5hItem, at: insertIdx)
      dynamicAccountItems.append(bar5hItem)
      insertIdx += 1

      // Weekly bar
      if let wPct = appAcc.weeklyPercentage {
        let wStr = String(format: "%.0f%%", wPct)
        let wColor = MenuBarAppearanceHelper.dropdownColor(
          forPercentage: wPct, planMultiplier: appAcc.planMultiplier)
        let wRich = MenuBarAppearanceHelper.makeColoredProgressBar(
          label: "  🗓️ Weekly: \(wStr) ", percentage: wPct,
          maxPercentage: 100.0 * appAcc.planMultiplier, fillColor: wColor)
        let wReset = appAcc.weeklyTimeUntilResetString
        if !wReset.isEmpty && wReset != L10n.resetNow {
          wRich.append(
            NSAttributedString(
              string: " (\(wReset))",
              attributes: [
                .font: NSFont.systemFont(ofSize: 11),
                .foregroundColor: NSColor.secondaryLabelColor,
              ]))
        }
        let weekItem = NSMenuItem(
          title: "  🗓️ Weekly: \(wStr)", action: #selector(noop), keyEquivalent: "")
        weekItem.target = self
        weekItem.attributedTitle = wRich
        menu.insertItem(weekItem, at: insertIdx)
        dynamicAccountItems.append(weekItem)
        insertIdx += 1
      }

      // Credits
      if appAcc.credits > 0 {
        let credItem = NSMenuItem(
          title: "  ✨ \(L10n.resetCredits): \(appAcc.credits)", action: #selector(noop),
          keyEquivalent: "")
        credItem.target = self
        credItem.view = ResetCreditsRowView(
          frame: NSRect(x: 0, y: 0, width: AppDelegate.defaultMenuWidth, height: 20),
          credits: appAcc.credits,
          accountDisplayName: appAcc.displayName,
          leftPadding: 20,
          onReset: { [weak self] in
            self?.confirmAndResetAccount(id: appAcc.id, email: appAcc.email)
          }
        )
        menu.insertItem(credItem, at: insertIdx)
        dynamicAccountItems.append(credItem)
        insertIdx += 1
      }

      // Error display if session failed or expired
      if let err = appAcc.error, !err.isEmpty {
        let errItem = NSMenuItem(title: "  ⚠️ \(err)", action: #selector(noop), keyEquivalent: "")
        errItem.target = self
        errItem.attributedTitle = NSAttributedString(
          string: "  ⚠️ \(err)",
          attributes: [
            .font: NSFont.systemFont(ofSize: 11, weight: .medium),
            .foregroundColor: NSColor.systemRed,
          ])
        menu.insertItem(errItem, at: insertIdx)
        dynamicAccountItems.append(errItem)
        insertIdx += 1
      }
    } else {
      let notDetectedTitle =
        isRu ? "⚪ Сессия ChatGPT.app не обнаружена" : "⚪ ChatGPT.app session not detected"
      let notDetectedItem = NSMenuItem(
        title: notDetectedTitle, action: #selector(noop), keyEquivalent: "")
      notDetectedItem.target = self
      notDetectedItem.attributedTitle = NSAttributedString(
        string: notDetectedTitle,
        attributes: [
          .font: NSFont.systemFont(ofSize: 12),
          .foregroundColor: NSColor.secondaryLabelColor,
        ])
      menu.insertItem(notDetectedItem, at: insertIdx)
      dynamicAccountItems.append(notDetectedItem)
      insertIdx += 1

      let hintTitle =
        isRu
        ? "  (Запустите ChatGPT.app для синхронизации)"
        : "  (Start ChatGPT.app to monitor desktop session)"
      let hintItem = NSMenuItem(title: hintTitle, action: #selector(noop), keyEquivalent: "")
      hintItem.target = self
      hintItem.attributedTitle = NSAttributedString(
        string: hintTitle,
        attributes: [
          .font: NSFont.systemFont(ofSize: 11),
          .foregroundColor: NSColor.tertiaryLabelColor,
        ])
      menu.insertItem(hintItem, at: insertIdx)
      dynamicAccountItems.append(hintItem)
      insertIdx += 1
    }

    // Inset separator
    let sepBetween = AppDelegate.makeInsetSeparatorItem()
    menu.insertItem(sepBetween, at: insertIdx)
    dynamicAccountItems.append(sepBetween)
    insertIdx += 1

    // ====================================================================
    // BLOCK 2: 💻 Codex CLI (мульти-аккаунт ротация)
    // ====================================================================
    let cliPrimary =
      snapshot.cliAccount ?? snapshot.accounts.first(where: { $0.isCurrentActive })
      ?? snapshot.accounts.first
    let baseCliHeader =
      isRu ? "Codex CLI (мульти-аккаунт ротация)" : "Codex CLI (multi-account rotation)"
    let cliHeaderTitle: String
    if let org = cliPrimary?.effectiveOrganizationName {
      cliHeaderTitle = "\(baseCliHeader) — \(org)"
    } else {
      cliHeaderTitle = baseCliHeader
    }
    let cliHeader = AppDelegate.makePrimarySectionHeaderItem(
      title: cliHeaderTitle, symbolName: "terminal")
    menu.insertItem(cliHeader, at: insertIdx)
    dynamicAccountItems.append(cliHeader)
    insertIdx += 1

    if let activeAcc = cliPrimary {
      let statusTag = L10n.activeInCli
      let cliWeeklyExhausted = MenuBarAppearanceHelper.isWeeklyExhausted(activeAcc.weeklyPercentage)
      let dotColor = MenuBarAppearanceHelper.dropdownColor(
        forPercentage: activeAcc.fiveHourPercentage,
        weeklyPercentage: activeAcc.weeklyPercentage,
        planMultiplier: activeAcc.planMultiplier
      )

      let accItem = NSMenuItem(
        title: "● \(activeAcc.email)  \(statusTag)", action: #selector(noop), keyEquivalent: "")
      accItem.target = self

      let rowView = AccountRowView(
        frame: NSRect(x: 0, y: 0, width: AppDelegate.defaultMenuWidth, height: 24),
        accountId: activeAcc.id,
        accountName: activeAcc.displayName,
        email: activeAcc.email,
        tier: activeAcc.planBadgeString,
        isCurrentActive: true,
        isAppSession: false,
        dotColor: dotColor,
        statusTag: statusTag,
        onDelete: { [weak self] id, email in
          self?.confirmAndRemoveAccount(id: id, email: email)
        },
        onRename: { [weak self] id, name, email in
          self?.promptRenameAccount(id: id, currentName: name, email: email)
        },
        onSelect: { _ in }
      )
      accItem.view = rowView
      menu.insertItem(accItem, at: insertIdx)
      dynamicAccountItems.append(accItem)
      insertIdx += 1

      // 5h Sprint bar
      let pPct = activeAcc.fiveHourPercentage
      let pStr = cliWeeklyExhausted ? "0%" : String(format: "%.0f%%", pPct)
      let pColor = MenuBarAppearanceHelper.dropdownColor(
        forPercentage: pPct,
        weeklyPercentage: activeAcc.weeklyPercentage,
        planMultiplier: activeAcc.planMultiplier
      )
      let pRich = MenuBarAppearanceHelper.makeColoredProgressBar(
        label: "  ⚡ 5h Sprint: \(pStr) ",
        percentage: cliWeeklyExhausted ? 0.0 : pPct,
        maxPercentage: 100.0 * activeAcc.planMultiplier,
        fillColor: pColor
      )
      let resetDesc = activeAcc.sprintTimeUntilResetString
      if !resetDesc.isEmpty && resetDesc != L10n.resetNow {
        pRich.append(
          NSAttributedString(
            string: " (\(resetDesc))",
            attributes: [
              .font: NSFont.systemFont(ofSize: 11),
              .foregroundColor: NSColor.secondaryLabelColor,
            ]))
      }
      let bar5hItem = NSMenuItem(
        title: "  ⚡ 5h Sprint: \(pStr)", action: #selector(noop), keyEquivalent: "")
      bar5hItem.target = self
      bar5hItem.attributedTitle = pRich
      menu.insertItem(bar5hItem, at: insertIdx)
      dynamicAccountItems.append(bar5hItem)
      insertIdx += 1

      // Weekly bar
      if let wPct = activeAcc.weeklyPercentage {
        let wStr = String(format: "%.0f%%", wPct)
        let wColor = MenuBarAppearanceHelper.dropdownColor(
          forPercentage: wPct, planMultiplier: activeAcc.planMultiplier)
        let wRich = MenuBarAppearanceHelper.makeColoredProgressBar(
          label: "  🗓️ Weekly: \(wStr) ", percentage: wPct,
          maxPercentage: 100.0 * activeAcc.planMultiplier, fillColor: wColor)
        let wReset = activeAcc.weeklyTimeUntilResetString
        if !wReset.isEmpty && wReset != L10n.resetNow {
          wRich.append(
            NSAttributedString(
              string: " (\(wReset))",
              attributes: [
                .font: NSFont.systemFont(ofSize: 11),
                .foregroundColor: NSColor.secondaryLabelColor,
              ]))
        }
        let weekItem = NSMenuItem(
          title: "  🗓️ Weekly: \(wStr)", action: #selector(noop), keyEquivalent: "")
        weekItem.target = self
        weekItem.attributedTitle = wRich
        menu.insertItem(weekItem, at: insertIdx)
        dynamicAccountItems.append(weekItem)
        insertIdx += 1
      }

      // Credits for CLI active account
      if activeAcc.credits > 0 {
        let credItem = NSMenuItem(
          title: "  ✨ \(L10n.resetCredits): \(activeAcc.credits)", action: #selector(noop),
          keyEquivalent: "")
        credItem.target = self
        credItem.view = ResetCreditsRowView(
          frame: NSRect(x: 0, y: 0, width: AppDelegate.defaultMenuWidth, height: 20),
          credits: activeAcc.credits,
          accountDisplayName: activeAcc.displayName,
          leftPadding: 20,
          onReset: { [weak self] in
            self?.confirmAndResetAccount(id: activeAcc.id, email: activeAcc.email)
          }
        )
        menu.insertItem(credItem, at: insertIdx)
        dynamicAccountItems.append(credItem)
        insertIdx += 1
      }

      // Models Submenu
      let modelsItem = NSMenuItem(
        title: L10n.modelsMenuTitle, action: #selector(noop), keyEquivalent: "")
      modelsItem.target = self
      modelsItem.attributedTitle = NSAttributedString(
        string: "  " + L10n.modelsMenuTitle,
        attributes: [
          .font: NSFont.systemFont(ofSize: 11, weight: .medium),
          .foregroundColor: NSColor.secondaryLabelColor,
        ])
      let modelsSubmenu = NSMenu()
      modelsSubmenu.autoenablesItems = false

      let standardModels = [
        "gpt-5-5",
        "gpt-5-4",
        "gpt-5-codex",
        "o3",
        "o3-mini",
        "gpt-4.5",
      ]

      let activeModel = snapshot.activeModelName ?? "gpt-5-5"
      for m in standardModels {
        let it = NSMenuItem(title: m, action: #selector(switchModelAction(_:)), keyEquivalent: "")
        it.target = self
        it.representedObject = m
        if ModelMatcher.matches(displayName: m, target: activeModel) {
          it.state = .on
        } else {
          it.state = .off
        }
        modelsSubmenu.addItem(it)
      }
      modelsItem.submenu = modelsSubmenu
      menu.insertItem(modelsItem, at: insertIdx)
      dynamicAccountItems.append(modelsItem)
      insertIdx += 1

      // Error display for CLI active account
      if let err = activeAcc.error, !err.isEmpty {
        let errItem = NSMenuItem(title: "  ⚠️ \(err)", action: #selector(noop), keyEquivalent: "")
        errItem.target = self
        errItem.attributedTitle = NSAttributedString(
          string: "  ⚠️ \(err)",
          attributes: [
            .font: NSFont.systemFont(ofSize: 11, weight: .medium),
            .foregroundColor: NSColor.systemRed,
          ])
        menu.insertItem(errItem, at: insertIdx)
        dynamicAccountItems.append(errItem)
        insertIdx += 1
      }
    }

    // ====================================================================
    // BLOCK 3: 👥 Резервные аккаунты (CLI пул)
    // ====================================================================
    let reserveAccs = snapshot.accounts.filter { $0.id != (cliPrimary?.id ?? "") }
    if !reserveAccs.isEmpty {
      let sepReserves = AppDelegate.makeInsetSeparatorItem()
      menu.insertItem(sepReserves, at: insertIdx)
      dynamicAccountItems.append(sepReserves)
      insertIdx += 1

      let reservesHeaderTitle =
        isRu ? "Резервные аккаунты (CLI пул)" : "Reserve Accounts (CLI Pool)"
      let resHeader = AppDelegate.makePrimarySectionHeaderItem(
        title: reservesHeaderTitle, symbolName: "person.3.sequence.fill")
      menu.insertItem(resHeader, at: insertIdx)
      dynamicAccountItems.append(resHeader)
      insertIdx += 1

      // Group business accounts by organization
      var orgGroups: [String: [AccountQuota]] = [:]
      var orgOrder: [String] = []
      var personalAccs: [AccountQuota] = []

      for acc in reserveAccs {
        if acc.isBusiness {
          let orgName = acc.effectiveOrganizationName ?? (isRu ? "Бизнес" : "Business")
          if orgGroups[orgName] == nil {
            orgGroups[orgName] = []
            orgOrder.append(orgName)
          }
          orgGroups[orgName]?.append(acc)
        } else {
          personalAccs.append(acc)
        }
      }

      var globalReserveIdx = 0

      func sectionEntries(for accounts: [AccountQuota]) -> [ReserveAccountSectionEntry] {
        accounts.map { account in
          globalReserveIdx += 1
          return ReserveAccountSectionEntry(account: account, reserveIndex: globalReserveIdx)
        }
      }

      func insertAccountSection(
        title: String,
        kind: AccountSectionHeaderView.Kind,
        accounts: [AccountQuota]
      ) {
        let entries = sectionEntries(for: accounts)
        let cardItem = AppDelegate.makeAccountSectionCardItem(
          title: title,
          kind: kind,
          entries: entries,
          onSwitch: { [weak self] accountId in
            self?.executeSwitchAccount(id: accountId)
          },
          onDelete: { [weak self] accountId, email in
            self?.confirmAndRemoveAccount(id: accountId, email: email)
          },
          onRename: { [weak self] accountId, name, email in
            self?.promptRenameAccount(id: accountId, currentName: name, email: email)
          },
          onReset: { [weak self] accountId, email in
            self?.confirmAndResetAccount(id: accountId, email: email)
          }
        )
        menu.insertItem(cardItem, at: insertIdx)
        dynamicAccountItems.append(cardItem)
        insertIdx += 1
      }

      // Render business organization sections
      for orgName in orgOrder {
        let groupAccs = orgGroups[orgName] ?? []
        insertAccountSection(title: orgName, kind: .business, accounts: groupAccs)
      }

      // Render personal accounts section if any
      if !personalAccs.isEmpty {
        insertAccountSection(
          title: L10n.personalAccounts, kind: .personal, accounts: personalAccs)
      }
    }

    // Inset separator before Add Account
    let preAddSep = AppDelegate.makeInsetSeparatorItem()
    menu.insertItem(preAddSep, at: insertIdx)
    dynamicAccountItems.append(preAddSep)
    insertIdx += 1

    // "➕ Add Account..." Button
    let addAccountItem = NSMenuItem(
      title: L10n.addAccount, action: #selector(addAccountAction), keyEquivalent: "")
    addAccountItem.target = self
    menu.insertItem(addAccountItem, at: insertIdx)
    dynamicAccountItems.append(addAccountItem)
    insertIdx += 1

    // Update timestamp
    lastUpdatedMenuItem?.attributedTitle = NSAttributedString(
      string: L10n.lastUpdated(time: Self.timeOfDayFormatter.string(from: snapshot.timestamp)),
      attributes: [
        .font: NSFont.systemFont(ofSize: 12),
        .foregroundColor: NSColor.secondaryLabelColor,
      ]
    )
  }

  public static func makeInsetSeparatorItem(width: CGFloat = defaultMenuWidth, inset: CGFloat = 16)
    -> NSMenuItem
  {
    let item = NSMenuItem()
    let view = InsetSeparatorView(
      frame: NSRect(x: 0, y: 0, width: width, height: 7), horizontalInset: inset)
    item.view = view
    return item
  }

  public static func makePrimarySectionHeaderItem(
    title: String,
    symbolName: String,
    width: CGFloat = defaultMenuWidth
  ) -> NSMenuItem {
    let item = NSMenuItem(title: title, action: nil, keyEquivalent: "")
    item.isEnabled = true
    item.view = PrimaryMenuSectionHeaderView(
      frame: NSRect(x: 0, y: 0, width: width, height: 32),
      title: title,
      symbolName: symbolName
    )
    return item
  }

  public static func makeAccountSectionCardItem(
    title: String,
    kind: AccountSectionHeaderView.Kind,
    entries: [ReserveAccountSectionEntry],
    onSwitch: @escaping (String) -> Void = { _ in },
    onDelete: @escaping (String, String) -> Void = { _, _ in },
    onRename: @escaping (String, String?, String) -> Void = { _, _, _ in },
    onReset: @escaping (String, String) -> Void = { _, _ in },
    width: CGFloat = defaultMenuWidth
  ) -> NSMenuItem {
    let item = NSMenuItem(title: title, action: nil, keyEquivalent: "")
    item.isEnabled = true
    item.view = AccountSectionCardView(
      frame: NSRect(
        x: 0,
        y: 0,
        width: width,
        height: AccountSectionCardView.preferredHeight(for: entries)
      ),
      title: title,
      kind: kind,
      entries: entries,
      onSwitch: onSwitch,
      onDelete: onDelete,
      onRename: onRename,
      onReset: onReset
    )
    return item
  }

  // MARK: - Timer & Refresh Actions

  private func startTimer() {
    refreshTimer?.invalidate()
    refreshTimer = Timer.scheduledTimer(withTimeInterval: refreshInterval, repeats: true) {
      [weak self] _ in
      self?.refreshNow()
    }
  }

  @objc private func noop() {}

  @objc internal func refreshNow() {
    guard !isRefreshing else {
      refreshPendingWhileBusy = true
      return
    }
    isRefreshing = true

    lastUpdatedMenuItem?.attributedTitle = NSAttributedString(
      string: L10n.refreshing,
      attributes: [
        .font: NSFont.systemFont(ofSize: 12),
        .foregroundColor: NSColor.secondaryLabelColor,
      ]
    )
    client.refreshQuotas { [weak self] freshSnapshot in
      guard let self = self else { return }
      self.isRefreshing = false
      if let snap = freshSnapshot {
        self.lastSnapshot = snap
        self.updateUI(with: snap)
      }
      if self.refreshPendingWhileBusy {
        self.refreshPendingWhileBusy = false
        self.refreshNow()
      }
    }
  }

  private func refreshCLIVersion() {
    DispatchQueue.global(qos: .utility).async { [weak self] in
      guard let self = self else { return }
      let home = FileManager.default.homeDirectoryForCurrentUser
      let candidates = [
        home.appendingPathComponent(".local/bin/codex-mon").path,
        "/usr/local/bin/codex-mon",
        "/usr/local/bin/cxi",
        home.appendingPathComponent(".local/bin/cxi").path,
      ]
      for path in candidates {
        guard FileManager.default.isExecutableFile(atPath: path) else { continue }
        let pipe = Pipe()
        let proc = Process()
        proc.executableURL = URL(fileURLWithPath: path)
        proc.arguments = ["--version"]
        proc.standardOutput = pipe
        proc.standardError = Pipe()
        do {
          try proc.run()
          let data = pipe.fileHandleForReading.readDataToEndOfFile()
          proc.waitUntilExit()
          if let str = String(data: data, encoding: .utf8),
            let ver = VersionHelper.parseCLIVersion(from: str)
          {
            DispatchQueue.main.async { [weak self] in
              self?.detectedCLIVersion = ver
              self?.updateCLIItemDisplay()
            }
            return
          }
        } catch {
          continue
        }
      }
    }
  }

  private func updateCLIItemDisplay() {
    let title = VersionHelper.formatCLIStatusTitle(
      currentVersion: detectedCLIVersion,
      isUpdateAvailable: isCLIUpdateAvailable,
      availableVersion: availableCLIVersion
    )
    updateCLIItem?.title = title
  }

  @objc private func updateCodexCLI() {
    refreshCLIVersion()
    refreshNow()
  }

  @objc internal func changeInterval(_ sender: NSMenuItem) {
    let newInterval = TimeInterval(sender.tag)
    refreshInterval = newInterval
    UserDefaults.standard.set(newInterval, forKey: AppDelegate.refreshIntervalKey)
    if let submenu = sender.menu {
      for item in submenu.items {
        item.state = (item.tag == sender.tag) ? .on : .off
      }
    }
    startTimer()
  }

  @objc internal func addAccountAction() {
    let alert = NSAlert()
    alert.messageText = L10n.addAccountTitle
    alert.informativeText = L10n.addAccountMsg
    alert.alertStyle = .informational

    // Compute smart suggested account ID based on active email or account count
    let existingIds = Set((lastSnapshot?.accounts ?? []).map { $0.id })
    let suggestedId: String
    if let activeEmail = lastSnapshot?.activeEmail ?? client.loadCachedSnapshot()?.activeEmail,
      !activeEmail.isEmpty
    {
      let rawPrefix = activeEmail.components(separatedBy: "@").first ?? "account"
      let prefix = rawPrefix.replacingOccurrences(of: " ", with: "-")
      if !existingIds.contains(prefix) {
        suggestedId = prefix
      } else {
        var counter = 2
        while existingIds.contains("\(prefix)-\(counter)") {
          counter += 1
        }
        suggestedId = "\(prefix)-\(counter)"
      }
    } else {
      let count = (lastSnapshot?.accounts.count ?? 0) + 1
      suggestedId = "account-\(count)"
    }

    let inputField = NSTextField(frame: NSRect(x: 0, y: 0, width: 260, height: 24))
    inputField.stringValue = suggestedId
    inputField.placeholderString = suggestedId
    alert.accessoryView = inputField

    alert.addButton(withTitle: L10n.addAccountLoginBrowserBtn)
    alert.addButton(withTitle: L10n.addAccountSaveCurrentBtn)
    alert.addButton(withTitle: L10n.cancelBtn)

    NSApp.activate(ignoringOtherApps: true)
    inputField.selectText(nil)
    let response = alert.runModal()

    guard response != .alertThirdButtonReturn else { return }

    let rawId = inputField.stringValue.trimmingCharacters(in: .whitespacesAndNewlines)
    let cleanId = rawId.replacingOccurrences(of: " ", with: "-")
    let finalId = cleanId.isEmpty ? suggestedId : cleanId

    if response == .alertFirstButtonReturn {
      client.addNewAccount(id: finalId) { [weak self] success, errMsg in
        if success {
          self?.refreshNow()
          self?.showAlert(title: L10n.addAccountTitle, message: L10n.addAccountSuccess(id: finalId))
        } else {
          let desc = (errMsg?.isEmpty == false) ? errMsg! : L10n.addAccountFailedDesc
          self?.showAlert(title: L10n.addAccountFailedTitle, message: desc, style: .warning)
        }
      }
    } else if response == .alertSecondButtonReturn {
      client.saveCurrentSession(id: finalId) { [weak self] success, errMsg in
        if success {
          self?.refreshNow()
          self?.showAlert(
            title: L10n.addAccountTitle, message: L10n.addAccountSavedSuccess(id: finalId))
        } else {
          let desc = (errMsg?.isEmpty == false) ? errMsg! : L10n.addAccountFailedDesc
          self?.showAlert(title: L10n.addAccountFailedTitle, message: desc, style: .warning)
        }
      }
    }
  }

  private func showAlert(title: String, message: String, style: NSAlert.Style = .informational) {
    let alert = NSAlert()
    alert.messageText = title
    alert.informativeText = message
    alert.alertStyle = style
    alert.addButton(withTitle: "OK")
    NSApp.activate(ignoringOtherApps: true)
    alert.runModal()
  }

  internal func confirmAndRemoveAccount(id: String, email: String) {
    let alert = NSAlert()
    alert.messageText = L10n.removeAccountTitle
    alert.informativeText = L10n.removeAccountConfirm(email: email)
    alert.alertStyle = .warning
    alert.addButton(withTitle: L10n.removeConfirmBtn)
    alert.addButton(withTitle: L10n.cancelBtn)

    NSApp.activate(ignoringOtherApps: true)
    let response = alert.runModal()
    if response == .alertFirstButtonReturn {
      client.removeAccount(id: id) { [weak self] _ in
        self?.refreshNow()
      }
    }
  }

  internal func confirmAndResetAccount(id: String, email: String) {
    statusItem?.menu?.cancelTracking()
    let alert = NSAlert()
    alert.messageText = L10n.resetAccountTitle
    alert.informativeText = L10n.resetAccountConfirm(email: email)
    alert.alertStyle = .warning
    alert.addButton(withTitle: L10n.resetConfirmBtn)
    alert.addButton(withTitle: L10n.cancelBtn)

    NSApp.activate(ignoringOtherApps: true)
    let response = alert.runModal()
    if response == .alertFirstButtonReturn {
      client.resetAccount(id: id) { [weak self] success, errMsg in
        self?.refreshNow()
        if !success, let msg = errMsg, !msg.isEmpty {
          self?.showAlert(title: L10n.resetAccountTitle, message: msg, style: .warning)
        } else if success {
          self?.showAlert(
            title: L10n.resetAccountTitle,
            message: L10n.resetSuccessMsg(email: email),
            style: .informational
          )
        }
      }
    }
  }

  internal func promptRenameAccount(id: String, currentName: String?, email: String) {
    let alert = NSAlert()
    alert.messageText = L10n.renameAccountTitle
    alert.informativeText = L10n.renameAccountPrompt(email: email)
    alert.alertStyle = .informational

    let input = NSTextField(frame: NSRect(x: 0, y: 0, width: 260, height: 24))
    input.stringValue = currentName ?? ""
    input.placeholderString = email.components(separatedBy: "@").first ?? "nickname"
    alert.accessoryView = input

    alert.addButton(withTitle: L10n.saveBtn)
    alert.addButton(withTitle: L10n.clearBtn)
    alert.addButton(withTitle: L10n.cancelBtn)

    NSApp.activate(ignoringOtherApps: true)
    let response = alert.runModal()
    if response == .alertFirstButtonReturn {
      let newName = input.stringValue.trimmingCharacters(in: .whitespacesAndNewlines)
      client.renameAccount(id: id, newName: newName.isEmpty ? nil : newName) {
        [weak self] success, err in
        if !success, let err = err {
          self?.showAlert(title: L10n.renameAccountTitle, message: err, style: .warning)
        }
        self?.refreshNow()
      }
    } else if response == .alertSecondButtonReturn {
      client.renameAccount(id: id, newName: nil) { [weak self] _, _ in
        self?.refreshNow()
      }
    }
  }

  @objc private func toggleRestartAppOnSwitch(_ sender: NSMenuItem) {
    let newState = sender.state != .on
    sender.state = newState ? .on : .off
    client.setRestartAppOnSwitch(newState)
  }

  @objc private func toggleAutoResetWeekly(_ sender: NSMenuItem) {
    let current = client.getAutoResetWeeklyConfiguration()
    let enabling = !current.enabled
    if enabling {
      let alert = NSAlert()
      alert.messageText = L10n.autoResetWeeklyEnableTitle
      alert.informativeText = L10n.autoResetWeeklyEnableMessage
      alert.alertStyle = .warning
      alert.addButton(withTitle: L10n.autoResetWeeklyEnableButton)
      alert.addButton(withTitle: L10n.cancelBtn)
      NSApp.activate(ignoringOtherApps: true)
      guard alert.runModal() == .alertFirstButtonReturn else { return }
    }
    sender.state = enabling ? .on : .off
    client.setAutoResetWeekly(
      enabled: enabling,
      minRemainingHours: current.minRemainingHours
    ) { [weak self, weak sender] success in
      guard let self = self else { return }
      if !success {
        sender?.state = current.enabled ? .on : .off
        self.showAlert(
          title: L10n.autoResetWeekly,
          message: "Could not save the automatic weekly reset setting.",
          style: .warning
        )
      }
      self.refreshNow()
    }
  }

  @objc private func changeAutoResetWeeklyThreshold(_ sender: NSMenuItem) {
    if sender.tag == -1 {
      promptAutoResetWeeklyCustomThreshold()
      return
    }
    applyAutoResetWeeklyThreshold(hours: sender.tag)
  }

  private func promptAutoResetWeeklyCustomThreshold() {
    let current = client.getAutoResetWeeklyConfiguration()
    let alert = NSAlert()
    alert.messageText = L10n.autoResetWeeklyThreshold
    alert.informativeText = "Enter whole hours from 1 to 167. A credit is used only when more than this time remains before the normal weekly reset."
    alert.alertStyle = .informational
    let input = NSTextField(frame: NSRect(x: 0, y: 0, width: 220, height: 24))
    input.stringValue = String(max(1, current.minRemainingHours))
    alert.accessoryView = input
    alert.addButton(withTitle: L10n.saveBtn)
    alert.addButton(withTitle: L10n.cancelBtn)
    NSApp.activate(ignoringOtherApps: true)
    guard alert.runModal() == .alertFirstButtonReturn,
      let hours = Int(input.stringValue.trimmingCharacters(in: .whitespacesAndNewlines)),
      (1...167).contains(hours)
    else {
      return
    }
    applyAutoResetWeeklyThreshold(hours: hours)
  }

  private func applyAutoResetWeeklyThreshold(hours: Int) {
    let current = client.getAutoResetWeeklyConfiguration()
    let safeHours = min(167, max(0, hours))
    for item in autoResetWeeklyThresholdItems {
      item.state = item.tag == safeHours ? .on : .off
    }
    autoResetWeeklyCustomThresholdItem?.state =
      autoResetWeeklyThresholdItems.contains(where: { $0.tag == safeHours }) ? .off : .on
    autoResetWeeklyCustomThresholdItem?.title =
      autoResetWeeklyThresholdItems.contains(where: { $0.tag == safeHours })
      ? L10n.autoResetWeeklyCustom
      : L10n.autoResetWeeklyCustomValue(hours: safeHours)
    client.setAutoResetWeekly(enabled: current.enabled, minRemainingHours: safeHours) {
      [weak self] success in
      guard let self = self else { return }
      if !success {
        self.showAlert(
          title: L10n.autoResetWeekly,
          message: "Could not save the weekly reset threshold.",
          style: .warning
        )
      }
      self.refreshNow()
    }
  }

  private func autoResetMenuStatus(_ state: String) -> String {
    let russian = LocalizationManager.shared.currentLanguage == .ru
    switch state {
    case "disabled": return russian ? "выключено" : "disabled"
    case "ready": return russian ? "готово" : "ready"
    case "waiting_for_task": return russian ? "ожидание заблокированной задачи" : "waiting for a blocked task"
    case "waiting_for_window": return russian ? "ожидание выбранного окна" : "waiting for the selected window"
    case "no_credit": return russian ? "нет кредитов сброса" : "no reset credit"
    case "waiting_for_desktop": return russian ? "ожидание Codex Desktop" : "waiting for Codex Desktop"
    case "waiting_for_service": return russian ? "сервис сброса временно недоступен" : "reset service temporarily unavailable"
    case "waiting_for_fresh_quota": return russian ? "ожидание свежей квоты" : "waiting for fresh quota"
    case "waiting_for_other_automation": return russian ? "ожидание другой автоматизации" : "waiting for other automation"
    case "waiting_for_original_task": return russian ? "ожидание исходной заблокированной задачи" : "waiting for the original blocked task"
    case "unsupported": return russian ? "переход на сервисный reset" : "migrating to service reset"
    case "applied": return russian ? "кредит применён; ожидается обновление" : "credit applied; awaiting refresh"
    case "unknown": return russian ? "результат неизвестен; повтор с тем же ключом" : "outcome unknown; retrying safely"
    case "journal_error": return russian ? "ошибка журнала — остановлено безопасно" : "journal error — stopped safely"
    default: return state.replacingOccurrences(of: "_", with: " ")
    }
  }

  @objc private func toggleAutoSwitchOnLimit(_ sender: NSMenuItem) {
    let newState = sender.state != .on
    sender.state = newState ? .on : .off
    client.setAutoSwitchEnabled(newState)
    if !newState {
      autoSwitchBusinessOnlyItem?.state = .off
      autoSwitchBusinessPriorityItem?.state = .off
    } else {
      let bizOnly = client.getAutoSwitchBusinessOnly()
      let bizPriority = client.getAutoSwitchBusinessPriority()
      autoSwitchBusinessOnlyItem?.state = bizOnly ? .on : .off
      autoSwitchBusinessPriorityItem?.state = bizPriority ? .on : .off
    }
  }

  @objc private func toggleAutoSwitchBusinessOnly(_ sender: NSMenuItem) {
    let newState = sender.state != .on
    sender.state = newState ? .on : .off
    if newState {
      autoSwitchBusinessPriorityItem?.state = .off
      autoSwitchItem?.state = .on
    }
    client.setAutoSwitchBusinessOnly(newState)
  }

  @objc private func toggleAutoSwitchBusinessPriority(_ sender: NSMenuItem) {
    let newState = sender.state != .on
    sender.state = newState ? .on : .off
    if newState {
      autoSwitchBusinessOnlyItem?.state = .off
      autoSwitchItem?.state = .on
    }
    client.setAutoSwitchBusinessPriority(newState)
  }

  public static func determineAutoSwitchTarget(
    activeAcc: AccountQuota?,
    accounts: [AccountQuota],
    autoSwitchEnabled: Bool,
    businessOnly: Bool,
    businessPriority: Bool
  ) -> AccountQuota? {
    guard autoSwitchEnabled else { return nil }
    guard let activeAcc = activeAcc else { return nil }

    let isDepleted =
      activeAcc.fiveHourPercentage <= 0.0
      || (activeAcc.error?.localizedCaseInsensitiveContains("429") == true)
      || (activeAcc.error?.localizedCaseInsensitiveContains("limit") == true)

    let shouldPreemptForBusiness =
      businessPriority && !activeAcc.isBusiness
      && accounts.contains { acc in
        acc.id != activeAcc.id && acc.isBusiness && acc.fiveHourPercentage > 0.0 && acc.error == nil
      }

    guard isDepleted || shouldPreemptForBusiness else { return nil }

    var candidates = accounts.filter { acc in
      acc.id != activeAcc.id && acc.fiveHourPercentage > 0.0 && acc.error == nil
    }

    if businessOnly || shouldPreemptForBusiness {
      candidates = candidates.filter { $0.isBusiness }
    }

    guard !candidates.isEmpty else { return nil }

    candidates.sort { a, b in
      if businessPriority && a.isBusiness != b.isBusiness {
        return a.isBusiness
      }
      if a.credits != b.credits {
        return a.credits > b.credits
      }
      let aReset = a.resetAfterSeconds ?? Int.max
      let bReset = b.resetAfterSeconds ?? Int.max
      if aReset != bReset {
        return aReset < bReset
      }
      return a.fiveHourPercentage > b.fiveHourPercentage
    }

    return candidates.first
  }

  @objc private func handleSwitchMenuItem(_ sender: NSMenuItem) {
    if let id = sender.representedObject as? String {
      executeSwitchAccount(id: id)
    }
  }

  internal func executeSwitchAccount(id: String) {
    if let currentSnapshot = self.lastSnapshot {
      let updatedAccounts = currentSnapshot.accounts.map { acc in
        AccountQuota(
          id: acc.id,
          name: acc.name,
          email: acc.email,
          planType: acc.planType,
          isCurrentActive: acc.id.caseInsensitiveCompare(id) == .orderedSame,
          fiveHourPercentage: acc.fiveHourPercentage,
          weeklyPercentage: acc.weeklyPercentage,
          models: acc.models,
          resetTime: acc.resetTime,
          resetAfterSeconds: acc.resetAfterSeconds,
          credits: acc.credits,
          error: acc.error
        )
      }
      let targetAcc = updatedAccounts.first(where: { $0.isCurrentActive }) ?? updatedAccounts.first
      let updatedSnapshot = MultiAccountSnapshot(
        timestamp: Date(),
        activeAccountId: targetAcc?.id ?? id,
        activeEmail: targetAcc?.email ?? currentSnapshot.activeEmail,
        activePlan: targetAcc?.planType ?? currentSnapshot.activePlan,
        fiveHourPercentage: targetAcc?.fiveHourPercentage ?? currentSnapshot.fiveHourPercentage,
        weeklyPercentage: targetAcc?.weeklyPercentage ?? currentSnapshot.weeklyPercentage,
        resetTime: targetAcc?.resetTime ?? currentSnapshot.resetTime,
        resetAfterSeconds: targetAcc?.resetAfterSeconds ?? currentSnapshot.resetAfterSeconds,
        credits: targetAcc?.credits ?? currentSnapshot.credits,
        autoSwitchEnabled: currentSnapshot.autoSwitchEnabled,
        autoSwitchBusinessOnly: currentSnapshot.autoSwitchBusinessOnly,
        autoSwitchBusinessPriority: currentSnapshot.autoSwitchBusinessPriority,
        isAppRunning: currentSnapshot.isAppRunning,
        activeModelName: currentSnapshot.activeModelName,
        accounts: updatedAccounts,
        appAccount: currentSnapshot.appAccount,
        cliAccount: targetAcc
      )
      self.lastSnapshot = updatedSnapshot
      self.updateUI(with: updatedSnapshot)
    }
    client.switchToAccount(id: id) { [weak self] _ in
      self?.refreshNow()
    }
  }

  @objc internal func switchModelAction(_ sender: NSMenuItem) {
    guard let model = sender.representedObject as? String else { return }
    client.setActiveModelName(model)
    refreshNow()
  }

  @objc private func handleRestartApp() {
    client.restartCodexDesktopApp()
  }

  @objc private func openHelpPage() {
    let langCode = LocalizationManager.shared.currentLanguage.rawValue
    if let url = HelpsDocHelper.localizedHelpsHTMLURL(languageCode: langCode) {
      NSWorkspace.shared.open(url)
    } else if let fallbackURL = HelpsDocHelper.findHelpsHTMLURL() {
      NSWorkspace.shared.open(fallbackURL)
    }
  }

  @objc func toggleLaunchAtLogin() {
    let newState = !autoLaunchManager.isEnabled
    autoLaunchManager.setEnabled(newState)
    launchAtLoginItem?.state = newState ? .on : .off
  }

  @objc func toggleStackPercentages() {
    let current = UserDefaults.standard.object(forKey: "stackPercentages") as? Bool ?? true
    let updated = !current
    UserDefaults.standard.set(updated, forKey: "stackPercentages")
    stackPercentagesItem?.state = updated ? .on : .off
    if let snap = lastSnapshot {
      updateStatusBar(with: snap)
    }
  }

  @objc private func quitApp() {
    if ownsBackgroundAutomation {
      guard client.stopBackgroundAutomation() else {
        let alert = NSAlert()
        alert.alertStyle = .critical
        alert.messageText = "Could not stop Codex automation"
        alert.informativeText =
          "Codex Monitor is staying open because its background daemon or restart worker is still active."
        alert.addButton(withTitle: "OK")
        alert.runModal()
        return
      }
      ownsBackgroundAutomation = false
    }
    NSApp.terminate(nil)
  }

  // MARK: - Status File Watcher (Instant 0ms UI Updates)

  internal func startStatusFileWatcher() {
    stopStatusFileWatcher()
    let statusPath = CodexClient.statusFileURL.path
    guard FileManager.default.fileExists(atPath: statusPath) else { return }

    let fd = open(statusPath, O_EVTONLY)
    guard fd >= 0 else { return }
    self.fileWatcherFD = fd

    let source = DispatchSource.makeFileSystemObjectSource(
      fileDescriptor: fd,
      eventMask: [.write, .delete, .rename, .extend, .attrib],
      queue: DispatchQueue.main
    )

    source.setEventHandler { [weak self] in
      guard let self = self else { return }
      let flags = source.data

      self.statusUpdateWorkItem?.cancel()
      let updateItem = DispatchWorkItem { [weak self] in
        if let snap = self?.client.loadCachedSnapshot() {
          self?.lastSnapshot = snap
          self?.updateUI(with: snap)
        }
      }
      self.statusUpdateWorkItem = updateItem
      DispatchQueue.main.asyncAfter(deadline: .now() + 0.1, execute: updateItem)

      if flags.contains(.delete) || flags.contains(.rename) {
        self.stopStatusFileWatcher()
        self.statusRestartWorkItem?.cancel()
        let restartItem = DispatchWorkItem { [weak self] in
          self?.startStatusFileWatcher()
        }
        self.statusRestartWorkItem = restartItem
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.2, execute: restartItem)
      }
    }

    source.setCancelHandler {
      close(fd)
    }

    source.resume()
    self.fileWatcherSource = source
  }

  internal func stopStatusFileWatcher() {
    statusRestartWorkItem?.cancel()
    statusRestartWorkItem = nil
    fileWatcherSource?.cancel()
    fileWatcherSource = nil
    fileWatcherFD = -1
  }

  // MARK: - Auth File Watcher (Instant Auto-Save of Logins in Codex App)

  internal func startAuthFileWatcher() {
    stopAuthFileWatcher()
    let authPath = CodexClient.codexHome.appendingPathComponent("auth.json").path
    guard FileManager.default.fileExists(atPath: authPath) else {
      // Retry after a short delay if auth.json doesn't exist yet
      authRestartWorkItem?.cancel()
      let retryItem = DispatchWorkItem { [weak self] in
        self?.startAuthFileWatcher()
      }
      authRestartWorkItem = retryItem
      DispatchQueue.main.asyncAfter(deadline: .now() + 3.0, execute: retryItem)
      return
    }

    let fd = open(authPath, O_EVTONLY)
    guard fd >= 0 else { return }
    self.authWatcherFD = fd

    let source = DispatchSource.makeFileSystemObjectSource(
      fileDescriptor: fd,
      eventMask: [.write, .delete, .rename, .extend, .attrib],
      queue: DispatchQueue.main
    )

    source.setEventHandler { [weak self] in
      guard let self = self else { return }
      let flags = source.data

      // Debounce so the external app (ChatGPT.app / codex login) finishes writing
      self.authRefreshWorkItem?.cancel()
      let refreshItem = DispatchWorkItem { [weak self] in
        self?.refreshNow()
      }
      self.authRefreshWorkItem = refreshItem
      DispatchQueue.main.asyncAfter(deadline: .now() + 0.4, execute: refreshItem)

      if flags.contains(.delete) || flags.contains(.rename) {
        self.stopAuthFileWatcher()
        self.authRestartWorkItem?.cancel()
        let restartItem = DispatchWorkItem { [weak self] in
          self?.startAuthFileWatcher()
        }
        self.authRestartWorkItem = restartItem
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.5, execute: restartItem)
      }
    }

    source.setCancelHandler {
      close(fd)
    }

    source.resume()
    self.authWatcherSource = source
  }

  internal func stopAuthFileWatcher() {
    authRestartWorkItem?.cancel()
    authRestartWorkItem = nil
    authWatcherSource?.cancel()
    authWatcherSource = nil
    authWatcherFD = -1
  }
}
