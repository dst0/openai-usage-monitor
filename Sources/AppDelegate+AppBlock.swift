import AppKit
import Foundation

extension AppDelegate {
  // MARK: - Desktop App Session Block

  internal func buildAppSessionBlock(
    snapshot: MultiAccountSnapshot,
    into menu: NSMenu,
    insertIdx: inout Int
  ) {
    let isRu = LocalizationManager.shared.currentLanguage == .ru
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

      if let cliAcc = snapshot.cliAccount,
        cliAcc.id.caseInsensitiveCompare(appAcc.id) != .orderedSame
      {
        let alignTitle =
          isRu ? "  💻 Переключить CLI на этот аккаунт" : "  💻 Switch CLI to this account"
        let alignItem = NSMenuItem(
          title: alignTitle, action: #selector(handleAlignCliWithAppAction(_:)), keyEquivalent: "")
        alignItem.target = self
        alignItem.representedObject = appAcc.id
        alignItem.attributedTitle = NSAttributedString(
          string: alignTitle,
          attributes: [
            .font: NSFont.systemFont(ofSize: 11, weight: .semibold),
            .foregroundColor: NSColor(red: 0.35, green: 0.75, blue: 0.35, alpha: 1.0),
          ])
        menu.insertItem(alignItem, at: insertIdx)
        dynamicAccountItems.append(alignItem)
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
  }
}
