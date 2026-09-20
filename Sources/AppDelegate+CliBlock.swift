import AppKit
import Foundation

extension AppDelegate {
  // MARK: - Codex CLI Active Session Block

  internal func buildCliSessionBlock(
    snapshot: MultiAccountSnapshot,
    into menu: NSMenu,
    insertIdx: inout Int
  ) {
    let isRu = LocalizationManager.shared.currentLanguage == .ru
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

    guard let activeAcc = cliPrimary else { return }

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
      needsRelogin: activeAcc.needsRelogin,
      dotColor: dotColor,
      statusTag: statusTag,
      onDelete: { [weak self] id, email in
        self?.confirmAndRemoveAccount(id: id, email: email)
      },
      onRename: { [weak self] id, name, email in
        self?.promptRenameAccount(id: id, currentName: name, email: email)
      },
      onSelect: { _ in },
      onRelogin: { [weak self] id, email in
        self?.promptReloginAccount(id: id, email: email)
      }
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

      if activeAcc.needsRelogin {
        let reloginItem = NSMenuItem(
          title: "  🔄 \(L10n.reloginToAccount)...",
          action: #selector(handleActiveAccountRelogin(_:)),
          keyEquivalent: ""
        )
        reloginItem.target = self
        reloginItem.representedObject = activeAcc
        reloginItem.attributedTitle = NSAttributedString(
          string: "  🔄 \(L10n.reloginToAccount)...",
          attributes: [
            .font: NSFont.systemFont(ofSize: 12, weight: .medium),
            .foregroundColor: NSColor.systemOrange,
          ])
        menu.insertItem(reloginItem, at: insertIdx)
        dynamicAccountItems.append(reloginItem)
        insertIdx += 1
      }
    }

    if snapshot.isAppRunning,
      let appAcc = snapshot.appAccount,
      appAcc.id.caseInsensitiveCompare(activeAcc.id) != .orderedSame
    {
      let isRu = LocalizationManager.shared.currentLanguage == .ru
      let alignTitle = isRu ? "  🖥️ Переключить Desktop App на этот аккаунт" : "  🖥️ Switch Desktop App to this account"
      let alignItem = NSMenuItem(title: alignTitle, action: #selector(handleAlignAppWithCliAction(_:)), keyEquivalent: "")
      alignItem.target = self
      alignItem.representedObject = activeAcc.id
      alignItem.attributedTitle = NSAttributedString(
        string: alignTitle,
        attributes: [
          .font: NSFont.systemFont(ofSize: 11, weight: .semibold),
          .foregroundColor: NSColor.systemTeal,
        ])
      menu.insertItem(alignItem, at: insertIdx)
      dynamicAccountItems.append(alignItem)
      insertIdx += 1
    }
  }
}
