import Foundation

/// Represents a reserve account entry tagged with its display pool index.
public struct ReserveAccountSectionEntry {
  public let account: AccountQuota
  public let reserveIndex: Int
  public let isCliActive: Bool
  public let isAppActive: Bool

  public init(
    account: AccountQuota,
    reserveIndex: Int,
    isCliActive: Bool = false,
    isAppActive: Bool = false
  ) {
    self.account = account
    self.reserveIndex = reserveIndex
    self.isCliActive = isCliActive
    self.isAppActive = isAppActive
  }
}
