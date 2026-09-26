import AppKit
import Foundation

extension AppDelegate {
  // MARK: - Status Bar Display Construction

  public typealias StatusBarSessionValues = (fiveHPct: String, fiveHColor: NSColor, weeklyPct: String, weeklyColor: NSColor)

  public static func resolveCliAccount(from snapshot: MultiAccountSnapshot) -> AccountQuota? {
    snapshot.cliAccount ?? snapshot.accounts.first(where: { $0.isCurrentActive })
  }

  public static func makeSessionValues(
    fiveHourPercentage: Double, weeklyPercentage: Double?, planMultiplier: Double, isScreenActive: Bool = true
  ) -> StatusBarSessionValues {
    let w = weeklyPercentage ?? fiveHourPercentage
    let exhausted = MenuBarAppearanceHelper.isWeeklyExhausted(weeklyPercentage)
    let fiveHColor = MenuBarAppearanceHelper.menuBarColor(
      forPercentage: fiveHourPercentage, weeklyPercentage: weeklyPercentage, isScreenActive: isScreenActive, planMultiplier: planMultiplier
    )
    let wColor = MenuBarAppearanceHelper.menuBarColor(
      forPercentage: w, weeklyPercentage: nil, isScreenActive: isScreenActive, planMultiplier: planMultiplier
    )
    return (
      fiveHPct: exhausted ? "0%" : String(format: "%.0f%%", fiveHourPercentage),
      fiveHColor: fiveHColor, weeklyPct: String(format: "%.0f%%", w), weeklyColor: wColor
    )
  }

  public static func resolveStatusBarSessions(
    from snapshot: MultiAccountSnapshot, isScreenActive: Bool = true
  ) -> (appSession: StatusBarSessionValues?, cliSession: StatusBarSessionValues) {
    let appSession: StatusBarSessionValues?
    if !snapshot.isAppRunning {
      appSession = nil
    } else if let app = snapshot.appAccount {
      appSession = makeSessionValues(
        fiveHourPercentage: app.fiveHourPercentage, weeklyPercentage: app.weeklyPercentage,
        planMultiplier: app.planMultiplier, isScreenActive: isScreenActive)
    } else {
      appSession = (fiveHPct: "—", fiveHColor: .secondaryLabelColor,
        weeklyPct: "—", weeklyColor: .secondaryLabelColor)
    }
    let cli = resolveCliAccount(from: snapshot)
    let cliSession = cli.map {
      makeSessionValues(fiveHourPercentage: $0.fiveHourPercentage, weeklyPercentage: $0.weeklyPercentage, planMultiplier: $0.planMultiplier, isScreenActive: isScreenActive)
    } ?? makeSessionValues(fiveHourPercentage: snapshot.fiveHourPercentage, weeklyPercentage: snapshot.weeklyPercentage, planMultiplier: snapshot.planMultiplier, isScreenActive: isScreenActive)
    return (appSession, cliSession)
  }

  public static func buildStatusBarAttributedString(
    snapshot: MultiAccountSnapshot, icon: NSImage? = nil, isScreenActive: Bool = true, useQuotaIcons: Bool = true, stackPercentages: Bool = true
  ) -> NSAttributedString {
    let sessions = resolveStatusBarSessions(from: snapshot, isScreenActive: isScreenActive)
    return buildStatusBarAttributedString(
      icon: icon, appSession: sessions.appSession, cliSession: sessions.cliSession, accounts: snapshot.accounts,
      isScreenActive: isScreenActive, useQuotaIcons: useQuotaIcons, stackPercentages: stackPercentages,
      appAccountId: snapshot.isAppRunning ? snapshot.appAccount?.id : nil, cliAccountId: resolveCliAccount(from: snapshot)?.id
    )
  }

