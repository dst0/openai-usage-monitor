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
    stackPercentages: Bool = true,
    appAccountId: String? = nil,
    cliAccountId: String? = nil
  ) -> NSAttributedString {
    return buildStatusBarAttributedString(
      icon: icon,
      appSession: appSession,
      cliSession: cliSession,
      accounts: accounts,
      isScreenActive: isScreenActive,
      useQuotaIcons: useModelIcons,
      stackPercentages: stackPercentages,
      appAccountId: appAccountId,
      cliAccountId: cliAccountId
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
}
