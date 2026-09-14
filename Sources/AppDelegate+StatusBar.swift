import AppKit
import Foundation

extension AppDelegate {
  // MARK: - Status Bar Display Construction

  internal func updateStatusBar(with snapshot: MultiAccountSnapshot) {
    let isScreenActive = true
    let cliMult = snapshot.cliAccount?.planMultiplier ?? snapshot.planMultiplier
    let cli5h = snapshot.fiveHourPercentage
    let cliW = snapshot.weeklyPercentage ?? cli5h
    let cliWeeklyExhausted = MenuBarAppearanceHelper.isWeeklyExhausted(snapshot.weeklyPercentage)
    let cli5hStr = cliWeeklyExhausted ? "0%" : String(format: "%.0f%%", cli5h)
    let cliWStr = String(format: "%.0f%%", cliW)
    let cli5hColor = MenuBarAppearanceHelper.menuBarColor(
      forPercentage: cli5h, weeklyPercentage: snapshot.weeklyPercentage, isScreenActive: isScreenActive, planMultiplier: cliMult
    )
    let cliWColor = MenuBarAppearanceHelper.menuBarColor(
      forPercentage: cliW, weeklyPercentage: nil, isScreenActive: isScreenActive, planMultiplier: cliMult
    )
    let cliSession = (fiveHPct: cli5hStr, fiveHColor: cli5hColor, weeklyPct: cliWStr, weeklyColor: cliWColor)

    var appSession: (fiveHPct: String, fiveHColor: NSColor, weeklyPct: String, weeklyColor: NSColor)? = nil
    if snapshot.isAppRunning, let app = snapshot.appAccount {
      let appMult = app.planMultiplier
      let app5h = app.fiveHourPercentage
      let appW = app.weeklyPercentage ?? app5h
      let appWeeklyExhausted = MenuBarAppearanceHelper.isWeeklyExhausted(app.weeklyPercentage)
      let app5hStr = appWeeklyExhausted ? "0%" : String(format: "%.0f%%", app5h)
      let appWStr = String(format: "%.0f%%", appW)
      let app5hColor = MenuBarAppearanceHelper.menuBarColor(
        forPercentage: app5h, weeklyPercentage: app.weeklyPercentage, isScreenActive: isScreenActive, planMultiplier: appMult
      )
      let appWColor = MenuBarAppearanceHelper.menuBarColor(
        forPercentage: appW, weeklyPercentage: nil, isScreenActive: isScreenActive, planMultiplier: appMult
      )
      appSession = (fiveHPct: app5hStr, fiveHColor: app5hColor, weeklyPct: appWStr, weeklyColor: appWColor)
    }

    guard let button = statusItem?.button else { return }
    let currentIcon = isScreenActive ? (menuBarIconActive ?? menuBarIcon) : (menuBarIconInactive ?? menuBarIcon)
    let stackPercentages = UserDefaults.standard.object(forKey: "stackPercentages") as? Bool ?? true

    let attributedTitle = AppDelegate.buildStatusBarAttributedString(
      icon: currentIcon, appSession: appSession, cliSession: cliSession, accounts: snapshot.accounts,
      isScreenActive: isScreenActive, useQuotaIcons: true, stackPercentages: stackPercentages
    )
    let compositeImage = AppDelegate.renderCompositeImage(from: attributedTitle)
    button.image = compositeImage
    button.imagePosition = .imageOnly
    button.attributedTitle = NSAttributedString()

    var tipParts: [String] = []
    if snapshot.isAppRunning, let app = snapshot.appAccount {
      var lines = ["🖥️ Codex Desktop App (\(app.email)):"]
      let sprintReset = app.sprintTimeUntilResetString
      let sprintResetStr = (!sprintReset.isEmpty && sprintReset != L10n.resetNow) ? " (resets: \(sprintReset))" : ""
      lines.append("  • 5h Sprint: \(String(format: "%.0f%%", app.fiveHourPercentage))\(sprintResetStr)")
      if let w = app.weeklyPercentage {
        let wReset = app.weeklyTimeUntilResetString
        let wResetStr = (!wReset.isEmpty && wReset != L10n.resetNow) ? " (resets: \(wReset))" : ""
        lines.append("  • Weekly Limit: \(String(format: "%.0f%%", w))\(wResetStr)")
      }
      if app.credits > 0 { lines.append("  • Reset Credits: \(app.credits)") }
      tipParts.append(lines.joined(separator: "\n"))
    }

    if let cli = snapshot.cliAccount ?? snapshot.accounts.first(where: { $0.isCurrentActive }) {
      var lines = ["💻 Codex CLI (\(cli.email)):"]
      let sprintReset = cli.sprintTimeUntilResetString
      let sprintResetStr = (!sprintReset.isEmpty && sprintReset != L10n.resetNow) ? " (resets: \(sprintReset))" : ""
      lines.append("  • 5h Sprint: \(String(format: "%.0f%%", cli.fiveHourPercentage))\(sprintResetStr)")
      if let w = cli.weeklyPercentage {
        let wReset = cli.weeklyTimeUntilResetString
        let wResetStr = (!wReset.isEmpty && wReset != L10n.resetNow) ? " (resets: \(wReset))" : ""
        lines.append("  • Weekly Limit: \(String(format: "%.0f%%", w))\(wResetStr)")
      }
      if cli.credits > 0 { lines.append("  • Reset Credits: \(cli.credits)") }
      tipParts.append(lines.joined(separator: "\n"))
    }

    button.toolTip = tipParts.isEmpty ? "OpenAI Codex Quota Monitor" : tipParts.joined(separator: "\n\n")
  }

