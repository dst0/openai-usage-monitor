import AppKit
import Foundation

extension AppDelegate {
  // MARK: - Account Management Actions

  @objc internal func addAccountAction() {
    let alert = NSAlert()
    alert.messageText = L10n.addAccountTitle
    alert.informativeText = L10n.addAccountMsg
    alert.alertStyle = .informational

    let existingIds = Set((lastSnapshot?.accounts ?? []).map { $0.id })
    let suggestedId: String
    if let activeEmail = lastSnapshot?.activeEmail ?? client.loadCachedSnapshot()?.activeEmail,
      !activeEmail.isEmpty
    {
      let rawPrefix = activeEmail.components(separatedBy: "@").first ?? "account"
      let prefix = rawPrefix.replacingOccurrences(of: " ", with: "-")
      if !existingIds.contains(prefix) {
        suggestedId = prefix
      } else {
        var counter = 2
        while existingIds.contains("\(prefix)-\(counter)") {
          counter += 1
        }
        suggestedId = "\(prefix)-\(counter)"
      }
    } else {
      let count = (lastSnapshot?.accounts.count ?? 0) + 1
      suggestedId = "account-\(count)"
    }

    let inputField = NSTextField(frame: NSRect(x: 0, y: 0, width: 260, height: 24))
    inputField.stringValue = suggestedId
    inputField.placeholderString = suggestedId
    alert.accessoryView = inputField

    alert.addButton(withTitle: L10n.addAccountLoginBrowserBtn)
    alert.addButton(withTitle: L10n.addAccountSaveCurrentBtn)
    alert.addButton(withTitle: L10n.cancelBtn)

    NSApp.activate(ignoringOtherApps: true)
    inputField.selectText(nil)
    let response = alert.runModal()
    guard response != .alertThirdButtonReturn else { return }

    let rawId = inputField.stringValue.trimmingCharacters(in: .whitespacesAndNewlines)
    let cleanId = rawId.replacingOccurrences(of: " ", with: "-")
    let finalId = cleanId.isEmpty ? suggestedId : cleanId

    if response == .alertFirstButtonReturn {
      client.addNewAccount(id: finalId) { [weak self] success, errMsg in
        if success {
          self?.refreshNow()
          self?.showAlert(title: L10n.addAccountTitle, message: L10n.addAccountSuccess(id: finalId))
        } else {
          let desc = (errMsg?.isEmpty == false) ? errMsg! : L10n.addAccountFailedDesc
          self?.showAlert(title: L10n.addAccountFailedTitle, message: desc, style: .warning)
        }
      }
    } else if response == .alertSecondButtonReturn {
      client.saveCurrentSession(id: finalId) { [weak self] success, errMsg in
        if success {
          self?.refreshNow()
          self?.showAlert(
            title: L10n.addAccountTitle, message: L10n.addAccountSavedSuccess(id: finalId))
        } else {
          let desc = (errMsg?.isEmpty == false) ? errMsg! : L10n.addAccountFailedDesc
          self?.showAlert(title: L10n.addAccountFailedTitle, message: desc, style: .warning)
        }
      }
    }
  }

  internal func showAlert(title: String, message: String, style: NSAlert.Style = .informational) {
    let alert = NSAlert()
    alert.messageText = title
    alert.informativeText = message
    alert.alertStyle = style
    alert.addButton(withTitle: "OK")
    NSApp.activate(ignoringOtherApps: true)
    alert.runModal()
  }

  internal func confirmAndRemoveAccount(id: String, email: String) {
    let alert = NSAlert()
    alert.messageText = L10n.removeAccountTitle
    alert.informativeText = L10n.removeAccountConfirm(email: email)
    alert.alertStyle = .warning
    alert.addButton(withTitle: L10n.removeConfirmBtn)
    alert.addButton(withTitle: L10n.cancelBtn)

    NSApp.activate(ignoringOtherApps: true)
    let response = alert.runModal()
    if response == .alertFirstButtonReturn {
      client.removeAccount(id: id) { [weak self] _ in
        self?.refreshNow()
      }
    }
  }

  internal func confirmAndResetAccount(id: String, email: String) {
    statusItem?.menu?.cancelTracking()
    let alert = NSAlert()
    alert.messageText = L10n.resetAccountTitle
    alert.informativeText = L10n.resetAccountConfirm(email: email)
    alert.alertStyle = .warning
    alert.addButton(withTitle: L10n.resetConfirmBtn)
    alert.addButton(withTitle: L10n.cancelBtn)

    NSApp.activate(ignoringOtherApps: true)
    let response = alert.runModal()
    if response == .alertFirstButtonReturn {
      client.resetAccount(id: id) { [weak self] success, errMsg in
        self?.refreshNow()
        if !success, let msg = errMsg, !msg.isEmpty {
          self?.showAlert(title: L10n.resetAccountTitle, message: msg, style: .warning)
        } else if success {
          self?.showAlert(
            title: L10n.resetAccountTitle,
            message: L10n.resetSuccessMsg(email: email),
            style: .informational
          )
        }
      }
    }
  }

  @objc internal func handleActiveAccountRelogin(_ sender: NSMenuItem) {
    if let account = sender.representedObject as? AccountQuota {
      promptReloginAccount(id: account.id, email: account.email)
    } else if let activeAcc = lastSnapshot?.accounts.first(where: { $0.id == (lastSnapshot?.activeAccountId ?? "") }) {
      promptReloginAccount(id: activeAcc.id, email: activeAcc.email)
    }
  }

