import AppKit
import Foundation

// MARK: - Inset Separator View

public final class InsetSeparatorView: NSView {
    public let horizontalInset: CGFloat
    public let lineColor: NSColor

    public init(frame frameRect: NSRect, horizontalInset: CGFloat = 16, lineColor: NSColor = NSColor.separatorColor) {
        self.horizontalInset = horizontalInset
        self.lineColor = lineColor
        super.init(frame: frameRect)
        self.wantsLayer = true
    }

    public required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    public override func draw(_ dirtyRect: NSRect) {
        super.draw(dirtyRect)
        let rect = NSRect(
            x: horizontalInset,
            y: bounds.midY - 0.5,
            width: max(0, bounds.width - (horizontalInset * 2)),
            height: 1.0
        )
        lineColor.setFill()
        rect.fill()
    }
}

// MARK: - Account Row View with ✕ Delete Button

public final class AccountRowView: NSView {
    public let accountId: String
    public let accountName: String?
    public let accountEmail: String
    public let tier: String?
    public let isCurrentActive: Bool
    public let onDelete: (String, String) -> Void
    public let onRename: (String, String?, String) -> Void
    public let onSelect: (String) -> Void

    public let titleLabel: NSTextField
    public var switchButton: NSButton?
    public let deleteButton: NSButton

