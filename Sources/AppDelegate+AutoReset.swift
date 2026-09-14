import AppKit
import Foundation

extension AppDelegate {
  // MARK: - Auto-Reset Weekly Policies & Dialogs

  @objc internal func toggleAutoResetWeekly(_ sender: NSMenuItem) {
    let current = client.getAutoResetWeeklyConfiguration()
    let enabling = !current.enabled
    if enabling {
      let alert = NSAlert()
      alert.messageText = L10n.autoResetWeeklyEnableTitle
      alert.informativeText = L10n.autoResetWeeklyEnableMessage
      alert.alertStyle = .warning
      alert.addButton(withTitle: L10n.autoResetWeeklyEnableButton)
      alert.addButton(withTitle: L10n.cancelBtn)
      NSApp.activate(ignoringOtherApps: true)
      guard alert.runModal() == .alertFirstButtonReturn else { return }
    }
    sender.state = enabling ? .on : .off
    client.setAutoResetWeekly(enabled: enabling, minRemainingHours: current.minRemainingHours) {
      [weak self, weak sender] success in
      guard let self = self else { return }
      if !success {
        sender?.state = current.enabled ? .on : .off
        self.showAlert(
          title: L10n.autoResetWeekly,
          message: "Could not save the automatic weekly reset setting.",
          style: .warning
        )
      }
      self.refreshNow()
    }
  }

  @objc internal func changeAutoResetWeeklyThreshold(_ sender: NSMenuItem) {
    if sender.tag == -1 {
      promptAutoResetWeeklyCustomThreshold()
      return
    }
    applyAutoResetWeeklyThreshold(hours: sender.tag)
  }

  private func promptAutoResetWeeklyCustomThreshold() {
    let current = client.getAutoResetWeeklyConfiguration()
    let alert = NSAlert()
    alert.messageText = L10n.autoResetWeeklyThreshold
    alert.informativeText =
      "Enter whole hours from 1 to 167. A credit is used only when more than this time remains before the normal weekly reset."
    alert.alertStyle = .informational
    let input = NSTextField(frame: NSRect(x: 0, y: 0, width: 220, height: 24))
    input.stringValue = String(max(1, current.minRemainingHours))
    alert.accessoryView = input
    alert.addButton(withTitle: L10n.saveBtn)
    alert.addButton(withTitle: L10n.cancelBtn)
    NSApp.activate(ignoringOtherApps: true)
    guard alert.runModal() == .alertFirstButtonReturn,
      let hours = Int(input.stringValue.trimmingCharacters(in: .whitespacesAndNewlines)),
      (1...167).contains(hours)
    else { return }
    applyAutoResetWeeklyThreshold(hours: hours)
  }

  private func applyAutoResetWeeklyThreshold(hours: Int) {
    let current = client.getAutoResetWeeklyConfiguration()
    let safeHours = min(167, max(0, hours))
    for item in autoResetWeeklyThresholdItems {
      item.state = item.tag == safeHours ? .on : .off
    }
    autoResetWeeklyCustomThresholdItem?.state =
      autoResetWeeklyThresholdItems.contains(where: { $0.tag == safeHours }) ? .off : .on
    autoResetWeeklyCustomThresholdItem?.title =
      autoResetWeeklyThresholdItems.contains(where: { $0.tag == safeHours })
      ? L10n.autoResetWeeklyCustom
      : L10n.autoResetWeeklyCustomValue(hours: safeHours)
    client.setAutoResetWeekly(enabled: current.enabled, minRemainingHours: safeHours) { [weak self] success in
      guard let self = self else { return }
      if !success {
        self.showAlert(
          title: L10n.autoResetWeekly,
          message: "Could not save the weekly reset threshold.",
          style: .warning
        )
      }
      self.refreshNow()
    }
  }

  internal func autoResetMenuStatus(_ state: String) -> String {
    let russian = LocalizationManager.shared.currentLanguage == .ru
    switch state {
    case "disabled": return russian ? "выключено" : "disabled"
    case "ready": return russian ? "готово" : "ready"
    case "waiting_for_task": return russian ? "ожидание заблокированной задачи" : "waiting for a blocked task"
    case "waiting_for_window": return russian ? "ожидание выбранного окна" : "waiting for the selected window"
    case "no_credit": return russian ? "нет кредитов сброса" : "no reset credit"
    case "waiting_for_desktop": return russian ? "ожидание Codex Desktop" : "waiting for Codex Desktop"
    case "waiting_for_service": return russian ? "сервис сброса временно недоступен" : "reset service temporarily unavailable"
    case "waiting_for_fresh_quota": return russian ? "ожидание свежей квоты" : "waiting for fresh quota"
    case "waiting_for_other_automation": return russian ? "ожидание другой автоматизации" : "waiting for other automation"
    case "waiting_for_original_task": return russian ? "ожидание исходной заблокированной задачи" : "waiting for the original blocked task"
    case "unsupported": return russian ? "переход на сервисный reset" : "migrating to service reset"
    case "applied": return russian ? "кредит применён; ожидается обновление" : "credit applied; awaiting refresh"
    case "unknown": return russian ? "результат неизвестен; повтор с тем же ключом" : "outcome unknown; retrying safely"
    case "journal_error": return russian ? "ошибка журнала — остановлено безопасно" : "journal error — stopped safely"
    default: return state.replacingOccurrences(of: "_", with: " ")
    }
  }
}