  internal func updateStatusBar(with snapshot: MultiAccountSnapshot) {
    guard let button = statusItem?.button else { return }
    let isScreenActive = true
    let currentIcon = isScreenActive ? (menuBarIconActive ?? menuBarIcon) : (menuBarIconInactive ?? menuBarIcon)
    let stackPercentages = UserDefaults.standard.object(forKey: "stackPercentages") as? Bool ?? true
    let attributedTitle = AppDelegate.buildStatusBarAttributedString(
      snapshot: snapshot, icon: currentIcon, isScreenActive: isScreenActive, useQuotaIcons: true, stackPercentages: stackPercentages
    )
    button.image = AppDelegate.renderCompositeImage(from: attributedTitle)
    button.imagePosition = .imageOnly
    button.attributedTitle = NSAttributedString()

    func tooltipLines(title: String, acc: AccountQuota) -> String {
      let sReset = acc.sprintTimeUntilResetString
      let sStr = (!sReset.isEmpty && sReset != L10n.resetNow) ? " (resets: \(sReset))" : ""
      var lines = [title, "  • 5h Sprint: \(String(format: "%.0f%%", acc.fiveHourPercentage))\(sStr)"]
      if let w = acc.weeklyPercentage {
        let wReset = acc.weeklyTimeUntilResetString
        let wStr = (!wReset.isEmpty && wReset != L10n.resetNow) ? " (resets: \(wReset))" : ""
        lines.append("  • Weekly Limit: \(String(format: "%.0f%%", w))\(wStr)")
      }
      if acc.credits > 0 { lines.append("  • Reset Credits: \(acc.credits)") }
      return lines.joined(separator: "\n")
    }

    var tipParts: [String] = []
    if snapshot.isAppRunning, let app = snapshot.appAccount {
      tipParts.append(tooltipLines(title: "🖥️ Codex Desktop App (\(app.email)):", acc: app))
    }
    if let cli = AppDelegate.resolveCliAccount(from: snapshot) {
      tipParts.append(tooltipLines(title: "💻 Codex CLI (\(cli.email)):", acc: cli))
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
    stackPercentages: Bool = true,
    appAccountId: String? = nil,
    cliAccountId: String? = nil
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
    let textShadow = MenuBarAppearanceHelper.textShadow(isScreenActive: isScreenActive)
    let sepColor = MenuBarAppearanceHelper.separatorColor(isScreenActive: isScreenActive)
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

    let appTagColor = MenuBarAppearanceHelper.appTagColor(isScreenActive: isScreenActive)
    let cliTagColor = MenuBarAppearanceHelper.cliTagColor(isScreenActive: isScreenActive)

    if let app = appSession {
      appendSession("APP ", appTagColor, app.fiveHPct, app.fiveHColor, app.weeklyPct, app.weeklyColor)
      attributed.append(NSAttributedString(string: " │ ", attributes: [
        .font: sepFont, .foregroundColor: sepColor, .shadow: textShadow, .baselineOffset: 0.0
      ]))
      appendSession("CLI ", cliTagColor, cliSession.fiveHPct, cliSession.fiveHColor, cliSession.weeklyPct, cliSession.weeklyColor)
    } else {
      appendSession("CLI ", cliTagColor, cliSession.fiveHPct, cliSession.fiveHColor, cliSession.weeklyPct, cliSession.weeklyColor)
    }

    let defaultCli = accounts.first(where: { $0.isCurrentActive })?.id ?? accounts.first?.id
    let rawCliId = (appAccountId != nil || cliAccountId != nil) ? cliAccountId : defaultCli
    let rawAppId = (appAccountId != nil || cliAccountId != nil) ? appAccountId : ((appSession != nil) ? defaultCli : nil)

    func resolveId(_ id: String?) -> String? {
      guard let id = id else { return nil }
      return accounts.first(where: {
        $0.id.caseInsensitiveCompare(id) == .orderedSame
          || $0.email.caseInsensitiveCompare(id) == .orderedSame
          || ($0.name?.caseInsensitiveCompare(id) == .orderedSame)
      })?.id ?? id
    }
    let effectiveCliId = resolveId(rawCliId)
    let effectiveAppId = resolveId(rawAppId)

    if accounts.isEmpty {
      let mode: BracketSelectionMode = (appAccountId != nil || cliAccountId != nil)
        ? ((appAccountId != nil && cliAccountId != nil) ? .both : (appAccountId != nil ? .app : .cli))
        : .cli

      if mode != .none {
        MenuBarAppearanceHelper.appendBracket(to: attributed, bracket: "[", mode: mode, isScreenActive: isScreenActive)
      }
      let badge = MenuBarAppearanceHelper.makeHybridQuotaIndicator(
        fiveHour: 100.0, weekly: 100.0, credits: 0, width: 6.5, height: 16.5, isScreenActive: isScreenActive
      )
      let attach = NSTextAttachment(); attach.image = badge; attach.bounds = CGRect(x: 0, y: -5.0, width: 6.5, height: 16.5)
      attributed.append(NSAttributedString(attachment: attach))
      if mode != .none {
        MenuBarAppearanceHelper.appendBracket(to: attributed, bracket: "]", mode: mode, isScreenActive: isScreenActive)
      }
    } else {
      for (idx, acc) in accounts.enumerated() {
        let isApp = (effectiveAppId != nil && acc.id.caseInsensitiveCompare(effectiveAppId!) == .orderedSame)
        let isCli = (effectiveCliId != nil && acc.id.caseInsensitiveCompare(effectiveCliId!) == .orderedSame)
        let mode: BracketSelectionMode = (isApp && isCli) ? .both : (isApp ? .app : (isCli ? .cli : .none))

        if idx == 0 {
          attributed.append(NSAttributedString(string: "  "))
        } else {
          let prevAcc = accounts[idx - 1]
          let prevIsApp = (effectiveAppId != nil && prevAcc.id == effectiveAppId)
          let prevIsCli = (effectiveCliId != nil && prevAcc.id == effectiveCliId)
          let prevHadBrackets = prevIsApp || prevIsCli
          let currentHasBrackets = (mode != .none)

          let spaceSize: CGFloat
          if prevHadBrackets && currentHasBrackets {
            spaceSize = 4.0
          } else if prevHadBrackets || currentHasBrackets {
            spaceSize = 7.0
          } else {
            spaceSize = 6.5
          }
          attributed.append(NSAttributedString(string: " ", attributes: [.font: NSFont.systemFont(ofSize: spaceSize)]))
        }

        if mode != .none {
          MenuBarAppearanceHelper.appendBracket(to: attributed, bracket: "[", mode: mode, isScreenActive: isScreenActive)
        }

        let badge = MenuBarAppearanceHelper.makeHybridQuotaIndicator(
          fiveHour: acc.fiveHourPercentage,
          weekly: acc.weeklyPercentage ?? acc.fiveHourPercentage,
          credits: acc.credits,
          width: 6.5,
          height: 16.5,
          isScreenActive: isScreenActive
        )
        let attach = NSTextAttachment()
        attach.image = badge
        attach.bounds = CGRect(x: 0, y: -5.0, width: 6.5, height: 16.5)
        attributed.append(NSAttributedString(attachment: attach))

        if mode != .none {
          MenuBarAppearanceHelper.appendBracket(to: attributed, bracket: "]", mode: mode, isScreenActive: isScreenActive)
        }
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