    public init(
        frame: NSRect,
        accountId: String,
        accountName: String?,
        email: String,
        tier: String?,
        isCurrentActive: Bool,
        dotColor: NSColor,
        statusTag: String,
        onDelete: @escaping (String, String) -> Void,
        onRename: @escaping (String, String?, String) -> Void,
        onSelect: @escaping (String) -> Void
    ) {
        self.accountId = accountId
        self.accountName = accountName
        self.accountEmail = email
        self.tier = tier
        self.isCurrentActive = isCurrentActive
        self.onDelete = onDelete
        self.onRename = onRename
        self.onSelect = onSelect

        let rightOffset: CGFloat = isCurrentActive ? 28 : 56
        let labelWidth = max(50, frame.width - rightOffset - 12)
        self.titleLabel = NSTextField(frame: NSRect(x: 12, y: 1, width: labelWidth, height: 20))
        self.deleteButton = NSButton(frame: NSRect(x: frame.width - 26, y: 2, width: 20, height: 18))

        if !isCurrentActive {
            let sb = NSButton(frame: NSRect(x: frame.width - 50, y: 2, width: 22, height: 18))
            sb.isBordered = false
            sb.title = "⇄"
            sb.font = NSFont.systemFont(ofSize: 13, weight: .bold)
            sb.contentTintColor = NSColor.systemBlue
            sb.toolTip = L10n.switchToAccount
            self.switchButton = sb
        } else {
            self.switchButton = nil
        }

        super.init(frame: frame)
        self.autoresizingMask = [.width]

        // Setup Title Label
        titleLabel.isBezeled = false
        titleLabel.drawsBackground = false
        titleLabel.isEditable = false
        titleLabel.isSelectable = false
        titleLabel.lineBreakMode = .byTruncatingMiddle

        let rich = NSMutableAttributedString()
        rich.append(NSAttributedString(string: "● ", attributes: [
            .font: NSFont.systemFont(ofSize: 13, weight: .bold),
            .foregroundColor: dotColor
        ]))
        let displayTitle: String
        let cleanName = accountName?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        let emailUsername = email.components(separatedBy: "@").first?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        if !cleanName.isEmpty
            && cleanName.caseInsensitiveCompare(email) != .orderedSame
            && cleanName.caseInsensitiveCompare(emailUsername) != .orderedSame {
            displayTitle = "\(cleanName) (\(email))"
        } else {
            displayTitle = email
        }
        rich.append(NSAttributedString(string: displayTitle, attributes: [
            .font: isCurrentActive ? NSFont.boldSystemFont(ofSize: 13) : NSFont.systemFont(ofSize: 13),
            .foregroundColor: NSColor.labelColor
        ]))
        let planLabel = tier ?? "Team"
        rich.append(NSAttributedString(string: "  \(statusTag)  \(planLabel)", attributes: [
            .font: NSFont.systemFont(ofSize: 11, weight: .semibold),
            .foregroundColor: isCurrentActive ? NSColor.systemGreen : NSColor.secondaryLabelColor
        ]))
        titleLabel.attributedStringValue = rich
        addSubview(titleLabel)

        // Setup Switch Button if reserve
        if let sb = switchButton {
            sb.target = self
            sb.action = #selector(handleSwitchClick)
            sb.autoresizingMask = [.minXMargin]
            addSubview(sb)
        }

        // Setup Delete Button (✕)
        deleteButton.isBordered = false
        deleteButton.title = "✕"
        deleteButton.font = NSFont.systemFont(ofSize: 12, weight: .bold)
        deleteButton.contentTintColor = NSColor.secondaryLabelColor
        deleteButton.toolTip = L10n.removeAccount
        deleteButton.target = self
        deleteButton.action = #selector(handleDelete)
        deleteButton.autoresizingMask = [.minXMargin]
        addSubview(deleteButton)
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    @objc private func handleDelete() {
        onDelete(accountId, accountEmail)
    }

    @objc private func handleSwitchClick() {
        enclosingMenuItem?.menu?.cancelTracking()
        onSelect(accountId)
    }

    public override func resetCursorRects() {
        super.resetCursorRects()
        if !isCurrentActive {
            addCursorRect(bounds, cursor: .pointingHand)
        }
    }

    public override func mouseUp(with event: NSEvent) {
        let point = convert(event.locationInWindow, from: nil)
        if deleteButton.frame.contains(point) {
            handleDelete()
            return
        }
        if let sb = switchButton, sb.frame.contains(point) {
            handleSwitchClick()
            return
        }
        if !isCurrentActive {
            enclosingMenuItem?.menu?.cancelTracking()
            onSelect(accountId)
        }
    }

    public override func menu(for event: NSEvent) -> NSMenu? {
        let ctxMenu = NSMenu()
        ctxMenu.autoenablesItems = false

        if !isCurrentActive {
            let switchItem = NSMenuItem(title: L10n.switchToAccount, action: #selector(handleSwitchFromCtx), keyEquivalent: "")
            switchItem.target = self
            ctxMenu.addItem(switchItem)
            ctxMenu.addItem(NSMenuItem.separator())
        }

        let renameItem = NSMenuItem(title: L10n.renameAccount, action: #selector(handleRenameFromCtx), keyEquivalent: "")
        renameItem.target = self
        ctxMenu.addItem(renameItem)

        let removeItem = NSMenuItem(title: L10n.removeAccount, action: #selector(handleDeleteFromCtx), keyEquivalent: "")
        removeItem.target = self
        ctxMenu.addItem(removeItem)

        return ctxMenu
    }

    @objc private func handleSwitchFromCtx() {
        onSelect(accountId)
    }

    @objc private func handleRenameFromCtx() {
        onRename(accountId, accountName, accountEmail)
    }

    @objc private func handleDeleteFromCtx() {
        onDelete(accountId, accountEmail)
    }
}

// MARK: - Main Application Delegate

public final class AppDelegate: NSObject, NSApplicationDelegate, NSMenuDelegate {
    public static let refreshIntervalKey = "codex_refresh_interval"
    public static let defaultMenuWidth: CGFloat = 380

    private var statusItem: NSStatusItem!
    private var refreshTimer: Timer?
    private var lastSnapshot: MultiAccountSnapshot?
    private var menuBarIcon: NSImage?
    private var menuBarIconActive: NSImage?
    private var menuBarIconInactive: NSImage?
    private var lastKnownScreenActive: Bool = true

    private var fileWatcherSource: DispatchSourceFileSystemObject?
    private var fileWatcherFD: Int32 = -1
    private var authWatcherSource: DispatchSourceFileSystemObject?
    private var authWatcherFD: Int32 = -1

    private var statusUpdateWorkItem: DispatchWorkItem?
    private var statusRestartWorkItem: DispatchWorkItem?
    private var authRefreshWorkItem: DispatchWorkItem?
    private var authRestartWorkItem: DispatchWorkItem?

    private var detectedCLIVersion: String? = "0.1.0"
    private var isCLIUpdateAvailable: Bool = false
    private var availableCLIVersion: String? = nil

    private var refreshInterval: TimeInterval = {
        let saved = UserDefaults.standard.double(forKey: AppDelegate.refreshIntervalKey)
        return saved > 0 ? saved : 60.0 // 1 minute default
    }()

    private let client = CodexClient.shared
    private let singleGuard = SingleInstanceGuard()
    private let autoLaunchManager = AutoLaunchManager.shared

    private var accountsSeparatorTop: NSMenuItem?
    private var accountsSeparatorBottom: NSMenuItem?
    private var dynamicAccountItems: [NSMenuItem] = []
    private var lastUpdatedMenuItem: NSMenuItem?
    private var updateCLIItem: NSMenuItem?
    private var launchAtLoginItem: NSMenuItem?
    private var stackPercentagesItem: NSMenuItem?
    private var autoSwitchItem: NSMenuItem?
    private var lastAutoSwitchTime: Date?
    private var isAutoSwitching = false
    private var isRefreshing: Bool = false
    private var refreshPendingWhileBusy: Bool = false

    private static let timeOfDayFormatter: DateFormatter = {
        let fmt = DateFormatter()
        fmt.dateFormat = "HH:mm:ss"
        return fmt
    }()

    // MARK: - Lifecycle

    public func applicationDidFinishLaunching(_ notification: Notification) {
        if !singleGuard.tryAcquire() || SingleInstanceGuard.isAnotherInstanceRunning() {
            print("Another instance of Codex Monitor is already running.")
            NSApp.terminate(nil)
            return
        }

        loadMenuBarIcon()

        statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
        statusItem.button?.imagePosition = .imageLeading

        buildMenu()
        setupScreenObservers()
        startStatusFileWatcher()
        startAuthFileWatcher()
        refreshCLIVersion()

        // Initial snapshot from cache
        if let snap = client.loadCachedSnapshot() {
            self.lastSnapshot = snap
            updateUI(with: snap)
        } else {
            // Placeholder status bar display while initial load happens
            updateStatusBarDisplay(
                fiveHPct: "100%",
                fiveHColor: MenuBarAppearanceHelper.menuBarColor(forPercentage: 100.0, isScreenActive: true),
                weeklyPct: "100%",
                weeklyColor: MenuBarAppearanceHelper.menuBarColor(forPercentage: 100.0, isScreenActive: true),
                accounts: [],
                isScreenActive: true
            )
        }

        // Live refresh
        refreshNow()
        startTimer()
    }

    public func applicationWillTerminate(_ notification: Notification) {
        singleGuard.release()
        stopStatusFileWatcher()
        stopAuthFileWatcher()
        refreshTimer?.invalidate()
    }

    // MARK: - Menu Bar Icon Loading & Contrast Boosting

    private func loadMenuBarIcon() {
        let bundle = Bundle.main
        if let image = bundle.image(forResource: "statusbar_icon") {
            menuBarIcon = image
        } else if let iconPath = bundle.path(forResource: "statusbar_icon", ofType: "png") {
            menuBarIcon = NSImage(contentsOfFile: iconPath)
        }

        if menuBarIcon == nil {
            let searchDirs: [URL] = [
                bundle.resourceURL,
                URL(fileURLWithPath: ProcessInfo.processInfo.arguments[0]).deletingLastPathComponent().appendingPathComponent("../Resources"),
                FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent("dev/openai-usage-monitor/resources")
            ].compactMap { $0 }

            for dir in searchDirs {
                let p1x = dir.appendingPathComponent("statusbar_icon.png")
                let p2x = dir.appendingPathComponent("statusbar_icon@2x.png")
                let p3x = dir.appendingPathComponent("statusbar_icon@3x.png")

                if FileManager.default.fileExists(atPath: p1x.path) {
                    let image = NSImage(size: NSSize(width: 22, height: 22))
                    if let rep3 = NSImageRep(contentsOf: p3x) { image.addRepresentation(rep3) }
                    if let rep2 = NSImageRep(contentsOf: p2x) { image.addRepresentation(rep2) }
                    if let rep1 = NSImageRep(contentsOf: p1x) { image.addRepresentation(rep1) }
                    if !image.representations.isEmpty {
                        menuBarIcon = image
                        break
                    }
                }
            }
        }

        if menuBarIcon == nil {
            let lightIcon = URL(fileURLWithPath: "/Applications/ChatGPT.app/Contents/Resources/icon-codex-light.png")
            if FileManager.default.fileExists(atPath: lightIcon.path) {
                menuBarIcon = NSImage(contentsOf: lightIcon)
            } else {
                let darkIcon = URL(fileURLWithPath: "/Applications/ChatGPT.app/Contents/Resources/icon-codex-dark-color.png")
                menuBarIcon = NSImage(contentsOf: darkIcon)
            }
        }

        if let icon = menuBarIcon {
            icon.size = NSSize(width: 22, height: 22)
            icon.isTemplate = false
            menuBarIconActive = icon
            menuBarIconInactive = icon
        }
    }

    // MARK: - Screen Parameter Observers

    private func setupScreenObservers() {
        let dCenter = NotificationCenter.default
        dCenter.addObserver(self, selector: #selector(handleScreenParametersChanged), name: NSApplication.didChangeScreenParametersNotification, object: nil)
    }

    private func isCurrentScreenActive() -> Bool {
        return true
    }

    @objc private func handleScreenParametersChanged() {
        DispatchQueue.main.async { [weak self] in
            guard let self = self else { return }
            if let snap = self.lastSnapshot {
                self.updateStatusBar(with: snap)
            }
        }
    }

    // MARK: - Status Bar Display Construction

    private func updateStatusBar(with snapshot: MultiAccountSnapshot) {
        let isScreenActive = true

        let cliMult = snapshot.cliAccount?.planMultiplier ?? snapshot.planMultiplier
        let cli5h = snapshot.fiveHourPercentage
        let cliW = snapshot.weeklyPercentage ?? cli5h
        let cli5hStr = String(format: "%.0f%%", cli5h)
        let cliWStr = String(format: "%.0f%%", cliW)
        let cli5hColor = MenuBarAppearanceHelper.menuBarColor(
            forPercentage: cli5h,
            weeklyPercentage: snapshot.weeklyPercentage,
            isScreenActive: isScreenActive,
            planMultiplier: cliMult
        )
        let cliWColor = MenuBarAppearanceHelper.menuBarColor(
            forPercentage: cliW,
            weeklyPercentage: nil,
            isScreenActive: isScreenActive,
            planMultiplier: cliMult
        )

        let cliSession = (
            fiveHPct: cli5hStr,
            fiveHColor: cli5hColor,
            weeklyPct: cliWStr,
            weeklyColor: cliWColor
        )

        var appSession: (fiveHPct: String, fiveHColor: NSColor, weeklyPct: String, weeklyColor: NSColor)? = nil
        if snapshot.isAppRunning, let app = snapshot.appAccount {
            let appMult = app.planMultiplier
            let app5h = app.fiveHourPercentage
            let appW = app.weeklyPercentage ?? app5h
            let app5hStr = String(format: "%.0f%%", app5h)
            let appWStr = String(format: "%.0f%%", appW)
            let app5hColor = MenuBarAppearanceHelper.menuBarColor(
                forPercentage: app5h,
                weeklyPercentage: app.weeklyPercentage,
                isScreenActive: isScreenActive,
                planMultiplier: appMult
            )
            let appWColor = MenuBarAppearanceHelper.menuBarColor(
                forPercentage: appW,
                weeklyPercentage: nil,
                isScreenActive: isScreenActive,
                planMultiplier: appMult
            )
            appSession = (
                fiveHPct: app5hStr,
                fiveHColor: app5hColor,
                weeklyPct: appWStr,
                weeklyColor: appWColor
            )
        }

        guard let button = statusItem?.button else { return }
        let currentIcon = isScreenActive ? (menuBarIconActive ?? menuBarIcon) : (menuBarIconInactive ?? menuBarIcon)
        let stackPercentages = UserDefaults.standard.object(forKey: "stackPercentages") as? Bool ?? true

        // NEVER set button.image = currentIcon because button.attributedTitle with NSTextAttachment is 100% reliable!
        button.image = nil
        button.attributedTitle = AppDelegate.buildStatusBarAttributedString(
            icon: currentIcon,
            appSession: appSession,
            cliSession: cliSession,
            accounts: snapshot.accounts,
            isScreenActive: isScreenActive,
            useQuotaIcons: true,
            stackPercentages: stackPercentages
        )

        // Detailed Tooltip
        var tipParts: [String] = []
        if snapshot.isAppRunning, let app = snapshot.appAccount {
            var lines = ["🖥️ Codex Desktop App (\(app.email)):"]
            lines.append("  • 5h Sprint: \(String(format: "%.0f%%", app.fiveHourPercentage)) (resets: \(app.timeUntilResetString))")
            if let w = app.weeklyPercentage {
                lines.append("  • Weekly Limit: \(String(format: "%.0f%%", w))")
            }
            if app.credits > 0 {
                lines.append("  • Reset Credits: \(app.credits)")
            }
            tipParts.append(lines.joined(separator: "\n"))
        }

        if let cli = snapshot.cliAccount ?? snapshot.accounts.first(where: { $0.isCurrentActive }) {
            var lines = ["💻 Codex CLI (\(cli.email)):"]
            lines.append("  • 5h Sprint: \(String(format: "%.0f%%", cli.fiveHourPercentage)) (resets: \(cli.timeUntilResetString))")
            if let w = cli.weeklyPercentage {
                lines.append("  • Weekly Limit: \(String(format: "%.0f%%", w))")
            }
            if cli.credits > 0 {
                lines.append("  • Reset Credits: \(cli.credits)")
            }
            tipParts.append(lines.joined(separator: "\n"))
        }

        button.toolTip = tipParts.isEmpty ? "OpenAI Codex Quota Monitor" : tipParts.joined(separator: "\n\n")
    }

    public static func buildStatusBarAttributedString(
        icon: NSImage?,
        appSession: (fiveHPct: String, fiveHColor: NSColor, weeklyPct: String, weeklyColor: NSColor)? = nil,
        cliSession: (fiveHPct: String, fiveHColor: NSColor, weeklyPct: String, weeklyColor: NSColor),
        accounts: [AccountQuota],
        isScreenActive: Bool = true,
        useQuotaIcons: Bool = true,
        stackPercentages: Bool = true
    ) -> NSAttributedString {
        let attributed = NSMutableAttributedString()

        // 1. [Codex Blue Logo] via NSTextAttachment
        if let icon = icon {
            let attachment = NSTextAttachment()
            attachment.image = icon
            let iconSize = icon.size.width > 0 ? icon.size : NSSize(width: 22, height: 22)
            let yOffset = iconSize.height >= 22.0 ? -6.0 : (iconSize.height >= 20.0 ? -5.5 : -5.0)
            attachment.bounds = CGRect(x: 0, y: yOffset, width: iconSize.width, height: iconSize.height)
            attributed.append(NSAttributedString(attachment: attachment))
            attributed.append(NSAttributedString(string: "  "))
        }

        // Fonts & Typography
        let numberFont = MenuBarAppearanceHelper.numberFont(isScreenActive: isScreenActive)
        let labelFont = MenuBarAppearanceHelper.labelFont(isScreenActive: isScreenActive)
        let sepFont = MenuBarAppearanceHelper.separatorFont(isScreenActive: isScreenActive)
        let bracketFont = MenuBarAppearanceHelper.bracketFont(isScreenActive: isScreenActive)
        let bracketShadow = MenuBarAppearanceHelper.bracketShadow(isScreenActive: isScreenActive)

        let textShadow = MenuBarAppearanceHelper.textShadow(isScreenActive: isScreenActive)
        let sepColor = MenuBarAppearanceHelper.separatorColor(isScreenActive: isScreenActive)
        let bracketColor = NSColor.white
        let kernValue: CGFloat = 0.3

        let appendSession = { (tag: String, tagColor: NSColor, fiveHPct: String, fiveHColor: NSColor, weeklyPct: String, weeklyColor: NSColor) in
            // Tag (e.g. "APP " or "CLI ")
            attributed.append(NSAttributedString(string: tag, attributes: [
                .font: NSFont.systemFont(ofSize: 10, weight: .heavy),
                .foregroundColor: tagColor,
                .shadow: textShadow,
                .kern: 0.2,
                .baselineOffset: 0.5
            ]))

            if stackPercentages {
                let attach = MenuBarAppearanceHelper.makeStackedValuesAttachment(
                    fiveHPct: fiveHPct,
                    fiveHColor: fiveHColor,
                    weeklyPct: weeklyPct,
                    weeklyColor: weeklyColor,
                    isScreenActive: isScreenActive,
                    useQuotaIcons: useQuotaIcons
                )
                attributed.append(NSAttributedString(attachment: attach))
            } else {
                // Horizontal layout: [⚡] 48%  [📅] 44%  or 5h: 48%  Wk: 44%
                if useQuotaIcons {
                    let sprintIcon = MenuBarAppearanceHelper.makeSprintIcon(size: 8.0, isScreenActive: isScreenActive)
                    let attach = NSTextAttachment()
                    attach.image = sprintIcon
                    attach.bounds = CGRect(x: 0, y: -0.5, width: 8.0, height: 8.0)
                    attributed.append(NSAttributedString(attachment: attach))
                    attributed.append(NSAttributedString(string: " ", attributes: [.font: NSFont.systemFont(ofSize: 2.5)]))
                } else {
                    attributed.append(NSAttributedString(string: "5h: ", attributes: [
                        .font: labelFont,
                        .foregroundColor: NSColor.white,
                        .shadow: textShadow,
                        .kern: 0.2,
                        .baselineOffset: 0.0
                    ]))
                }

                attributed.append(NSAttributedString(string: fiveHPct, attributes: [
                    .font: numberFont,
                    .foregroundColor: fiveHColor,
                    .shadow: textShadow,
                    .kern: kernValue,
                    .baselineOffset: 0.0
                ]))

                attributed.append(NSAttributedString(string: " ", attributes: [
                    .font: NSFont.systemFont(ofSize: 4),
                    .baselineOffset: 0.0
                ]))

                if useQuotaIcons {
                    let weeklyIcon = MenuBarAppearanceHelper.makeWeeklyIcon(size: 9.5, isScreenActive: isScreenActive)
                    let attach = NSTextAttachment()
                    attach.image = weeklyIcon
                    attach.bounds = CGRect(x: 0, y: -0.5, width: 9.5, height: 9.5)
                    attributed.append(NSAttributedString(attachment: attach))
                    attributed.append(NSAttributedString(string: " ", attributes: [.font: NSFont.systemFont(ofSize: 2.5)]))
                } else {
                    attributed.append(NSAttributedString(string: "Wk: ", attributes: [
                        .font: labelFont,
                        .foregroundColor: NSColor.white,
                        .shadow: textShadow,
                        .kern: 0.2,
                        .baselineOffset: 0.0
                    ]))
                }

                attributed.append(NSAttributedString(string: weeklyPct, attributes: [
                    .font: numberFont,
                    .foregroundColor: weeklyColor,
                    .shadow: textShadow,
                    .kern: kernValue,
                    .baselineOffset: 0.0
                ]))
            }
        }

        let appTagColor = isScreenActive
            ? NSColor(red: 0.35, green: 0.85, blue: 1.0, alpha: 1.0)
            : NSColor(red: 0.30, green: 0.75, blue: 0.90, alpha: 1.0)
        let cliTagColor = isScreenActive
            ? NSColor(red: 0.65, green: 0.95, blue: 0.65, alpha: 1.0)
            : NSColor(red: 0.55, green: 0.82, blue: 0.55, alpha: 1.0)

        if let app = appSession {
            appendSession("APP ", appTagColor, app.fiveHPct, app.fiveHColor, app.weeklyPct, app.weeklyColor)
            attributed.append(NSAttributedString(string: " │ ", attributes: [
                .font: sepFont,
                .foregroundColor: sepColor,
                .shadow: textShadow,
                .baselineOffset: 0.0
            ]))
            appendSession("CLI ", cliTagColor, cliSession.fiveHPct, cliSession.fiveHColor, cliSession.weeklyPct, cliSession.weeklyColor)
        } else {
            appendSession("CLI ", cliTagColor, cliSession.fiveHPct, cliSession.fiveHColor, cliSession.weeklyPct, cliSession.weeklyColor)
        }

        // 3D Hybrid Quota Indicators: [ 🛡️ ] 🛡️ 🛡️ (Active in brackets, reserves outside)
        if accounts.isEmpty {
            attributed.append(NSAttributedString(string: "  [", attributes: [
                .font: bracketFont,
                .foregroundColor: bracketColor,
                .shadow: bracketShadow,
                .baselineOffset: 0.0
            ]))
            let singleBadge = MenuBarAppearanceHelper.makeHybridQuotaIndicator(
                fiveHour: 100.0,
                weekly: 100.0,
                credits: 0,
                width: 13.0,
                height: 16.5,
                isScreenActive: isScreenActive
            )
            let attach = NSTextAttachment()
            attach.image = singleBadge
            attach.bounds = CGRect(x: 0, y: -3.5, width: 13.0, height: 16.5)
            attributed.append(NSAttributedString(attachment: attach))
            attributed.append(NSAttributedString(string: "]", attributes: [
                .font: bracketFont,
                .foregroundColor: bracketColor,
                .shadow: bracketShadow,
                .baselineOffset: 0.0
            ]))
        } else {
            let activeAcc = accounts.first(where: { $0.isCurrentActive }) ?? accounts[0]
            let reserveAccs = accounts.filter { $0.id != activeAcc.id }

            // Active account in brackets: [badge]
            attributed.append(NSAttributedString(string: "  [", attributes: [
                .font: bracketFont,
                .foregroundColor: bracketColor,
                .shadow: bracketShadow,
                .baselineOffset: 0.0
            ]))

            let f5h = activeAcc.fiveHourPercentage
            let w = activeAcc.weeklyPercentage ?? f5h
            let cr = activeAcc.credits

            let activeBadge = MenuBarAppearanceHelper.makeHybridQuotaIndicator(
                fiveHour: f5h,
                weekly: w,
                credits: cr,
                width: 13.0,
                height: 16.5,
                isScreenActive: isScreenActive
            )
            let activeAttach = NSTextAttachment()
            activeAttach.image = activeBadge
            activeAttach.bounds = CGRect(x: 0, y: -3.5, width: 13.0, height: 16.5)
            attributed.append(NSAttributedString(attachment: activeAttach))
            attributed.append(NSAttributedString(string: "]", attributes: [
                .font: bracketFont,
                .foregroundColor: bracketColor,
                .shadow: bracketShadow,
                .baselineOffset: 0.0
            ]))

            if !reserveAccs.isEmpty {
                attributed.append(NSAttributedString(string: " ", attributes: [.font: NSFont.systemFont(ofSize: 2.0)]))
            }

            // Reserve accounts outside brackets
            for (idx, acc) in reserveAccs.enumerated() {
                if idx > 0 {
                    attributed.append(NSAttributedString(string: " ", attributes: [.font: NSFont.systemFont(ofSize: 1.5)]))
                }
                let r5h = acc.fiveHourPercentage
                let rW = acc.weeklyPercentage ?? r5h
                let rCr = acc.credits

                let reserveBadge = MenuBarAppearanceHelper.makeHybridQuotaIndicator(
                    fiveHour: r5h,
                    weekly: rW,
                    credits: rCr,
                    width: 13.0,
                    height: 16.5,
                    isScreenActive: isScreenActive
                )
                let attach = NSTextAttachment()
                attach.image = reserveBadge
                attach.bounds = CGRect(x: 0, y: -3.5, width: 13.0, height: 16.5)
                attributed.append(NSAttributedString(attachment: attach))
            }
        }

        return attributed
    }

    /// Overload with useModelIcons argument label for cross-project compatibility.
    public static func buildStatusBarAttributedString(
        icon: NSImage?,
        appSession: (fiveHPct: String, fiveHColor: NSColor, weeklyPct: String, weeklyColor: NSColor)? = nil,
        cliSession: (fiveHPct: String, fiveHColor: NSColor, weeklyPct: String, weeklyColor: NSColor),
        accounts: [AccountQuota],
        isScreenActive: Bool = true,
        useModelIcons: Bool,
        stackPercentages: Bool = true
    ) -> NSAttributedString {
        return buildStatusBarAttributedString(
            icon: icon,
            appSession: appSession,
            cliSession: cliSession,
            accounts: accounts,
            isScreenActive: isScreenActive,
            useQuotaIcons: useModelIcons,
            stackPercentages: stackPercentages
        )
    }

    /// Single session convenience overload.
    public static func buildStatusBarAttributedString(
        icon: NSImage?,
        fiveHPct: String,
        fiveHColor: NSColor,
        weeklyPct: String,
        weeklyColor: NSColor,
        accounts: [AccountQuota] = [],
        isScreenActive: Bool = true,
        useQuotaIcons: Bool = true,
        stackPercentages: Bool = true
    ) -> NSAttributedString {
        return buildStatusBarAttributedString(
            icon: icon,
            appSession: nil,
            cliSession: (fiveHPct: fiveHPct, fiveHColor: fiveHColor, weeklyPct: weeklyPct, weeklyColor: weeklyColor),
            accounts: accounts,
            isScreenActive: isScreenActive,
            useQuotaIcons: useQuotaIcons,
            stackPercentages: stackPercentages
        )
    }

    /// Single session convenience overload with useModelIcons.
    public static func buildStatusBarAttributedString(
        icon: NSImage?,
        fiveHPct: String,
        fiveHColor: NSColor,
        weeklyPct: String,
        weeklyColor: NSColor,
        accounts: [AccountQuota] = [],
        isScreenActive: Bool = true,
        useModelIcons: Bool,
        stackPercentages: Bool = true
    ) -> NSAttributedString {
        return buildStatusBarAttributedString(
            icon: icon,
            appSession: nil,
            cliSession: (fiveHPct: fiveHPct, fiveHColor: fiveHColor, weeklyPct: weeklyPct, weeklyColor: weeklyColor),
            accounts: accounts,
            isScreenActive: isScreenActive,
            useQuotaIcons: useModelIcons,
            stackPercentages: stackPercentages
        )
    }

    /// Backward compatibility overload for previous 3-tuple session arguments.
    public static func buildStatusBarAttributedString(
        icon: NSImage?,
        appSession: (pct: String, color: NSColor, resetDesc: String)?,
        cliSession: (pct: String, color: NSColor, resetDesc: String),
        accounts: [AccountQuota],
        isScreenActive: Bool = true
    ) -> NSAttributedString {
        let appTuple: (fiveHPct: String, fiveHColor: NSColor, weeklyPct: String, weeklyColor: NSColor)? = appSession.map {
            ($0.pct, $0.color, $0.pct, $0.color)
        }
        let cliTuple = (cliSession.pct, cliSession.color, cliSession.pct, cliSession.color)
        let stackPref = UserDefaults.standard.object(forKey: "stackPercentages") as? Bool ?? true
        return buildStatusBarAttributedString(
            icon: icon,
            appSession: appTuple,
            cliSession: cliTuple,
            accounts: accounts,
            isScreenActive: isScreenActive,
            useQuotaIcons: true,
            stackPercentages: stackPref
        )
    }

    private func updateStatusBarDisplay(
        fiveHPct: String,
        fiveHColor: NSColor,
        weeklyPct: String,
        weeklyColor: NSColor,
        accounts: [AccountQuota],
        isScreenActive: Bool = true
    ) {
        guard let button = statusItem?.button else { return }
        let currentIcon = isScreenActive ? (menuBarIconActive ?? menuBarIcon) : (menuBarIconInactive ?? menuBarIcon)
        let stackPref = UserDefaults.standard.object(forKey: "stackPercentages") as? Bool ?? true
        button.image = nil
        button.attributedTitle = AppDelegate.buildStatusBarAttributedString(
            icon: currentIcon,
            appSession: nil,
            cliSession: (fiveHPct: fiveHPct, fiveHColor: fiveHColor, weeklyPct: weeklyPct, weeklyColor: weeklyColor),
            accounts: accounts,
            isScreenActive: isScreenActive,
            useQuotaIcons: true,
            stackPercentages: stackPref
        )
    }

    // MARK: - Menu Construction

    @discardableResult
    func buildMenu() -> NSMenu {
        let menu = NSMenu()
        menu.autoenablesItems = false

        let appVersion = Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String ?? "0.1.0"
        let fullTitle = VersionHelper.formatMenuTitle(baseTitle: L10n.menuTitle, version: appVersion)
        let titleItem = NSMenuItem(title: fullTitle, action: #selector(noop), keyEquivalent: "")
        titleItem.target = self
        titleItem.attributedTitle = NSAttributedString(string: fullTitle, attributes: [
            .font: NSFont.boldSystemFont(ofSize: 13),
            .foregroundColor: NSColor.labelColor
        ])
        menu.addItem(titleItem)

        // Legend Item with 3D Glossy Shield Sample
        let legendItem = NSMenuItem(title: "🛡️ " + L10n.legendCircles, action: #selector(noop), keyEquivalent: "")
        legendItem.target = self
        let legendAttr = NSMutableAttributedString()
        let sampleBadge = MenuBarAppearanceHelper.makeHybridQuotaIndicator(
            fiveHour: 100.0,
            weekly: 80.0,
            credits: 2,
            width: 13.0,
            height: 16.5,
            isScreenActive: true
        )
        let attach = NSTextAttachment()
        attach.image = sampleBadge
        attach.bounds = CGRect(x: 0, y: -3.5, width: 13.0, height: 16.5)
        legendAttr.append(NSAttributedString(attachment: attach))
        legendAttr.append(NSAttributedString(string: "  " + L10n.legendCircles, attributes: [
            .font: NSFont.systemFont(ofSize: 11),
            .foregroundColor: NSColor.secondaryLabelColor
        ]))
        legendItem.attributedTitle = legendAttr
        menu.addItem(legendItem)

        menu.addItem(NSMenuItem.separator())
        accountsSeparatorTop = menu.items.last!

        let accPlaceholder = NSMenuItem(title: "  " + L10n.tr("loading_accounts"), action: #selector(noop), keyEquivalent: "")
        accPlaceholder.target = self
        menu.addItem(accPlaceholder)
        dynamicAccountItems = [accPlaceholder]

        menu.addItem(NSMenuItem.separator())
        accountsSeparatorBottom = menu.items.last!

        lastUpdatedMenuItem = NSMenuItem(title: L10n.lastUpdated(time: "..."), action: #selector(noop), keyEquivalent: "")
        lastUpdatedMenuItem?.target = self
        menu.addItem(lastUpdatedMenuItem!)

        let refreshItem = NSMenuItem(title: L10n.refreshNow, action: #selector(refreshNow), keyEquivalent: "r")
        refreshItem.target = self
        menu.addItem(refreshItem)

        // Refresh Interval Submenu
        let intervalItem = NSMenuItem(title: L10n.refreshInterval, action: nil, keyEquivalent: "")
        let intervalSubmenu = NSMenu()
        intervalSubmenu.autoenablesItems = false
        for (label, seconds) in [
            (L10n.minutesShort(count: 1), 60.0),
            (L10n.minutesShort(count: 5), 300.0),
            (L10n.minutesShort(count: 15), 900.0),
            (L10n.minutesShort(count: 30), 1800.0)
        ] {
            let it = NSMenuItem(title: label, action: #selector(changeInterval(_:)), keyEquivalent: "")
            it.target = self
            it.tag = Int(seconds)
            if seconds == refreshInterval { it.state = .on }
            intervalSubmenu.addItem(it)
        }
        intervalItem.submenu = intervalSubmenu
        menu.addItem(intervalItem)

        // Restart Codex App
        let restartAppItem = NSMenuItem(title: L10n.restartApp, action: #selector(handleRestartApp), keyEquivalent: "")
        restartAppItem.target = self
        menu.addItem(restartAppItem)

        // Restart Codex App on Switch Toggle
        let restartOnSwitch = client.getRestartAppOnSwitch()
        let restartOnSwitchItem = NSMenuItem(
            title: L10n.restartAppOnSwitch,
            action: #selector(toggleRestartAppOnSwitch(_:)),
            keyEquivalent: ""
        )
        restartOnSwitchItem.target = self
        restartOnSwitchItem.state = restartOnSwitch ? .on : .off
        menu.addItem(restartOnSwitchItem)

        // Auto-switch on Quota Depletion Toggle
        let autoSwitch = client.getAutoSwitchEnabled()
        let autoSwitchItem = NSMenuItem(
            title: L10n.autoSwitchOnLimit,
            action: #selector(toggleAutoSwitchOnLimit(_:)),
            keyEquivalent: ""
        )
        autoSwitchItem.target = self
        autoSwitchItem.state = autoSwitch ? .on : .off
        self.autoSwitchItem = autoSwitchItem
        menu.addItem(autoSwitchItem)

        // Help Guide
        let helpItem = NSMenuItem(title: L10n.helpGuide, action: #selector(openHelpPage), keyEquivalent: "?")
        helpItem.target = self
        menu.addItem(helpItem)

        // CLI Status / Update
        let cliStatusTitle = VersionHelper.formatCLIStatusTitle(
            currentVersion: detectedCLIVersion,
            isUpdateAvailable: isCLIUpdateAvailable,
            availableVersion: availableCLIVersion
        )
        updateCLIItem = NSMenuItem(
            title: cliStatusTitle,
            action: isCLIUpdateAvailable ? #selector(updateCodexCLI) : #selector(noop),
            keyEquivalent: isCLIUpdateAvailable ? "u" : ""
        )
        updateCLIItem?.target = self
        menu.addItem(updateCLIItem!)

        menu.addItem(NSMenuItem.separator())

        // Stack percentages (2-Row)
        let isStacked = UserDefaults.standard.object(forKey: "stackPercentages") as? Bool ?? true
        let stackItem = NSMenuItem(
            title: L10n.stackPercentages,
            action: #selector(toggleStackPercentages),
            keyEquivalent: ""
        )
        stackItem.target = self
        stackItem.state = isStacked ? .on : .off
        stackPercentagesItem = stackItem
        menu.addItem(stackItem)

        // Launch at login
        let autostart = NSMenuItem(
            title: L10n.launchAtLogin,
            action: #selector(toggleLaunchAtLogin),
            keyEquivalent: ""
        )
        autostart.target = self
        autostart.state = autoLaunchManager.isEnabled ? .on : .off
        launchAtLoginItem = autostart
        menu.addItem(autostart)

        // Quit
        let quitItem = NSMenuItem(title: L10n.quit, action: #selector(quitApp), keyEquivalent: "q")
        quitItem.target = self
        menu.addItem(quitItem)

        statusItem?.menu = menu
        return menu
    }

    // MARK: - Update Dynamic Menu Items

    private func updateUI(with snapshot: MultiAccountSnapshot) {
        updateStatusBar(with: snapshot)

        guard let menu = statusItem?.menu else { return }

        for it in dynamicAccountItems {
            menu.removeItem(it)
        }
        dynamicAccountItems.removeAll()

        guard let topIdx = menu.items.firstIndex(of: accountsSeparatorTop!) else { return }
        var insertIdx = topIdx + 1
        let isRu = LocalizationManager.shared.currentLanguage == .ru

        // ====================================================================
        // BLOCK 1: 🖥️ Codex Desktop App (ChatGPT.app)
        // ====================================================================
        let appHeaderTitle = isRu ? "🖥️ Codex Desktop App (сессия ChatGPT.app)" : "🖥️ Codex Desktop App (ChatGPT.app)"
        let appHeader = NSMenuItem(title: appHeaderTitle, action: #selector(noop), keyEquivalent: "")
        appHeader.target = self
        appHeader.attributedTitle = NSAttributedString(string: appHeaderTitle, attributes: [
            .font: NSFont.boldSystemFont(ofSize: 11),
            .foregroundColor: NSColor.secondaryLabelColor
        ])
        menu.insertItem(appHeader, at: insertIdx)
        dynamicAccountItems.append(appHeader)
        insertIdx += 1

        if snapshot.isAppRunning, let appAcc = snapshot.appAccount {
            let appStatusTag = isRu ? "[АКТИВЕН В APP]" : "[ACTIVE IN APP]"
            let dotColor = MenuBarAppearanceHelper.dropdownColor(forPercentage: appAcc.fiveHourPercentage)

            let appItem = NSMenuItem(title: "● \(appAcc.email)  \(appStatusTag)", action: #selector(noop), keyEquivalent: "")
            appItem.target = self
            let rich = NSMutableAttributedString()
            rich.append(NSAttributedString(string: "  ● ", attributes: [
                .font: NSFont.systemFont(ofSize: 13, weight: .bold),
                .foregroundColor: dotColor
            ]))
            rich.append(NSAttributedString(string: appAcc.email, attributes: [
                .font: NSFont.boldSystemFont(ofSize: 13),
                .foregroundColor: NSColor.labelColor
            ]))
            rich.append(NSAttributedString(string: "  \(appStatusTag)  \(appAcc.planBadgeString)", attributes: [
                .font: NSFont.systemFont(ofSize: 11, weight: .semibold),
                .foregroundColor: NSColor.systemTeal
            ]))
            appItem.attributedTitle = rich
            menu.insertItem(appItem, at: insertIdx)
            dynamicAccountItems.append(appItem)
            insertIdx += 1

            // 5-Hour sprint bar
            let pPct = appAcc.fiveHourPercentage
            let pStr = String(format: "%.0f%%", pPct)
            let pColor = MenuBarAppearanceHelper.dropdownColor(forPercentage: pPct, planMultiplier: appAcc.planMultiplier)
            let pRich = MenuBarAppearanceHelper.makeColoredProgressBar(label: "    ⚡ 5h Sprint: \(pStr) ", percentage: pPct, maxPercentage: 100.0 * appAcc.planMultiplier, fillColor: pColor)
            let resetDesc = appAcc.timeUntilResetString
            if !resetDesc.isEmpty && resetDesc != L10n.resetNow {
                pRich.append(NSAttributedString(string: " (\(resetDesc))", attributes: [
                    .font: NSFont.systemFont(ofSize: 11),
                    .foregroundColor: NSColor.secondaryLabelColor
                ]))
            }
            let bar5hItem = NSMenuItem(title: "    ⚡ 5h Sprint: \(pStr)", action: #selector(noop), keyEquivalent: "")
            bar5hItem.target = self
            bar5hItem.attributedTitle = pRich
            menu.insertItem(bar5hItem, at: insertIdx)
            dynamicAccountItems.append(bar5hItem)
            insertIdx += 1

            // Weekly bar
            if let wPct = appAcc.weeklyPercentage {
                let wStr = String(format: "%.0f%%", wPct)
                let wColor = MenuBarAppearanceHelper.dropdownColor(forPercentage: wPct, planMultiplier: appAcc.planMultiplier)
                let wRich = MenuBarAppearanceHelper.makeColoredProgressBar(label: "    🗓️ Weekly: \(wStr) ", percentage: wPct, maxPercentage: 100.0 * appAcc.planMultiplier, fillColor: wColor)
                let weekItem = NSMenuItem(title: "    🗓️ Weekly: \(wStr)", action: #selector(noop), keyEquivalent: "")
                weekItem.target = self
                weekItem.attributedTitle = wRich
                menu.insertItem(weekItem, at: insertIdx)
                dynamicAccountItems.append(weekItem)
                insertIdx += 1
            }

            // Credits
            if appAcc.credits > 0 {
                let credItem = NSMenuItem(title: "    ✨ \(L10n.resetCredits): \(appAcc.credits)", action: #selector(noop), keyEquivalent: "")
                credItem.target = self
                credItem.attributedTitle = NSAttributedString(string: "    ✨ \(L10n.resetCredits): \(appAcc.credits)", attributes: [
                    .font: NSFont.systemFont(ofSize: 11, weight: .medium),
                    .foregroundColor: NSColor.systemIndigo
                ])
                menu.insertItem(credItem, at: insertIdx)
                dynamicAccountItems.append(credItem)
                insertIdx += 1
            }

            // Error display if session failed or expired
            if let err = appAcc.error, !err.isEmpty {
                let errItem = NSMenuItem(title: "    ⚠️ \(err)", action: #selector(noop), keyEquivalent: "")
                errItem.target = self
                errItem.attributedTitle = NSAttributedString(string: "    ⚠️ \(err)", attributes: [
                    .font: NSFont.systemFont(ofSize: 11, weight: .medium),
                    .foregroundColor: NSColor.systemRed
                ])
                menu.insertItem(errItem, at: insertIdx)
                dynamicAccountItems.append(errItem)
                insertIdx += 1
            }
        } else {
            let notDetectedTitle = isRu ? "  ⚪ Сессия ChatGPT.app не обнаружена" : "  ⚪ ChatGPT.app session not detected"
            let notDetectedItem = NSMenuItem(title: notDetectedTitle, action: #selector(noop), keyEquivalent: "")
            notDetectedItem.target = self
            notDetectedItem.attributedTitle = NSAttributedString(string: notDetectedTitle, attributes: [
                .font: NSFont.systemFont(ofSize: 12),
                .foregroundColor: NSColor.secondaryLabelColor
            ])
            menu.insertItem(notDetectedItem, at: insertIdx)
            dynamicAccountItems.append(notDetectedItem)
            insertIdx += 1

            let hintTitle = isRu ? "    (Запустите ChatGPT.app для синхронизации)" : "    (Start ChatGPT.app to monitor desktop session)"
            let hintItem = NSMenuItem(title: hintTitle, action: #selector(noop), keyEquivalent: "")
            hintItem.target = self
            hintItem.attributedTitle = NSAttributedString(string: hintTitle, attributes: [
                .font: NSFont.systemFont(ofSize: 11),
                .foregroundColor: NSColor.tertiaryLabelColor
            ])
            menu.insertItem(hintItem, at: insertIdx)
            dynamicAccountItems.append(hintItem)
            insertIdx += 1
        }

        // Inset separator
        let sepBetween = AppDelegate.makeInsetSeparatorItem()
        menu.insertItem(sepBetween, at: insertIdx)
        dynamicAccountItems.append(sepBetween)
        insertIdx += 1

        // ====================================================================
        // BLOCK 2: 💻 Codex CLI (мульти-аккаунт ротация)
        // ====================================================================
        let cliHeaderTitle = isRu ? "💻 Codex CLI (мульти-аккаунт ротация)" : "💻 Codex CLI (multi-account rotation)"
        let cliHeader = NSMenuItem(title: cliHeaderTitle, action: #selector(noop), keyEquivalent: "")
        cliHeader.target = self
        cliHeader.attributedTitle = NSAttributedString(string: cliHeaderTitle, attributes: [
            .font: NSFont.boldSystemFont(ofSize: 11),
            .foregroundColor: NSColor.secondaryLabelColor
        ])
        menu.insertItem(cliHeader, at: insertIdx)
        dynamicAccountItems.append(cliHeader)
        insertIdx += 1

        let cliPrimary = snapshot.cliAccount ?? snapshot.accounts.first(where: { $0.isCurrentActive }) ?? snapshot.accounts.first
        if let activeAcc = cliPrimary {
            let statusTag = L10n.activeInCli
            let dotColor = MenuBarAppearanceHelper.dropdownColor(forPercentage: activeAcc.fiveHourPercentage, planMultiplier: activeAcc.planMultiplier)

            let accItem = NSMenuItem(title: "● \(activeAcc.email)  \(statusTag)", action: #selector(noop), keyEquivalent: "")
            accItem.target = self

            let rowView = AccountRowView(
                frame: NSRect(x: 0, y: 0, width: AppDelegate.defaultMenuWidth, height: 24),
                accountId: activeAcc.id,
                accountName: activeAcc.displayName,
                email: activeAcc.email,
                tier: activeAcc.planBadgeString,
                isCurrentActive: true,
                dotColor: dotColor,
                statusTag: statusTag,
                onDelete: { [weak self] id, email in
                    self?.confirmAndRemoveAccount(id: id, email: email)
                },
                onRename: { [weak self] id, name, email in
                    self?.promptRenameAccount(id: id, currentName: name, email: email)
                },
                onSelect: { _ in }
            )
            accItem.view = rowView
            menu.insertItem(accItem, at: insertIdx)
            dynamicAccountItems.append(accItem)
            insertIdx += 1

            // 5h Sprint bar
            let pPct = activeAcc.fiveHourPercentage
            let pStr = String(format: "%.0f%%", pPct)
            let pColor = MenuBarAppearanceHelper.dropdownColor(forPercentage: pPct, planMultiplier: activeAcc.planMultiplier)
            let pRich = MenuBarAppearanceHelper.makeColoredProgressBar(label: "    ⚡ 5h Sprint: \(pStr) ", percentage: pPct, maxPercentage: 100.0 * activeAcc.planMultiplier, fillColor: pColor)
            let resetDesc = activeAcc.timeUntilResetString
            if !resetDesc.isEmpty && resetDesc != L10n.resetNow {
                pRich.append(NSAttributedString(string: " (\(resetDesc))", attributes: [
                    .font: NSFont.systemFont(ofSize: 11),
                    .foregroundColor: NSColor.secondaryLabelColor
                ]))
            }
            let bar5hItem = NSMenuItem(title: "    ⚡ 5h Sprint: \(pStr)", action: #selector(noop), keyEquivalent: "")
            bar5hItem.target = self
            bar5hItem.attributedTitle = pRich
            menu.insertItem(bar5hItem, at: insertIdx)
            dynamicAccountItems.append(bar5hItem)
            insertIdx += 1

            // Weekly bar
            if let wPct = activeAcc.weeklyPercentage {
                let wStr = String(format: "%.0f%%", wPct)
                let wColor = MenuBarAppearanceHelper.dropdownColor(forPercentage: wPct, planMultiplier: activeAcc.planMultiplier)
                let wRich = MenuBarAppearanceHelper.makeColoredProgressBar(label: "    🗓️ Weekly: \(wStr) ", percentage: wPct, maxPercentage: 100.0 * activeAcc.planMultiplier, fillColor: wColor)
                let weekItem = NSMenuItem(title: "    🗓️ Weekly: \(wStr)", action: #selector(noop), keyEquivalent: "")
                weekItem.target = self
                weekItem.attributedTitle = wRich
                menu.insertItem(weekItem, at: insertIdx)
                dynamicAccountItems.append(weekItem)
                insertIdx += 1
            }

            // Models Submenu
            let modelsItem = NSMenuItem(title: L10n.modelsMenuTitle, action: #selector(noop), keyEquivalent: "")
            modelsItem.target = self
            modelsItem.attributedTitle = NSAttributedString(string: "  " + L10n.modelsMenuTitle, attributes: [
                .font: NSFont.systemFont(ofSize: 11, weight: .medium),
                .foregroundColor: NSColor.secondaryLabelColor
            ])
            let modelsSubmenu = NSMenu()
            modelsSubmenu.autoenablesItems = false

            let standardModels = [
                "gpt-5-5",
                "gpt-5-4",
                "gpt-5-codex",
                "o3",
                "o3-mini",
                "gpt-4.5"
            ]

            let activeModel = snapshot.activeModelName ?? "gpt-5-5"
            for m in standardModels {
                let it = NSMenuItem(title: m, action: #selector(switchModelAction(_:)), keyEquivalent: "")
                it.target = self
                it.representedObject = m
                if ModelMatcher.matches(displayName: m, target: activeModel) {
                    it.state = .on
                } else {
                    it.state = .off
                }
                modelsSubmenu.addItem(it)
            }
            modelsItem.submenu = modelsSubmenu
            menu.insertItem(modelsItem, at: insertIdx)
            dynamicAccountItems.append(modelsItem)
            insertIdx += 1

            // Error display for CLI active account
            if let err = activeAcc.error, !err.isEmpty {
                let errItem = NSMenuItem(title: "    ⚠️ \(err)", action: #selector(noop), keyEquivalent: "")
                errItem.target = self
                errItem.attributedTitle = NSAttributedString(string: "    ⚠️ \(err)", attributes: [
                    .font: NSFont.systemFont(ofSize: 11, weight: .medium),
                    .foregroundColor: NSColor.systemRed
                ])
                menu.insertItem(errItem, at: insertIdx)
                dynamicAccountItems.append(errItem)
                insertIdx += 1
            }
        }

        // ====================================================================
        // BLOCK 3: 👥 Резервные аккаунты (CLI пул)
        // ====================================================================
        let reserveAccs = snapshot.accounts.filter { $0.id != (cliPrimary?.id ?? "") }
        if !reserveAccs.isEmpty {
            let sepReserves = AppDelegate.makeInsetSeparatorItem()
            menu.insertItem(sepReserves, at: insertIdx)
            dynamicAccountItems.append(sepReserves)
            insertIdx += 1

            let reservesHeaderTitle = isRu ? "👥 Резервные аккаунты (CLI пул)" : "👥 Reserve Accounts (CLI Pool)"
            let resHeader = NSMenuItem(title: reservesHeaderTitle, action: #selector(noop), keyEquivalent: "")
            resHeader.target = self
            resHeader.attributedTitle = NSAttributedString(string: reservesHeaderTitle, attributes: [
                .font: NSFont.boldSystemFont(ofSize: 11),
                .foregroundColor: NSColor.secondaryLabelColor
            ])
            menu.insertItem(resHeader, at: insertIdx)
            dynamicAccountItems.append(resHeader)
            insertIdx += 1

            for (index, acc) in reserveAccs.enumerated() {
                let statusTag = L10n.reserveSlot(index: index + 1)
                let dotColor = MenuBarAppearanceHelper.dropdownColor(forPercentage: acc.fiveHourPercentage, planMultiplier: acc.planMultiplier)

                if index > 0 {
                    let sep = AppDelegate.makeInsetSeparatorItem()
                    menu.insertItem(sep, at: insertIdx)
                    dynamicAccountItems.append(sep)
                    insertIdx += 1
                }

                let accItem = NSMenuItem(title: "● \(acc.email)  \(statusTag)", action: #selector(noop), keyEquivalent: "")
                accItem.target = self

                let rowView = AccountRowView(
                    frame: NSRect(x: 0, y: 0, width: AppDelegate.defaultMenuWidth, height: 24),
                    accountId: acc.id,
                    accountName: acc.displayName,
                    email: acc.email,
                    tier: acc.planBadgeString,
                    isCurrentActive: false,
                    dotColor: dotColor,
                    statusTag: statusTag,
                    onDelete: { [weak self] id, email in
                        self?.confirmAndRemoveAccount(id: id, email: email)
                    },
                    onRename: { [weak self] id, name, email in
                        self?.promptRenameAccount(id: id, currentName: name, email: email)
                    },
                    onSelect: { [weak self] id in
                        self?.executeSwitchAccount(id: id)
                    }
                )
                accItem.view = rowView
                menu.insertItem(accItem, at: insertIdx)
                dynamicAccountItems.append(accItem)
                insertIdx += 1

                // 5h Sprint bar for reserve
                let pPct = acc.fiveHourPercentage
                let pStr = String(format: "%.0f%%", pPct)
                let pColor = MenuBarAppearanceHelper.dropdownColor(forPercentage: pPct, planMultiplier: acc.planMultiplier)
                let pRich = MenuBarAppearanceHelper.makeColoredProgressBar(label: "    ⚡ 5h Sprint: \(pStr) ", percentage: pPct, maxPercentage: 100.0 * acc.planMultiplier, fillColor: pColor)
                let resetDesc = acc.timeUntilResetString
                if !resetDesc.isEmpty && resetDesc != L10n.resetNow {
                    pRich.append(NSAttributedString(string: " (\(resetDesc))", attributes: [
                        .font: NSFont.systemFont(ofSize: 11),
                        .foregroundColor: NSColor.secondaryLabelColor
                    ]))
                }
                let bar5hItem = NSMenuItem(title: "    ⚡ 5h Sprint: \(pStr)", action: #selector(noop), keyEquivalent: "")
                bar5hItem.target = self
                bar5hItem.attributedTitle = pRich
                menu.insertItem(bar5hItem, at: insertIdx)
                dynamicAccountItems.append(bar5hItem)
                insertIdx += 1

                // Explicit Clickable Switch Item
                let switchItemTitle = "    ⇄ " + L10n.switchToAccount
                let switchItem = NSMenuItem(
                    title: switchItemTitle,
                    action: #selector(handleSwitchMenuItem(_:)),
                    keyEquivalent: ""
                )
                switchItem.target = self
                switchItem.representedObject = acc.id
                switchItem.attributedTitle = NSAttributedString(string: switchItemTitle, attributes: [
                    .font: NSFont.systemFont(ofSize: 12, weight: .medium),
                    .foregroundColor: NSColor.systemBlue
                ])
                menu.insertItem(switchItem, at: insertIdx)
                dynamicAccountItems.append(switchItem)
                insertIdx += 1

                // Error display for reserve account
                if let err = acc.error, !err.isEmpty {
                    let errItem = NSMenuItem(title: "    ⚠️ \(err)", action: #selector(noop), keyEquivalent: "")
                    errItem.target = self
                    errItem.attributedTitle = NSAttributedString(string: "    ⚠️ \(err)", attributes: [
                        .font: NSFont.systemFont(ofSize: 11, weight: .medium),
                        .foregroundColor: NSColor.systemRed
                    ])
                    menu.insertItem(errItem, at: insertIdx)
                    dynamicAccountItems.append(errItem)
                    insertIdx += 1
                }
            }
        }

        // Inset separator before Add Account
        let preAddSep = AppDelegate.makeInsetSeparatorItem()
        menu.insertItem(preAddSep, at: insertIdx)
        dynamicAccountItems.append(preAddSep)
        insertIdx += 1

        // "➕ Add Account..." Button
        let addAccountItem = NSMenuItem(title: L10n.addAccount, action: #selector(addAccountAction), keyEquivalent: "")
        addAccountItem.target = self
        menu.insertItem(addAccountItem, at: insertIdx)
        dynamicAccountItems.append(addAccountItem)
        insertIdx += 1

        // Update timestamp
        lastUpdatedMenuItem?.attributedTitle = NSAttributedString(
            string: L10n.lastUpdated(time: Self.timeOfDayFormatter.string(from: snapshot.timestamp)),
            attributes: [
                .font: NSFont.systemFont(ofSize: 12),
                .foregroundColor: NSColor.secondaryLabelColor
            ]
        )
    }

    public static func makeInsetSeparatorItem(width: CGFloat = defaultMenuWidth, inset: CGFloat = 16) -> NSMenuItem {
        let item = NSMenuItem()
        let view = InsetSeparatorView(frame: NSRect(x: 0, y: 0, width: width, height: 7), horizontalInset: inset)
        item.view = view
        return item
    }

    // MARK: - Timer & Refresh Actions

    private func startTimer() {
        refreshTimer?.invalidate()
        refreshTimer = Timer.scheduledTimer(withTimeInterval: refreshInterval, repeats: true) { [weak self] _ in
            self?.refreshNow()
        }
    }

    @objc private func noop() {}

    @objc internal func refreshNow() {
        guard !isRefreshing else {
            refreshPendingWhileBusy = true
            return
        }
        isRefreshing = true

        lastUpdatedMenuItem?.attributedTitle = NSAttributedString(
            string: L10n.refreshing,
            attributes: [
                .font: NSFont.systemFont(ofSize: 12),
                .foregroundColor: NSColor.secondaryLabelColor
            ]
        )
        client.refreshQuotas { [weak self] freshSnapshot in
            guard let self = self else { return }
            self.isRefreshing = false
            if let snap = freshSnapshot {
                self.lastSnapshot = snap
                self.updateUI(with: snap)
                self.checkAutoSwitchQuotaDepletion(snapshot: snap)
            }
            if self.refreshPendingWhileBusy {
                self.refreshPendingWhileBusy = false
                self.refreshNow()
            }
        }
    }

    private func refreshCLIVersion() {
        DispatchQueue.global(qos: .utility).async { [weak self] in
            guard let self = self else { return }
            let home = FileManager.default.homeDirectoryForCurrentUser
            let candidates = [
                home.appendingPathComponent(".local/bin/codex-mon").path,
                "/usr/local/bin/codex-mon",
                "/usr/local/bin/cxi",
                home.appendingPathComponent(".local/bin/cxi").path
            ]
            for path in candidates {
                guard FileManager.default.isExecutableFile(atPath: path) else { continue }
                let pipe = Pipe()
                let proc = Process()
                proc.executableURL = URL(fileURLWithPath: path)
                proc.arguments = ["--version"]
                proc.standardOutput = pipe
                proc.standardError = Pipe()
                do {
                    try proc.run()
                    let data = pipe.fileHandleForReading.readDataToEndOfFile()
                    proc.waitUntilExit()
                    if let str = String(data: data, encoding: .utf8),
                       let ver = VersionHelper.parseCLIVersion(from: str) {
                        DispatchQueue.main.async { [weak self] in
                            self?.detectedCLIVersion = ver
                            self?.updateCLIItemDisplay()
                        }
                        return
                    }
                } catch {
                    continue
                }
            }
        }
    }

    private func updateCLIItemDisplay() {
        let title = VersionHelper.formatCLIStatusTitle(
            currentVersion: detectedCLIVersion,
            isUpdateAvailable: isCLIUpdateAvailable,
            availableVersion: availableCLIVersion
        )
        updateCLIItem?.title = title
    }

    @objc private func updateCodexCLI() {
        refreshCLIVersion()
        refreshNow()
    }

    @objc internal func changeInterval(_ sender: NSMenuItem) {
        let newInterval = TimeInterval(sender.tag)
        refreshInterval = newInterval
        UserDefaults.standard.set(newInterval, forKey: AppDelegate.refreshIntervalKey)
        if let submenu = sender.menu {
            for item in submenu.items {
                item.state = (item.tag == sender.tag) ? .on : .off
            }
        }
        startTimer()
    }

    @objc internal func addAccountAction() {
        let alert = NSAlert()
        alert.messageText = L10n.addAccountTitle
        alert.informativeText = L10n.addAccountMsg
        alert.alertStyle = .informational

        // Compute smart suggested account ID based on active email or account count
        let existingIds = Set((lastSnapshot?.accounts ?? []).map { $0.id })
        let suggestedId: String
        if let activeEmail = lastSnapshot?.activeEmail ?? client.loadCachedSnapshot()?.activeEmail, !activeEmail.isEmpty {
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
                    self?.showAlert(title: L10n.addAccountTitle, message: L10n.addAccountSavedSuccess(id: finalId))
                } else {
                    let desc = (errMsg?.isEmpty == false) ? errMsg! : L10n.addAccountFailedDesc
                    self?.showAlert(title: L10n.addAccountFailedTitle, message: desc, style: .warning)
                }
            }
        }
    }

    private func showAlert(title: String, message: String, style: NSAlert.Style = .informational) {
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
            client.renameAccount(id: id, newName: newName.isEmpty ? nil : newName) { [weak self] success, err in
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

    @objc private func toggleRestartAppOnSwitch(_ sender: NSMenuItem) {
        let newState = sender.state != .on
        sender.state = newState ? .on : .off
        client.setRestartAppOnSwitch(newState)
    }

    @objc private func toggleAutoSwitchOnLimit(_ sender: NSMenuItem) {
        let newState = sender.state != .on
        sender.state = newState ? .on : .off
        client.setAutoSwitchEnabled(newState)
    }

    private func checkAutoSwitchQuotaDepletion(snapshot: MultiAccountSnapshot) {
        guard client.getAutoSwitchEnabled() else { return }
        guard !isAutoSwitching else { return }
        if let last = lastAutoSwitchTime, Date().timeIntervalSince(last) < 60 {
            return
        }

        guard let activeAcc = snapshot.cliAccount ?? snapshot.accounts.first(where: { $0.isCurrentActive }) ?? snapshot.accounts.first else {
            return
        }

        let isDepleted = activeAcc.fiveHourPercentage <= 0.0 ||
            (activeAcc.error?.localizedCaseInsensitiveContains("429") == true) ||
            (activeAcc.error?.localizedCaseInsensitiveContains("limit") == true)

        guard isDepleted else { return }

        // Find candidate reserve accounts with available quota (> 0%)
        let candidates = snapshot.accounts.filter { acc in
            acc.id != activeAcc.id && acc.fiveHourPercentage > 0.0 && acc.error == nil
        }

        guard let bestCandidate = candidates.max(by: { $0.fiveHourPercentage < $1.fiveHourPercentage }) else {
            return
        }

        self.isAutoSwitching = true
        self.lastAutoSwitchTime = Date()

        NSLog("[CodexMonitor] Auto-switching from %@ (0%%) to %@ (%.0f%%)", activeAcc.email, bestCandidate.email, bestCandidate.fiveHourPercentage)

        self.executeSwitchAccount(id: bestCandidate.id)
        DispatchQueue.main.asyncAfter(deadline: .now() + 15.0) { [weak self] in
            self?.isAutoSwitching = false
        }
    }

    @objc private func handleSwitchMenuItem(_ sender: NSMenuItem) {
        if let id = sender.representedObject as? String {
            executeSwitchAccount(id: id)
        }
    }

    internal func executeSwitchAccount(id: String) {
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

    @objc private func handleRestartApp() {
        client.restartCodexDesktopApp()
    }

    @objc private func openHelpPage() {
        let langCode = LocalizationManager.shared.currentLanguage.rawValue
        if let url = HelpsDocHelper.localizedHelpsHTMLURL(languageCode: langCode) {
            NSWorkspace.shared.open(url)
        } else if let fallbackURL = HelpsDocHelper.findHelpsHTMLURL() {
            NSWorkspace.shared.open(fallbackURL)
        }
    }

    @objc func toggleLaunchAtLogin() {
        let newState = !autoLaunchManager.isEnabled
        autoLaunchManager.setEnabled(newState)
        launchAtLoginItem?.state = newState ? .on : .off
    }

    @objc func toggleStackPercentages() {
        let current = UserDefaults.standard.object(forKey: "stackPercentages") as? Bool ?? true
        let updated = !current
        UserDefaults.standard.set(updated, forKey: "stackPercentages")
        stackPercentagesItem?.state = updated ? .on : .off
        if let snap = lastSnapshot {
            updateStatusBar(with: snap)
        }
    }

    @objc private func quitApp() {
        NSApp.terminate(nil)
    }

    // MARK: - Status File Watcher (Instant 0ms UI Updates)

    internal func startStatusFileWatcher() {
        stopStatusFileWatcher()
        let statusPath = CodexClient.statusFileURL.path
        guard FileManager.default.fileExists(atPath: statusPath) else { return }

        let fd = open(statusPath, O_EVTONLY)
        guard fd >= 0 else { return }
        self.fileWatcherFD = fd

        let source = DispatchSource.makeFileSystemObjectSource(
            fileDescriptor: fd,
            eventMask: [.write, .delete, .rename, .extend, .attrib],
            queue: DispatchQueue.main
        )

        source.setEventHandler { [weak self] in
            guard let self = self else { return }
            let flags = source.data

            self.statusUpdateWorkItem?.cancel()
            let updateItem = DispatchWorkItem { [weak self] in
                if let snap = self?.client.loadCachedSnapshot() {
                    self?.lastSnapshot = snap
                    self?.updateUI(with: snap)
                }
            }
            self.statusUpdateWorkItem = updateItem
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.1, execute: updateItem)

            if flags.contains(.delete) || flags.contains(.rename) {
                self.stopStatusFileWatcher()
                self.statusRestartWorkItem?.cancel()
                let restartItem = DispatchWorkItem { [weak self] in
                    self?.startStatusFileWatcher()
                }
                self.statusRestartWorkItem = restartItem
                DispatchQueue.main.asyncAfter(deadline: .now() + 0.2, execute: restartItem)
            }
        }

        source.setCancelHandler {
            close(fd)
        }

        source.resume()
        self.fileWatcherSource = source
    }

    internal func stopStatusFileWatcher() {
        statusRestartWorkItem?.cancel()
        statusRestartWorkItem = nil
        fileWatcherSource?.cancel()
        fileWatcherSource = nil
        fileWatcherFD = -1
    }

    // MARK: - Auth File Watcher (Instant Auto-Save of Logins in Codex App)

    internal func startAuthFileWatcher() {
        stopAuthFileWatcher()
        let authPath = CodexClient.codexHome.appendingPathComponent("auth.json").path
        guard FileManager.default.fileExists(atPath: authPath) else {
            // Retry after a short delay if auth.json doesn't exist yet
            authRestartWorkItem?.cancel()
            let retryItem = DispatchWorkItem { [weak self] in
                self?.startAuthFileWatcher()
            }
            authRestartWorkItem = retryItem
            DispatchQueue.main.asyncAfter(deadline: .now() + 3.0, execute: retryItem)
            return
        }

        let fd = open(authPath, O_EVTONLY)
        guard fd >= 0 else { return }
        self.authWatcherFD = fd

        let source = DispatchSource.makeFileSystemObjectSource(
            fileDescriptor: fd,
            eventMask: [.write, .delete, .rename, .extend, .attrib],
            queue: DispatchQueue.main
        )

        source.setEventHandler { [weak self] in
            guard let self = self else { return }
            let flags = source.data

            // Debounce so the external app (ChatGPT.app / codex login) finishes writing
            self.authRefreshWorkItem?.cancel()
            let refreshItem = DispatchWorkItem { [weak self] in
                self?.refreshNow()
            }
            self.authRefreshWorkItem = refreshItem
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.4, execute: refreshItem)

            if flags.contains(.delete) || flags.contains(.rename) {
                self.stopAuthFileWatcher()
                self.authRestartWorkItem?.cancel()
                let restartItem = DispatchWorkItem { [weak self] in
                    self?.startAuthFileWatcher()
                }
                self.authRestartWorkItem = restartItem
                DispatchQueue.main.asyncAfter(deadline: .now() + 0.5, execute: restartItem)
            }
        }

        source.setCancelHandler {
            close(fd)
        }

        source.resume()
        self.authWatcherSource = source
    }

    internal func stopAuthFileWatcher() {
        authRestartWorkItem?.cancel()
        authRestartWorkItem = nil
        authWatcherSource?.cancel()
        authWatcherSource = nil
        authWatcherFD = -1
    }
}
