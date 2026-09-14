import AppKit
import Foundation

extension AppDelegate {
  // MARK: - Update Dynamic Menu Items

  public func updateUI(with snapshot: MultiAccountSnapshot) {
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

    // BLOCK 1: Codex Desktop App (ChatGPT.app)
    buildAppSessionBlock(snapshot: snapshot, into: menu, insertIdx: &insertIdx)

    // Inset separator between App and CLI blocks
    let sepBetween = AppDelegate.makeInsetSeparatorItem()
    menu.insertItem(sepBetween, at: insertIdx)
    dynamicAccountItems.append(sepBetween)
    insertIdx += 1

    // BLOCK 2: Codex CLI (multi-account rotation)
    buildCliSessionBlock(snapshot: snapshot, into: menu, insertIdx: &insertIdx)

    // BLOCK 3: Reserve accounts (CLI pool)
    buildReserveSections(snapshot: snapshot, into: menu, insertIdx: &insertIdx)

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
}
