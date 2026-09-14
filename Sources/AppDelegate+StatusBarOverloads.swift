import AppKit
import Foundation

extension AppDelegate {
  // MARK: - Compatibility & Convenience Overloads

  /// Overload with useModelIcons argument label for cross-project compatibility.
  public static func buildStatusBarAttributedString(
    icon: NSImage?,
    appSession: (fiveHPct: String, fiveHColor: NSColor, weeklyPct: String, weeklyColor: NSColor)? = nil,
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
        appSession.map { ($0.pct, $0.color, $0.pct, $0.color) }
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
}
