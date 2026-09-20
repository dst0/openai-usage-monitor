import AppKit
import Foundation

extension AppDelegate {
  // MARK: - Menu Construction

  @discardableResult
  public func buildMenu() -> NSMenu {
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
    let legendItem = NSMenuItem(title: "🛡️ " + L10n.legendCircles, action: #selector(noop), keyEquivalent: "")
    legendItem.target = self
    let legendAttr = NSMutableAttributedString()
    let sampleBadge = MenuBarAppearanceHelper.makeHybridQuotaIndicator(
      fiveHour: 100.0, weekly: 80.0, credits: 2, width: 6.5, height: 16.5, isScreenActive: true
    )
    let attach = NSTextAttachment()
    attach.image = sampleBadge
    attach.bounds = CGRect(x: 0, y: -5.0, width: 6.5, height: 16.5)
    legendAttr.append(NSAttributedString(attachment: attach))
    legendAttr.append(NSAttributedString(string: "  " + L10n.legendCircles, attributes: [
      .font: NSFont.systemFont(ofSize: 11), .foregroundColor: NSColor.secondaryLabelColor
    ]))
    legendItem.attributedTitle = legendAttr
    menu.addItem(legendItem)

    menu.addItem(NSMenuItem.separator())
    accountsSeparatorTop = menu.items.last!

    let accPlaceholder = NSMenuItem(title: L10n.tr("loading_accounts"), action: #selector(noop), keyEquivalent: "")
    accPlaceholder.target = self
    menu.addItem(accPlaceholder)
    dynamicAccountItems = [accPlaceholder]

    menu.addItem(NSMenuItem.separator())
    accountsSeparatorBottom = menu.items.last!

    lastUpdatedMenuItem = NSMenuItem(title: L10n.lastUpdated(time: "..."), action: #selector(noop), keyEquivalent: "")
    lastUpdatedMenuItem?.target = self
    menu.addItem(lastUpdatedMenuItem!)

    let refreshItem = NSMenuItem(title: L10n.refreshNow, action: #selector(refreshNow), keyEquivalent: "r")
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
    let restartAppItem = NSMenuItem(title: L10n.restartApp, action: #selector(handleRestartApp), keyEquivalent: "")
    restartAppItem.target = self
    menu.addItem(restartAppItem)

    // Restart Codex App on Switch Toggle
    let restartOnSwitch = client.getRestartAppOnSwitch()
    let restartOnSwitchItem = NSMenuItem(
      title: L10n.restartAppOnSwitch, action: #selector(toggleRestartAppOnSwitch(_:)), keyEquivalent: ""
    )
    restartOnSwitchItem.target = self
    restartOnSwitchItem.state = restartOnSwitch ? .on : .off
    menu.addItem(restartOnSwitchItem)

    // Auto-switch on Quota Depletion Toggle
    let autoSwitch = client.getAutoSwitchEnabled()
    let autoSwitchItem = NSMenuItem(title: L10n.autoSwitchOnLimit, action: #selector(toggleAutoSwitchOnLimit(_:)), keyEquivalent: "")
    autoSwitchItem.target = self
    autoSwitchItem.state = autoSwitch ? .on : .off
    self.autoSwitchItem = autoSwitchItem
    menu.addItem(autoSwitchItem)

    // Auto-switch: Business Accounts Only Toggle
    let autoSwitchBusinessOnly = client.getAutoSwitchBusinessOnly()
    let autoSwitchBusinessOnlyItem = NSMenuItem(
      title: L10n.autoSwitchBusinessOnly, action: #selector(toggleAutoSwitchBusinessOnly(_:)), keyEquivalent: ""
    )
    autoSwitchBusinessOnlyItem.target = self
    autoSwitchBusinessOnlyItem.state = (autoSwitch && autoSwitchBusinessOnly) ? .on : .off
    self.autoSwitchBusinessOnlyItem = autoSwitchBusinessOnlyItem
    menu.addItem(autoSwitchBusinessOnlyItem)

    // Auto-switch: Business Accounts Priority Toggle
    let autoSwitchBusinessPriority = client.getAutoSwitchBusinessPriority()
    let autoSwitchBusinessPriorityItem = NSMenuItem(
      title: L10n.autoSwitchBusinessPriority, action: #selector(toggleAutoSwitchBusinessPriority(_:)), keyEquivalent: ""
    )
    autoSwitchBusinessPriorityItem.target = self
    autoSwitchBusinessPriorityItem.state = (autoSwitch && autoSwitchBusinessPriority) ? .on : .off
    self.autoSwitchBusinessPriorityItem = autoSwitchBusinessPriorityItem
    menu.addItem(autoSwitchBusinessPriorityItem)

    // Auto-distribute accounts between APP & CLI
    let autoDistributeItem = NSMenuItem(
      title: L10n.autoDistributeAppCli, action: #selector(autoDistributeAccountsAction), keyEquivalent: ""
    )
    autoDistributeItem.target = self
    menu.addItem(autoDistributeItem)

    // Weekly reset credits submenu
    let autoResetConfig = client.getAutoResetWeeklyConfiguration()
    let autoResetContainer = NSMenuItem(title: L10n.autoResetWeekly, action: nil, keyEquivalent: "")
    let autoResetSubmenu = NSMenu()
    autoResetSubmenu.autoenablesItems = false
    let autoResetWeeklyItem = NSMenuItem(
      title: L10n.autoResetWeekly, action: #selector(toggleAutoResetWeekly(_:)), keyEquivalent: ""
    )
    autoResetWeeklyItem.target = self
    autoResetWeeklyItem.state = autoResetConfig.enabled ? .on : .off
    self.autoResetWeeklyItem = autoResetWeeklyItem
    autoResetSubmenu.addItem(autoResetWeeklyItem)

    let autoResetStatus = NSMenuItem(
      title: L10n.autoResetWeeklyStatus(autoResetMenuStatus("disabled")), action: #selector(noop), keyEquivalent: ""
    )
    autoResetStatus.isEnabled = false
    self.autoResetWeeklyStatusItem = autoResetStatus
    autoResetSubmenu.addItem(autoResetStatus)
    autoResetSubmenu.addItem(NSMenuItem.separator())

    let thresholdTitle = NSMenuItem(title: L10n.autoResetWeeklyThreshold, action: #selector(noop), keyEquivalent: "")
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
      action: #selector(changeAutoResetWeeklyThreshold(_:)), keyEquivalent: ""
    )
    customThreshold.target = self
    customThreshold.tag = -1
    self.autoResetWeeklyCustomThresholdItem = customThreshold
    customThreshold.state = fixedHours.contains(autoResetConfig.minRemainingHours) ? .off : .on
    autoResetSubmenu.addItem(customThreshold)
    autoResetContainer.submenu = autoResetSubmenu
    menu.addItem(autoResetContainer)

    // Help Guide
    let helpItem = NSMenuItem(title: L10n.helpGuide, action: #selector(openHelpPage), keyEquivalent: "?")
    helpItem.target = self
    menu.addItem(helpItem)

    // CLI Status / Update
    let cliStatusTitle = VersionHelper.formatCLIStatusTitle(
      currentVersion: detectedCLIVersion, isUpdateAvailable: isCLIUpdateAvailable, availableVersion: availableCLIVersion
    )
    updateCLIItem = NSMenuItem(
      title: cliStatusTitle, action: isCLIUpdateAvailable ? #selector(updateCodexCLI) : #selector(noop),
      keyEquivalent: isCLIUpdateAvailable ? "u" : ""
    )
    updateCLIItem?.target = self
    menu.addItem(updateCLIItem!)

    menu.addItem(NSMenuItem.separator())

    // Stack percentages
    let isStacked = UserDefaults.standard.object(forKey: "stackPercentages") as? Bool ?? true
    let stackItem = NSMenuItem(title: L10n.stackPercentages, action: #selector(toggleStackPercentages), keyEquivalent: "")
    stackItem.target = self
    stackItem.state = isStacked ? .on : .off
    stackPercentagesItem = stackItem
    menu.addItem(stackItem)

    // Launch at login
    let autostart = NSMenuItem(title: L10n.launchAtLogin, action: #selector(toggleLaunchAtLogin), keyEquivalent: "")
    autostart.target = self
    autostart.state = autoLaunchManager.isEnabled ? .on : .off
    launchAtLoginItem = autostart
    menu.addItem(autostart)

    // Destructive action: Uninstall
    menu.addItem(NSMenuItem.separator())
    let uninstallItem = NSMenuItem(title: L10n.uninstallAction, action: #selector(handleUninstall), keyEquivalent: "")
    uninstallItem.target = self
    menu.addItem(uninstallItem)

    // Quit
    let quitItem = NSMenuItem(title: L10n.quit, action: #selector(quitApp), keyEquivalent: "q")
    quitItem.target = self
    menu.addItem(quitItem)

    statusItem?.menu = menu
    return menu
  }

  // MARK: - Factory Helpers

  public static func makeInsetSeparatorItem(width: CGFloat = defaultMenuWidth, inset: CGFloat = 16) -> NSMenuItem {
    let item = NSMenuItem()
    item.view = InsetSeparatorView(frame: NSRect(x: 0, y: 0, width: width, height: 7), horizontalInset: inset)
    return item
  }

  public static func makePrimarySectionHeaderItem(title: String, symbolName: String, width: CGFloat = defaultMenuWidth) -> NSMenuItem {
    let item = NSMenuItem(title: title, action: nil, keyEquivalent: "")
    item.isEnabled = true
    item.view = PrimaryMenuSectionHeaderView(frame: NSRect(x: 0, y: 0, width: width, height: 32), title: title, symbolName: symbolName)
    return item
  }

  public static func makeAccountSectionCardItem(
    title: String, kind: AccountSectionHeaderView.Kind, entries: [ReserveAccountSectionEntry],
    onSwitchCli: @escaping (String) -> Void = { _ in },
    onSwitchApp: @escaping (String) -> Void = { _ in },
    onSwitch: ((String) -> Void)? = nil,
    onDelete: @escaping (String, String) -> Void = { _, _ in },
    onRename: @escaping (String, String?, String) -> Void = { _, _, _ in },
    onReset: @escaping (String, String) -> Void = { _, _ in },
    onRelogin: @escaping (String, String) -> Void = { _, _ in },
    width: CGFloat = defaultMenuWidth
  ) -> NSMenuItem {
    let item = NSMenuItem(title: title, action: nil, keyEquivalent: "")
    item.isEnabled = true
    item.view = AccountSectionCardView(
      frame: NSRect(x: 0, y: 0, width: width, height: AccountSectionCardView.preferredHeight(for: entries)),
      title: title, kind: kind, entries: entries,
      onSwitchCli: onSwitchCli, onSwitchApp: onSwitchApp, onSwitch: onSwitch,
      onDelete: onDelete, onRename: onRename,
      onReset: onReset, onRelogin: onRelogin
    )
    return item
  }
}
