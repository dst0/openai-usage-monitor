import AppKit
import Foundation

/// Horizontal action strip hosting independent CLI and APP account switch affordances.
public final class AccountSwitchButtonsView: NSView {
  public let accountId: String
  public let accountEmail: String
  public let isCliActive: Bool
  public let isAppActive: Bool

  public let cliButton: MenuIconButton
  public let appButton: MenuIconButton

  private let onSwitchCli: (String) -> Void
  private let onSwitchApp: (String) -> Void

  public init(
    frame: NSRect,
    accountId: String,
    accountEmail: String,
    isCliActive: Bool = false,
    isAppActive: Bool = false,
    onSwitchCli: @escaping (String) -> Void = { _ in },
    onSwitchApp: @escaping (String) -> Void = { _ in }
  ) {
    self.accountId = accountId
    self.accountEmail = accountEmail
    self.isCliActive = isCliActive
    self.isAppActive = isAppActive
    self.onSwitchCli = onSwitchCli
    self.onSwitchApp = onSwitchApp

    let btnHeight: CGFloat = 22
    let btnWidth: CGFloat = 92
    let spacing: CGFloat = 8

    let cliTitle = isCliActive ? "✓ CLI" : "> CLI"
    let cliSymbol = isCliActive ? "checkmark.circle.fill" : "terminal"
    let cliColor = MenuBarAppearanceHelper.cliTagColor()

    let cliBtn = MenuIconButton(
      frame: NSRect(x: 0, y: (frame.height - btnHeight) / 2, width: btnWidth, height: btnHeight),
      title: cliTitle,
      symbolName: cliSymbol,
      pointSize: 11,
      weight: .semibold,
      tintColor: isCliActive ? cliColor.withAlphaComponent(0.65) : cliColor,
      hoverTintColor: cliColor,
      isCapsule: true,
      tooltip: isCliActive
        ? "\(L10n.activeInCli): \(accountEmail)"
        : "\(L10n.switchCliTooltip): \(accountEmail)",
      accessibilityLabel: "\(L10n.switchCli): \(accountEmail)"
    )
    if cliBtn.image == nil { cliBtn.title = cliTitle }
    cliBtn.identifier = NSUserInterfaceItemIdentifier("cli:\(accountId)")
    cliBtn.managesOwnTracking = false
    self.cliButton = cliBtn

    let appTitle = isAppActive ? "✓ APP" : "🖥 APP"
    let appSymbol = isAppActive ? "checkmark.circle.fill" : "desktopcomputer"
    let appColor = MenuBarAppearanceHelper.appTagColor()

    let appBtn = MenuIconButton(
      frame: NSRect(
        x: btnWidth + spacing, y: (frame.height - btnHeight) / 2, width: btnWidth,
        height: btnHeight),
      title: appTitle,
      symbolName: appSymbol,
      pointSize: 11,
      weight: .semibold,
      tintColor: isAppActive ? appColor.withAlphaComponent(0.65) : appColor,
      hoverTintColor: appColor,
      isCapsule: true,
      tooltip: isAppActive
        ? "\(L10n.activeInApp): \(accountEmail)"
        : "\(L10n.switchAppTooltip): \(accountEmail)",
      accessibilityLabel: "\(L10n.switchApp): \(accountEmail)"
    )
    if appBtn.image == nil { appBtn.title = appTitle }
    appBtn.identifier = NSUserInterfaceItemIdentifier("app:\(accountId)")
    appBtn.managesOwnTracking = false
    self.appButton = appBtn

    super.init(frame: frame)
    autoresizingMask = [.width]

    cliBtn.target = self
    cliBtn.action = #selector(handleCliClicked)
    addSubview(cliBtn)

    appBtn.target = self
    appBtn.action = #selector(handleAppClicked)
    addSubview(appBtn)
  }

  public required init?(coder: NSCoder) {
    fatalError("init(coder:) has not been implemented")
  }

  @objc internal func handleCliClicked() {
    guard !isCliActive else { return }
    enclosingMenuItem?.menu?.cancelTracking()
    onSwitchCli(accountId)
  }

  @objc internal func handleAppClicked() {
    guard !isAppActive else { return }
    enclosingMenuItem?.menu?.cancelTracking()
    onSwitchApp(accountId)
  }

  public func button(at point: NSPoint) -> MenuIconButton? {
    if cliButton.frame.contains(point) { return cliButton }
    if appButton.frame.contains(point) { return appButton }
    return nil
  }
}
