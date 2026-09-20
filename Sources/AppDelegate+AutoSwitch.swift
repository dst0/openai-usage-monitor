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

  @objc internal func autoDistributeAccountsAction() {
    client.autoDistributeAccounts { [weak self] _ in
      self?.refreshNow()
    }
  }
}