  internal func promptReloginAccount(id: String, email: String) {
    let alert = NSAlert()
    alert.messageText = L10n.reloginAccountTitle
    alert.informativeText = L10n.reloginAccountMsg(email: email)
    alert.alertStyle = .informational
    alert.addButton(withTitle: L10n.reloginToAccount)
    alert.addButton(withTitle: L10n.cancelBtn)

    NSApp.activate(ignoringOtherApps: true)
    let response = alert.runModal()
    guard response == .alertFirstButtonReturn else { return }

    executeRelogin(id: id, email: email)
  }

  internal func executeRelogin(id: String, email: String) {
    client.reloginAccount(id: id) { [weak self] success, errMsg in
      DispatchQueue.main.async {
        if success {
          self?.refreshNow()
          self?.showAlert(
            title: L10n.reloginSuccessTitle,
            message: L10n.reloginSuccessMsg(email: email)
          )
        } else {
          let desc = (errMsg?.isEmpty == false) ? errMsg! : L10n.reloginFailedDesc
          self?.showAlert(title: L10n.reloginFailedTitle, message: desc, style: .warning)
        }
      }
    }
  }

  internal func promptRenameAccount(id: String, currentName: String?, email: String) {
    let alert = NSAlert()
    alert.messageText = L10n.renameAccountTitle
    alert.informativeText = L10n.renameAccountPrompt(email: email)
    alert.alertStyle = .informational

    let input = NSTextField(frame: NSRect(x: 0, y: 0, width: 260, height: 24))
    input.stringValue = currentName ?? ""
    input.placeholderString = email.components(separatedBy: "@").first ?? "nickname"
    alert.accessoryView = input

    alert.addButton(withTitle: L10n.saveBtn)
    alert.addButton(withTitle: L10n.clearBtn)
    alert.addButton(withTitle: L10n.cancelBtn)

    NSApp.activate(ignoringOtherApps: true)
    let response = alert.runModal()
    if response == .alertFirstButtonReturn {
      let newName = input.stringValue.trimmingCharacters(in: .whitespacesAndNewlines)
      client.renameAccount(id: id, newName: newName.isEmpty ? nil : newName) {
        [weak self] success, err in
        if !success, let err = err {
          self?.showAlert(title: L10n.renameAccountTitle, message: err, style: .warning)
        }
        self?.refreshNow()
      }
    } else if response == .alertSecondButtonReturn {
      client.renameAccount(id: id, newName: nil) { [weak self] _, _ in
        self?.refreshNow()
      }
    }
  }

  internal func executeSwitchAccount(id: String) {
    let target = lastSnapshot?.accounts.first(where: {
      $0.id.caseInsensitiveCompare(id) == .orderedSame
        || $0.email.caseInsensitiveCompare(id) == .orderedSame
        || ($0.name?.caseInsensitiveCompare(id) == .orderedSame)
    })
    if let target = target, target.needsRelogin {
      promptReloginAccount(id: target.id, email: target.email)
      return
    }
    if let currentSnapshot = self.lastSnapshot {
      let updatedAccounts = currentSnapshot.accounts.map { acc in
        AccountQuota(
          id: acc.id,
          name: acc.name,
          email: acc.email,
          planType: acc.planType,
          isCurrentActive: acc.id.caseInsensitiveCompare(id) == .orderedSame,
          fiveHourPercentage: acc.fiveHourPercentage,
          weeklyPercentage: acc.weeklyPercentage,
          models: acc.models,
          resetTime: acc.resetTime,
          resetAfterSeconds: acc.resetAfterSeconds,
          credits: acc.credits,
          error: acc.error
        )
      }
      let targetAcc = updatedAccounts.first(where: { $0.isCurrentActive }) ?? updatedAccounts.first
      let updatedSnapshot = MultiAccountSnapshot(
        timestamp: Date(),
        activeAccountId: targetAcc?.id ?? id,
        activeEmail: targetAcc?.email ?? currentSnapshot.activeEmail,
        activePlan: targetAcc?.planType ?? currentSnapshot.activePlan,
        fiveHourPercentage: targetAcc?.fiveHourPercentage ?? currentSnapshot.fiveHourPercentage,
        weeklyPercentage: targetAcc?.weeklyPercentage ?? currentSnapshot.weeklyPercentage,
        resetTime: targetAcc?.resetTime ?? currentSnapshot.resetTime,
        resetAfterSeconds: targetAcc?.resetAfterSeconds ?? currentSnapshot.resetAfterSeconds,
        credits: targetAcc?.credits ?? currentSnapshot.credits,
        autoSwitchEnabled: currentSnapshot.autoSwitchEnabled,
        autoSwitchBusinessOnly: currentSnapshot.autoSwitchBusinessOnly,
        autoSwitchBusinessPriority: currentSnapshot.autoSwitchBusinessPriority,
        isAppRunning: currentSnapshot.isAppRunning,
        activeModelName: currentSnapshot.activeModelName,
        accounts: updatedAccounts,
        appAccount: currentSnapshot.appAccount,
        cliAccount: targetAcc
      )
      self.lastSnapshot = updatedSnapshot
      self.updateUI(with: updatedSnapshot)
    }
    client.switchToAccount(id: id) { [weak self] _ in
      self?.refreshNow()
    }
  }

  @objc internal func switchModelAction(_ sender: NSMenuItem) {
    guard let model = sender.representedObject as? String else { return }
    client.setActiveModelName(model)
    refreshNow()
  }
}