  public static func buildStatusBarAttributedString(
    icon: NSImage?,
    appSession: (fiveHPct: String, fiveHColor: NSColor, weeklyPct: String, weeklyColor: NSColor)? = nil,
    cliSession: (fiveHPct: String, fiveHColor: NSColor, weeklyPct: String, weeklyColor: NSColor),
    accounts: [AccountQuota],
    isScreenActive: Bool = true,
    useQuotaIcons: Bool = true,
    stackPercentages: Bool = true
  ) -> NSAttributedString {
    let attributed = NSMutableAttributedString()
    if let icon = icon {
      let attachment = NSTextAttachment()
      attachment.image = icon
      let iconSize = icon.size.width > 0 ? icon.size : NSSize(width: 22, height: 22)
      let yOffset = iconSize.height >= 22.0 ? -7.0 : (iconSize.height >= 20.0 ? -6.5 : -6.0)
      attachment.bounds = CGRect(x: 0, y: yOffset, width: iconSize.width, height: iconSize.height)
      attributed.append(NSAttributedString(attachment: attachment))
      attributed.append(NSAttributedString(string: "  "))
    }

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
      (tag: String, tagColor: NSColor, fiveHPct: String, fiveHColor: NSColor, weeklyPct: String, weeklyColor: NSColor) in
      attributed.append(NSAttributedString(string: tag, attributes: [
        .font: NSFont.systemFont(ofSize: 10, weight: .heavy), .foregroundColor: tagColor,
        .shadow: textShadow, .kern: 0.2, .baselineOffset: 0.5
      ]))
      if stackPercentages {
        let attach = MenuBarAppearanceHelper.makeStackedValuesAttachment(
          fiveHPct: fiveHPct, fiveHColor: fiveHColor, weeklyPct: weeklyPct, weeklyColor: weeklyColor,
          isScreenActive: isScreenActive, useQuotaIcons: useQuotaIcons
        )
        attributed.append(NSAttributedString(attachment: attach))
      } else {
        if useQuotaIcons {
          let sprintIcon = MenuBarAppearanceHelper.makeSprintIcon(size: 8.0, isScreenActive: isScreenActive)
          let attach = NSTextAttachment()
          attach.image = sprintIcon
          let sprintW = sprintIcon.size.height > 0 ? sprintIcon.size.width * (8.0 / sprintIcon.size.height) : 8.0
          attach.bounds = CGRect(x: 0, y: -0.5, width: sprintW, height: 8.0)
          attributed.append(NSAttributedString(attachment: attach))
          attributed.append(NSAttributedString(string: " ", attributes: [.font: NSFont.systemFont(ofSize: 2.5)]))
        } else {
          attributed.append(NSAttributedString(string: "5h: ", attributes: [
            .font: labelFont, .foregroundColor: NSColor.white, .shadow: textShadow, .kern: 0.2, .baselineOffset: 0.0
          ]))
        }
        attributed.append(NSAttributedString(string: fiveHPct, attributes: [
          .font: numberFont, .foregroundColor: fiveHColor, .shadow: textShadow, .kern: kernValue, .baselineOffset: 0.0
        ]))
        attributed.append(NSAttributedString(string: " ", attributes: [.font: NSFont.systemFont(ofSize: 4), .baselineOffset: 0.0]))
        if useQuotaIcons {
          let weeklyIcon = MenuBarAppearanceHelper.makeWeeklyIcon(size: 9.5, isScreenActive: isScreenActive)
          let attach = NSTextAttachment()
          attach.image = weeklyIcon
          attach.bounds = CGRect(x: 0, y: -0.5, width: 9.5, height: 9.5)
          attributed.append(NSAttributedString(attachment: attach))
          attributed.append(NSAttributedString(string: " ", attributes: [.font: NSFont.systemFont(ofSize: 2.5)]))
        } else {
          attributed.append(NSAttributedString(string: "Wk: ", attributes: [
            .font: labelFont, .foregroundColor: NSColor.white, .shadow: textShadow, .kern: 0.2, .baselineOffset: 0.0
          ]))
        }
        attributed.append(NSAttributedString(string: weeklyPct, attributes: [
          .font: numberFont, .foregroundColor: weeklyColor, .shadow: textShadow, .kern: kernValue, .baselineOffset: 0.0
        ]))
      }
    }

