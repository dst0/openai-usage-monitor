import Foundation

/// Represents a reserve account entry tagged with its display pool index.
public struct ReserveAccountSectionEntry {
  public let account: AccountQuota
  public let reserveIndex: Int

  public init(account: AccountQuota, reserveIndex: Int) {
    self.account = account
    self.reserveIndex = reserveIndex
  }
}
