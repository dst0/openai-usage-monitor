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
  /// switching, so they show on only while it is on. Rows stay disabled while a write is pending.
  internal func syncAutoSwitchSettingsMarks() {
    let settings = client.getAutoSwitchSettings()
    autoSwitchItem?.state = settings.autoSwitchEnabled ? .on : .off
    autoSwitchBusinessPriorityItem?.state =
      (settings.autoSwitchEnabled && settings.businessPriority) ? .on : .off
    autoSwitchBusinessOnlyItem?.state =
      (settings.autoSwitchEnabled && settings.businessOnly) ? .on : .off
    restartAppOnSwitchItem?.state = settings.restartAppOnSwitch ? .on : .off
    preserveWindowBoundsItem?.state = settings.preserveWindowBoundsOnRestart ? .on : .off
    let idle = pendingAutoSwitchSettingWrites == 0
    for row in [
      autoSwitchItem, autoSwitchBusinessPriorityItem, autoSwitchBusinessOnlyItem,
      restartAppOnSwitchItem, preserveWindowBoundsItem,
    ] {
      row?.isEnabled = idle
    }
  }

  // MARK: - Auto-Switch Policy & Toggles

  @objc internal func toggleAutoSwitchOnLimit(_ sender: NSMenuItem) {
    requestAutoSwitchSetting(sender, L10n.autoSwitchOnLimit, client.setAutoSwitchEnabled)
  }

  @objc internal func toggleAutoSwitchBusinessOnly(_ sender: NSMenuItem) {
    requestAutoSwitchSetting(sender, L10n.autoSwitchBusinessOnly, client.setAutoSwitchBusinessOnly)
  }

  @objc internal func toggleAutoSwitchBusinessPriority(_ sender: NSMenuItem) {
    requestAutoSwitchSetting(
      sender, L10n.autoSwitchBusinessPriority, client.setAutoSwitchBusinessPriority)
  }

  @objc internal func toggleRestartAppOnSwitch(_ sender: NSMenuItem) {
    requestAutoSwitchSetting(sender, L10n.restartAppOnSwitch, client.setRestartAppOnSwitch)
  }

  @objc internal func togglePreserveWindowBounds(_ sender: NSMenuItem) {
    requestAutoSwitchSetting(
      sender, L10n.preserveWindowBoundsOnRestart, client.setPreserveWindowBoundsOnRestart)
  }

  /// Asks `write` for the opposite of `row`'s mark and leaves the mark alone: choosing an item
  /// closes the menu, and the mark then shows what the Rust core saved, read back when the
  /// write finishes. Every row stays disabled until then, so a second click cannot repeat a
  /// request the mark does not show yet. A write whose read-back does not show the request
  /// raises a warning; the CLI's exit status is not the test, because it can fail after the
  /// registry is saved (syncing the status cache) and the daemon reads the registry.
  private func requestAutoSwitchSetting(
    _ row: NSMenuItem, _ title: String, _ write: (Bool, ((Bool) -> Void)?) -> Void
  ) {
    let requested: NSControl.StateValue = row.state == .on ? .off : .on
    pendingAutoSwitchSettingWrites += 1
    syncAutoSwitchSettingsMarks()
    write(requested == .on, { [weak self, weak row] _ in
      guard let self = self else { return }
      self.pendingAutoSwitchSettingWrites -= 1
      self.syncAutoSwitchSettingsMarks()
      if let row = row, row.state != requested {
        self.showAlertOutsideMenuTracking(title: title, message: L10n.settingSaveFailed)
      }
    })
  }

  @objc internal func autoDistributeAccountsAction() {
    client.autoDistributeAccounts { [weak self] _ in
      self?.refreshNow()
    }
  }
}
