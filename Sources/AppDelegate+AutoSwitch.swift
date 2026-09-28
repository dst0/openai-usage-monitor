import AppKit
import Foundation

extension AppDelegate {
  // MARK: - Auto-Switch Settings Submenu

  /// "⚙️ Auto-Switch Settings": one top-level item whose submenu holds every switch-time
  /// setting as a checkmark, the way the AGY monitor groups them.
  internal func makeAutoSwitchSettingsItem() -> NSMenuItem {
    let container = NSMenuItem(title: L10n.autoSwitchSettings, action: nil, keyEquivalent: "")
    let submenu = NSMenu(title: L10n.autoSwitchSettings)
    submenu.autoenablesItems = false
    func addToggle(_ title: String, _ action: Selector) -> NSMenuItem {
      let item = NSMenuItem(title: title, action: action, keyEquivalent: "")
      item.target = self
      submenu.addItem(item)
      return item
    }
    autoSwitchItem = addToggle(L10n.autoSwitchOnLimit, #selector(toggleAutoSwitchOnLimit(_:)))
    autoSwitchBusinessPriorityItem = addToggle(
      L10n.autoSwitchBusinessPriority, #selector(toggleAutoSwitchBusinessPriority(_:)))
    autoSwitchBusinessOnlyItem = addToggle(
      L10n.autoSwitchBusinessOnly, #selector(toggleAutoSwitchBusinessOnly(_:)))
    restartAppOnSwitchItem = addToggle(
      L10n.restartAppOnSwitch, #selector(toggleRestartAppOnSwitch(_:)))
    preserveWindowBoundsItem = addToggle(
      L10n.preserveWindowBoundsOnRestart, #selector(togglePreserveWindowBounds(_:)))
    container.submenu = submenu
    autoSwitchSettingsItem = container
    syncAutoSwitchSettingsMarks()
    return container
  }

  /// Sets every mark from one read of the registry, which only the Rust core writes, so a
  /// change made with `cxi config` shows too. The business modes only shape automatic
  /// switching, so they show on only while it is on.
  internal func syncAutoSwitchSettingsMarks() {
    let settings = client.getAutoSwitchSettings()
    autoSwitchItem?.state = settings.autoSwitchEnabled ? .on : .off
    autoSwitchBusinessPriorityItem?.state =
      (settings.autoSwitchEnabled && settings.businessPriority) ? .on : .off
    autoSwitchBusinessOnlyItem?.state =
      (settings.autoSwitchEnabled && settings.businessOnly) ? .on : .off
    restartAppOnSwitchItem?.state = settings.restartAppOnSwitch ? .on : .off
    preserveWindowBoundsItem?.state = settings.preserveWindowBoundsOnRestart ? .on : .off
  }

  // MARK: - Auto-Switch Policy & Toggles

  // Each toggle asks for the opposite of its mark and leaves the mark alone: choosing an item
  // closes the menu, and the mark then shows what the Rust core saved (which may change other
  // settings too), read back when the write finishes and again whenever the menu opens.

  @objc internal func toggleAutoSwitchOnLimit(_ sender: NSMenuItem) {
    client.setAutoSwitchEnabled(
      sender.state != .on, completion: autoSwitchSettingSaved(L10n.autoSwitchOnLimit))
  }

  @objc internal func toggleAutoSwitchBusinessOnly(_ sender: NSMenuItem) {
    client.setAutoSwitchBusinessOnly(
      sender.state != .on, completion: autoSwitchSettingSaved(L10n.autoSwitchBusinessOnly))
  }

  @objc internal func toggleAutoSwitchBusinessPriority(_ sender: NSMenuItem) {
    client.setAutoSwitchBusinessPriority(
      sender.state != .on, completion: autoSwitchSettingSaved(L10n.autoSwitchBusinessPriority))
  }

  @objc internal func toggleRestartAppOnSwitch(_ sender: NSMenuItem) {
    client.setRestartAppOnSwitch(
      sender.state != .on, completion: autoSwitchSettingSaved(L10n.restartAppOnSwitch))
  }

  @objc internal func togglePreserveWindowBounds(_ sender: NSMenuItem) {
    client.setPreserveWindowBoundsOnRestart(
      sender.state != .on, completion: autoSwitchSettingSaved(L10n.preserveWindowBoundsOnRestart))
  }

  /// A setter's completion: the marks show what the registry now holds, and a write that
  /// failed (a missing CLI, say) says so instead of leaving a click that changed nothing.
  private func autoSwitchSettingSaved(_ title: String) -> (Bool) -> Void {
    return { [weak self] success in
      guard let self = self else { return }
      self.syncAutoSwitchSettingsMarks()
      if !success {
        self.showAlert(title: title, message: L10n.settingSaveFailed, style: .warning)
      }
    }
  }

  @objc internal func autoDistributeAccountsAction() {
    client.autoDistributeAccounts { [weak self] _ in
      self?.refreshNow()
    }
  }
}
