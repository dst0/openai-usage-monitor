import AppKit
import Foundation

extension AppDelegate {
  // MARK: - Reserve Account Sections & Cards

  internal func buildReserveSections(
    snapshot: MultiAccountSnapshot,
    into menu: NSMenu,
    insertIdx: inout Int
  ) {
    let cliPrimary =
      snapshot.cliAccount ?? snapshot.accounts.first(where: { $0.isCurrentActive })
      ?? snapshot.accounts.first
    let reserveAccs = snapshot.accounts.filter { $0.id != (cliPrimary?.id ?? "") }
    guard !reserveAccs.isEmpty else { return }

    let isRu = LocalizationManager.shared.currentLanguage == .ru
    let sepReserves = AppDelegate.makeInsetSeparatorItem()
    menu.insertItem(sepReserves, at: insertIdx)
    dynamicAccountItems.append(sepReserves)
    insertIdx += 1

    let reservesHeaderTitle =
      isRu ? "Резервные аккаунты (CLI пул)" : "Reserve Accounts (CLI Pool)"
    let resHeader = AppDelegate.makePrimarySectionHeaderItem(
      title: reservesHeaderTitle, symbolName: "person.3.sequence.fill")
    menu.insertItem(resHeader, at: insertIdx)
    dynamicAccountItems.append(resHeader)
    insertIdx += 1

    var orgGroups: [String: [AccountQuota]] = [:]
    var orgOrder: [String] = []
    var personalAccs: [AccountQuota] = []

    for acc in reserveAccs {
      if acc.isBusiness {
        let orgName = acc.effectiveOrganizationName ?? (isRu ? "Бизнес" : "Business")
        if orgGroups[orgName] == nil {
          orgGroups[orgName] = []
          orgOrder.append(orgName)
        }
        orgGroups[orgName]?.append(acc)
      } else {
        personalAccs.append(acc)
      }
    }

    var globalReserveIdx = 0

    func sectionEntries(for accounts: [AccountQuota]) -> [ReserveAccountSectionEntry] {
      accounts.map { account in
        globalReserveIdx += 1
        return ReserveAccountSectionEntry(account: account, reserveIndex: globalReserveIdx)
      }
    }

    func insertAccountSection(
      title: String,
      kind: AccountSectionHeaderView.Kind,
      accounts: [AccountQuota]
    ) {
      let entries = sectionEntries(for: accounts)
      let cardItem = AppDelegate.makeAccountSectionCardItem(
        title: title,
        kind: kind,
        entries: entries,
        onSwitch: { [weak self] accountId in
          self?.executeSwitchAccount(id: accountId)
        },
        onDelete: { [weak self] accountId, email in
          self?.confirmAndRemoveAccount(id: accountId, email: email)
        },
        onRename: { [weak self] accountId, name, email in
          self?.promptRenameAccount(id: accountId, currentName: name, email: email)
        },
        onReset: { [weak self] accountId, email in
          self?.confirmAndResetAccount(id: accountId, email: email)
        },
        onRelogin: { [weak self] accountId, email in
          self?.promptReloginAccount(id: accountId, email: email)
        }
      )
      menu.insertItem(cardItem, at: insertIdx)
      dynamicAccountItems.append(cardItem)
      insertIdx += 1
    }

    for orgName in orgOrder {
      let groupAccs = orgGroups[orgName] ?? []
      insertAccountSection(title: orgName, kind: .business, accounts: groupAccs)
    }

    if !personalAccs.isEmpty {
      insertAccountSection(
        title: L10n.personalAccounts, kind: .personal, accounts: personalAccs)
    }
  }
}