    let appTagColor = isScreenActive ? NSColor(red: 0.35, green: 0.85, blue: 1.0, alpha: 1.0) : NSColor(red: 0.30, green: 0.75, blue: 0.90, alpha: 1.0)
    let cliTagColor = isScreenActive ? NSColor(red: 0.65, green: 0.95, blue: 0.65, alpha: 1.0) : NSColor(red: 0.55, green: 0.82, blue: 0.55, alpha: 1.0)

    if let app = appSession {
      appendSession("APP ", appTagColor, app.fiveHPct, app.fiveHColor, app.weeklyPct, app.weeklyColor)
      attributed.append(NSAttributedString(string: " │ ", attributes: [
        .font: sepFont, .foregroundColor: sepColor, .shadow: textShadow, .baselineOffset: 0.0
      ]))
      appendSession("CLI ", cliTagColor, cliSession.fiveHPct, cliSession.fiveHColor, cliSession.weeklyPct, cliSession.weeklyColor)
    } else {
      appendSession("CLI ", cliTagColor, cliSession.fiveHPct, cliSession.fiveHColor, cliSession.weeklyPct, cliSession.weeklyColor)
    }

    let bracketOffset = MenuBarAppearanceHelper.bracketBaselineOffset(isScreenActive: isScreenActive)
    if accounts.isEmpty {
      attributed.append(NSAttributedString(string: "  [", attributes: [.font: bracketFont, .foregroundColor: bracketColor, .shadow: bracketShadow, .baselineOffset: bracketOffset]))
      attributed.append(NSAttributedString(string: " ", attributes: [.font: NSFont.systemFont(ofSize: 2.5)]))
      let badge = MenuBarAppearanceHelper.makeHybridQuotaIndicator(fiveHour: 100.0, weekly: 100.0, credits: 0, width: 6.5, height: 16.5, isScreenActive: isScreenActive)
      let attach = NSTextAttachment(); attach.image = badge; attach.bounds = CGRect(x: 0, y: -5.0, width: 6.5, height: 16.5)
      attributed.append(NSAttributedString(attachment: attach))
      attributed.append(NSAttributedString(string: " ", attributes: [.font: NSFont.systemFont(ofSize: 2.5)]))
      attributed.append(NSAttributedString(string: "]", attributes: [.font: bracketFont, .foregroundColor: bracketColor, .shadow: bracketShadow, .baselineOffset: bracketOffset]))
    } else {
      let activeAcc = accounts.first(where: { $0.isCurrentActive }) ?? accounts[0]
      let reserveAccs = accounts.filter { $0.id != activeAcc.id }
      attributed.append(NSAttributedString(string: "  [", attributes: [.font: bracketFont, .foregroundColor: bracketColor, .shadow: bracketShadow, .baselineOffset: bracketOffset]))
      attributed.append(NSAttributedString(string: " ", attributes: [.font: NSFont.systemFont(ofSize: 2.5)]))
      let activeBadge = MenuBarAppearanceHelper.makeHybridQuotaIndicator(
        fiveHour: activeAcc.fiveHourPercentage, weekly: activeAcc.weeklyPercentage ?? activeAcc.fiveHourPercentage,
        credits: activeAcc.credits, width: 6.5, height: 16.5, isScreenActive: isScreenActive
      )
      let activeAttach = NSTextAttachment(); activeAttach.image = activeBadge; activeAttach.bounds = CGRect(x: 0, y: -5.0, width: 6.5, height: 16.5)
      attributed.append(NSAttributedString(attachment: activeAttach))
      attributed.append(NSAttributedString(string: " ", attributes: [.font: NSFont.systemFont(ofSize: 2.5)]))
      attributed.append(NSAttributedString(string: "]", attributes: [.font: bracketFont, .foregroundColor: bracketColor, .shadow: bracketShadow, .baselineOffset: bracketOffset]))

      if !reserveAccs.isEmpty { attributed.append(NSAttributedString(string: " ", attributes: [.font: NSFont.systemFont(ofSize: 7.0)])) }
      for (idx, acc) in reserveAccs.enumerated() {
        if idx > 0 { attributed.append(NSAttributedString(string: " ", attributes: [.font: NSFont.systemFont(ofSize: 6.5)])) }
        let reserveBadge = MenuBarAppearanceHelper.makeHybridQuotaIndicator(
          fiveHour: acc.fiveHourPercentage, weekly: acc.weeklyPercentage ?? acc.fiveHourPercentage,
          credits: acc.credits, width: 6.5, height: 16.5, isScreenActive: isScreenActive
        )
        let attach = NSTextAttachment(); attach.image = reserveBadge; attach.bounds = CGRect(x: 0, y: -5.0, width: 6.5, height: 16.5)
        attributed.append(NSAttributedString(attachment: attach))
      }
    }
    return attributed
  }

  public static func renderCompositeImage(from attributedTitle: NSAttributedString) -> NSImage {
    let titleSize = attributedTitle.size()
    let compositeWidth = max(1.0, ceil(titleSize.width))
    let compositeHeight: CGFloat = 22.0
    let compositeImage = NSImage(size: NSSize(width: compositeWidth, height: compositeHeight), flipped: false) { rect in
      attributedTitle.draw(at: NSPoint(x: 0, y: 0.0))
      return true
    }
    compositeImage.isTemplate = false
    return compositeImage
  }

  internal func updateStatusBarDisplay(
    fiveHPct: String, fiveHColor: NSColor, weeklyPct: String, weeklyColor: NSColor, accounts: [AccountQuota], isScreenActive: Bool = true
  ) {
    guard let button = statusItem?.button else { return }
    let currentIcon = isScreenActive ? (menuBarIconActive ?? menuBarIcon) : (menuBarIconInactive ?? menuBarIcon)
    let stackPref = UserDefaults.standard.object(forKey: "stackPercentages") as? Bool ?? true
    let attributedTitle = AppDelegate.buildStatusBarAttributedString(
      icon: currentIcon, appSession: nil, cliSession: (fiveHPct: fiveHPct, fiveHColor: fiveHColor, weeklyPct: weeklyPct, weeklyColor: weeklyColor),
      accounts: accounts, isScreenActive: isScreenActive, useQuotaIcons: true, stackPercentages: stackPref
    )
    let compositeImage = AppDelegate.renderCompositeImage(from: attributedTitle)
    button.image = compositeImage
    button.imagePosition = .imageOnly
    button.attributedTitle = NSAttributedString()
  }
}
