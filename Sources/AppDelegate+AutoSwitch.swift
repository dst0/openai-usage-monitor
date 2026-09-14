import AppKit
import Foundation

extension AppDelegate {
  // MARK: - Auto-Switch Policy & Toggles

  @objc internal func toggleAutoSwitchOnLimit(_ sender: NSMenuItem) {
    let newState = sender.state != .on
    sender.state = newState ? .on : .off
    client.setAutoSwitchEnabled(newState)
    if !newState {
      autoSwitchBusinessOnlyItem?.state = .off
      autoSwitchBusinessPriorityItem?.state = .off
    } else {
      let bizOnly = client.getAutoSwitchBusinessOnly()
      let bizPriority = client.getAutoSwitchBusinessPriority()
      autoSwitchBusinessOnlyItem?.state = bizOnly ? .on : .off
      autoSwitchBusinessPriorityItem?.state = bizPriority ? .on : .off
    }
  }

  @objc internal func toggleAutoSwitchBusinessOnly(_ sender: NSMenuItem) {
    let newState = sender.state != .on
    sender.state = newState ? .on : .off
    if newState {
      autoSwitchBusinessPriorityItem?.state = .off
      autoSwitchItem?.state = .on
    }
    client.setAutoSwitchBusinessOnly(newState)
  }

  @objc internal func toggleAutoSwitchBusinessPriority(_ sender: NSMenuItem) {
    let newState = sender.state != .on
    sender.state = newState ? .on : .off
    if newState {
      autoSwitchBusinessOnlyItem?.state = .off
      autoSwitchItem?.state = .on
    }
    client.setAutoSwitchBusinessPriority(newState)
  }

  public static func determineAutoSwitchTarget(
    activeAcc: AccountQuota?,
    accounts: [AccountQuota],
    autoSwitchEnabled: Bool,
    businessOnly: Bool,
    businessPriority: Bool
  ) -> AccountQuota? {
    guard autoSwitchEnabled, let activeAcc = activeAcc else { return nil }
    let isDepleted = activeAcc.fiveHourPercentage <= 0.0 || activeAcc.needsRelogin
      || (activeAcc.error?.localizedCaseInsensitiveContains("429") == true)
      || (activeAcc.error?.localizedCaseInsensitiveContains("limit") == true)
    let shouldPreemptForBusiness = businessPriority && !activeAcc.isBusiness
      && accounts.contains { acc in
        acc.id != activeAcc.id && acc.isBusiness && acc.fiveHourPercentage > 0.0 && !acc.needsRelogin
          && (acc.error == nil || acc.error?.isEmpty == true)
      }
    guard isDepleted || shouldPreemptForBusiness else { return nil }

    var candidates = accounts.filter { acc in
      acc.id != activeAcc.id && acc.fiveHourPercentage > 0.0 && !acc.needsRelogin
        && (acc.error == nil || acc.error?.isEmpty == true)
    }
    if businessOnly || shouldPreemptForBusiness {
      candidates = candidates.filter { $0.isBusiness }
    }
    guard !candidates.isEmpty else { return nil }

    candidates.sort { a, b in
      if businessPriority && a.isBusiness != b.isBusiness { return a.isBusiness }
      if a.credits != b.credits { return a.credits > b.credits }
      let aReset = a.resetAfterSeconds ?? Int.max
      let bReset = b.resetAfterSeconds ?? Int.max
      if aReset != bReset { return aReset < bReset }
      return a.fiveHourPercentage > b.fiveHourPercentage
    }
    return candidates.first
  }
}
