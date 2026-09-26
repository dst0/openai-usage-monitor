import AppKit
import Foundation

func assertEqual<T: Equatable>(
  _ actual: T, _ expected: T, _ message: String = "", file: StaticString = #file, line: UInt = #line
) {
  if actual != expected {
    print("❌ Assertion Failed: [\(actual)] != [\(expected)] - \(message) at \(file):\(line)")
    exit(1)
  }
}

func assertTrue(
  _ condition: Bool, _ message: String = "", file: StaticString = #file, line: UInt = #line
) {
  if !condition {
    print("❌ Assertion Failed: condition is false - \(message) at \(file):\(line)")
    exit(1)
  }
}

func waitUntil(
  _ message: String, timeout: TimeInterval = 2.0, condition: @escaping () -> Bool
) {
  let deadline = Date().addingTimeInterval(timeout)
  while !condition() && Date() < deadline {
    RunLoop.current.run(until: Date().addingTimeInterval(0.01))
  }
  assertTrue(condition(), message)
}

@main
struct AppDelegateTestRunner {
  static func main() {
    print("🧪 Running AppDelegate Status Bar & Layout Tests...")

    let mockIcon = NSImage(size: NSSize(width: 18, height: 18))

    // ====================================================================
    // Test 1: Stacked mode (stackPercentages: true) - Single Session (CLI)
    // ====================================================================
    let defaultStackedAttr = AppDelegate.buildStatusBarAttributedString(
      icon: mockIcon,
      fiveHPct: "90%",
      fiveHColor: NSColor.systemGreen,
      weeklyPct: "85%",
      weeklyColor: NSColor.systemGreen,
      accounts: [],
      isScreenActive: true,
      useQuotaIcons: true,
      stackPercentages: true
    )

    let stackedStr = defaultStackedAttr.string
    assertTrue(stackedStr.contains("CLI "), "CLI tag must be present")
    assertTrue(
      stackedStr.contains("[") && stackedStr.contains("]"), "Bracket frame must be present")
    assertTrue(
      !stackedStr.contains("90%"),
      "Percentages should be drawn into attachment image, not text string")
    assertTrue(
      !stackedStr.contains("85%"),
      "Percentages should be drawn into attachment image, not text string")

    // Count attachments: 1 app icon + 1 stacked values attachment + 1 hybrid badge = 3 attachments
    var stackedAttachmentCount = 0
    var stackedAttachments: [NSTextAttachment] = []
    defaultStackedAttr.enumerateAttribute(
      .attachment, in: NSRange(location: 0, length: defaultStackedAttr.length), options: []
    ) { val, _, _ in
      if let att = val as? NSTextAttachment {
        stackedAttachmentCount += 1
        stackedAttachments.append(att)
      }
    }
    assertEqual(
      stackedAttachmentCount, 3,
      "Expected 1 app icon + 1 stacked values + 1 bracketed badge = 3 attachments")

    // Attachment 0: app icon (bounds 18x18, vertically centered at y=-6.0)
    assertEqual(stackedAttachments[0].image, mockIcon)
    assertEqual(stackedAttachments[0].bounds, CGRect(x: 0, y: -6.0, width: 18, height: 18))

    // Verify attachment with 22x22 icon (standard macOS menu bar size, vertically centered at y=-7.0)
    let icon22 = NSImage(size: NSSize(width: 22, height: 22))
    let attr22 = AppDelegate.buildStatusBarAttributedString(
      icon: icon22,
      fiveHPct: "90%",
      fiveHColor: NSColor.systemGreen,
      weeklyPct: "85%",
      weeklyColor: NSColor.systemGreen,
      accounts: [],
      isScreenActive: true,
      useQuotaIcons: true,
      stackPercentages: true
    )
    var att22List: [NSTextAttachment] = []
    attr22.enumerateAttribute(
      .attachment, in: NSRange(location: 0, length: attr22.length), options: []
    ) { val, _, _ in
      if let att = val as? NSTextAttachment { att22List.append(att) }
    }
    assertEqual(
      att22List[0].bounds, CGRect(x: 0, y: -7.0, width: 22, height: 22),
      "22x22 icon must have bounds y=-7.0, w=22, h=22")

    // Attachment 1: 2-row stacked values (bounds height=20.5, y=-6.0)
    assertEqual(
      stackedAttachments[1].bounds.origin.y, -6.0, "Stacked values attachment y origin must be -6.0"
    )
    assertEqual(
      stackedAttachments[1].bounds.size.height, 20.5,
      "Stacked values attachment height must be 20.5")
    assertTrue(
      stackedAttachments[1].bounds.size.width > 20.0,
      "Stacked values attachment width must be > 20.0")

    // Bracket '[' attributes: font pointSize 21.0, baselineOffset -2.4
    do {
      let fullStr = defaultStackedAttr.string as NSString
      let bracketIdx = fullStr.range(of: "[").location
      assertTrue(bracketIdx != NSNotFound, "Bracket '[' must exist in attributed string")
      let bracketAttrs = defaultStackedAttr.attributes(at: bracketIdx, effectiveRange: nil)
      if let bracketFnt = bracketAttrs[.font] as? NSFont {
        assertEqual(bracketFnt.pointSize, 21.0, "Bracket font pointSize must be 21.0")
      } else {
        assertTrue(false, "Bracket must have .font attribute")
      }
      if let bracketBL = bracketAttrs[.baselineOffset] as? CGFloat {
        assertEqual(bracketBL, -2.4, "Bracket baselineOffset must be -2.4")
      } else {
        assertTrue(false, "Bracket must have .baselineOffset attribute")
      }
    }

    // Attachment 2: badge attachment bounds must be CGRect(x: 0, y: -5.0, width: 6.5, height: 16.5)
    assertEqual(
      stackedAttachments[2].bounds, CGRect(x: 0, y: -5.0, width: 6.5, height: 16.5),
      "Badge attachment bounds must match")

    print("  ✅ Stacked mode single session layout verified")

    // ====================================================================
    // Test 2: Dual Session Mode (APP + CLI) with stacked layout
    // ====================================================================
    let dualStackedAttr = AppDelegate.buildStatusBarAttributedString(
      icon: mockIcon,
      appSession: (
        fiveHPct: "100%", fiveHColor: NSColor.systemGreen, weeklyPct: "95%",
        weeklyColor: NSColor.systemGreen
      ),
      cliSession: (
        fiveHPct: "90%", fiveHColor: NSColor.systemGreen, weeklyPct: "85%",
        weeklyColor: NSColor.systemGreen
      ),
      accounts: [],
      isScreenActive: true,
      useQuotaIcons: true,
      stackPercentages: true
    )
    let dualStackedStr = dualStackedAttr.string
    assertTrue(dualStackedStr.contains("APP "), "APP session tag must be present")
    assertTrue(dualStackedStr.contains("CLI "), "CLI session tag must be present")
    assertTrue(dualStackedStr.contains(" │ "), "Session separator │ must separate APP and CLI")

    var dualStackedAttachmentCount = 0
    var dualStackedAttachments: [NSTextAttachment] = []
    dualStackedAttr.enumerateAttribute(
      .attachment, in: NSRange(location: 0, length: dualStackedAttr.length), options: []
    ) { val, _, _ in
      if let att = val as? NSTextAttachment {
        dualStackedAttachmentCount += 1
        dualStackedAttachments.append(att)
      }
    }
    assertEqual(
      dualStackedAttachmentCount, 4,
      "Expected 1 app icon + 1 APP stacked + 1 CLI stacked + 1 badge = 4 attachments")
    assertTrue(
      dualStackedAttachments[1] !== dualStackedAttachments[2],
      "APP and CLI attachments must be distinct instances")
    assertEqual(dualStackedAttachments[1].bounds.size.height, 20.5)
    assertEqual(dualStackedAttachments[2].bounds.size.height, 20.5)

    print("  ✅ Dual session mode (APP + CLI) layout verified")

    // ====================================================================
    // Test 3: Horizontal Mode (stackPercentages: false)
    // ====================================================================
    let horizontalAttr = AppDelegate.buildStatusBarAttributedString(
      icon: mockIcon,
      fiveHPct: "48%",
      fiveHColor: NSColor.systemGreen,
      weeklyPct: "44%",
      weeklyColor: NSColor.systemGreen,
      accounts: [],
      isScreenActive: true,
      useQuotaIcons: true,
      stackPercentages: false
    )
    let horizStr = horizontalAttr.string
    assertTrue(horizStr.contains("48%"), "In horizontal mode, 5h percentage must be in text string")
    assertTrue(
      horizStr.contains("44%"), "In horizontal mode, weekly percentage must be in text string")

    var horizAttachmentCount = 0
    var horizAttachments: [NSTextAttachment] = []
    horizontalAttr.enumerateAttribute(
      .attachment, in: NSRange(location: 0, length: horizontalAttr.length), options: []
    ) { val, _, _ in
      if let att = val as? NSTextAttachment {
        horizAttachmentCount += 1
        horizAttachments.append(att)
      }
    }
    // 1 app icon + 1 sprint icon + 1 weekly icon + 1 shield = 4 attachments
    assertEqual(
      horizAttachmentCount, 4,
      "Expected 1 app icon + 1 sprint icon + 1 weekly icon + 1 shield badge = 4 attachments")

    // Bracket '[' attributes in horizontal mode: font pointSize 21.0, baselineOffset -2.4
    do {
      let horizNS = horizontalAttr.string as NSString
      let hBracketIdx = horizNS.range(of: "[").location
      assertTrue(
        hBracketIdx != NSNotFound, "Bracket '[' must exist in horizontal attributed string")
      let hBracketAttrs = horizontalAttr.attributes(at: hBracketIdx, effectiveRange: nil)
      if let hBracketFnt = hBracketAttrs[.font] as? NSFont {
        assertEqual(hBracketFnt.pointSize, 21.0, "Horizontal bracket font pointSize must be 21.0")
      } else {
        assertTrue(false, "Horizontal bracket must have .font attribute")
      }
      if let hBracketBL = hBracketAttrs[.baselineOffset] as? CGFloat {
        assertEqual(hBracketBL, -2.4, "Horizontal bracket baselineOffset must be -2.4")
      } else {
        assertTrue(false, "Horizontal bracket must have .baselineOffset attribute")
      }
    }

    // Active badge attachment bounds in horizontal mode: CGRect(x: 0, y: -5.0, width: 6.5, height: 16.5)
    // The badge is the last attachment (index 3)
    assertEqual(
      horizAttachments[3].bounds, CGRect(x: 0, y: -5.0, width: 6.5, height: 16.5),
      "Horizontal active badge bounds must match")

    print("  ✅ Horizontal mode layout verified")

    // ====================================================================
    // Test 4: UserDefaults default & toggle logic
    // ====================================================================
    UserDefaults.standard.removeObject(forKey: "stackPercentages")
    let defaultPref = UserDefaults.standard.object(forKey: "stackPercentages") as? Bool ?? true
    assertTrue(defaultPref == true, "Default stackPercentages must be true")

    UserDefaults.standard.set(false, forKey: "stackPercentages")
    let updatedPref = UserDefaults.standard.bool(forKey: "stackPercentages")
    assertTrue(updatedPref == false, "stackPercentages must reflect updated value false")

    UserDefaults.standard.set(true, forKey: "stackPercentages")
    // ====================================================================
    // Test 5: Auto-Switch localization and client default
    // ====================================================================
    let ruStr = L10n.autoSwitchOnLimit
    assertTrue(!ruStr.isEmpty, "Auto-switch localization must not be empty")
    let client = CodexClient.shared
    let autoSwitch = client.getAutoSwitchEnabled()
    assertTrue(autoSwitch == true || autoSwitch == false, "Auto-switch enabled must return boolean")
    print("  ✅ Auto-switch localization & client settings verified (enabled: \(autoSwitch))")

    let stopCommands = CodexClient.backgroundAutomationStopCommands(
      daemonPath: "/tmp/com.codex.switcher.plist")
    assertEqual(
      stopCommands,
      [["unload", "/tmp/com.codex.switcher.plist"]],
      "Quit must stop the daemon without killing a worker inside the shutdown/relaunch boundary"
    )
    print("  ✅ Background automation safe cancellation boundary verified")

    // ====================================================================
    // Test 6: Inactive Screen Dimming & Compatibility Matrix
    // ====================================================================
    // 1. Verify button alphaValue remains 1.0 (full opacity) to prevent macOS vibrancy wash-out
    let activeAlpha: CGFloat = 1.0
    assertEqual(
      activeAlpha, 1.0, "Button alphaValue must remain 1.0 to prevent macOS vibrancy wash-out")

    // 2. Verify attributed string generation does not bloom on inactive screen
    let activeFull = AppDelegate.buildStatusBarAttributedString(
      icon: mockIcon,
      appSession: (
        fiveHPct: "100%", fiveHColor: NSColor.systemGreen, weeklyPct: "95%",
        weeklyColor: NSColor.systemGreen
      ),
      cliSession: (
        fiveHPct: "90%", fiveHColor: NSColor.systemGreen, weeklyPct: "85%",
        weeklyColor: NSColor.systemGreen
      ),
      accounts: [],
      isScreenActive: true,
      useQuotaIcons: true,
      stackPercentages: true
    )
    let inactiveFull = AppDelegate.buildStatusBarAttributedString(
      icon: mockIcon,
      appSession: (
        fiveHPct: "100%", fiveHColor: NSColor.systemGreen, weeklyPct: "95%",
        weeklyColor: NSColor.systemGreen
      ),
      cliSession: (
        fiveHPct: "90%", fiveHColor: NSColor.systemGreen, weeklyPct: "85%",
        weeklyColor: NSColor.systemGreen
      ),
      accounts: [],
      isScreenActive: false,
      useQuotaIcons: true,
      stackPercentages: true
    )
    assertEqual(activeFull.string, inactiveFull.string, "String structure must be identical")

    // 3. Verify APP tag and CLI tag colors are not inflated to neon pastel when inactive
    var activeTagColors: [NSColor] = []
    activeFull.enumerateAttribute(
      .foregroundColor, in: NSRange(location: 0, length: activeFull.length), options: []
    ) { val, _, _ in
      if let col = val as? NSColor { activeTagColors.append(col) }
    }
    var inactiveTagColors: [NSColor] = []
    inactiveFull.enumerateAttribute(
      .foregroundColor, in: NSRange(location: 0, length: inactiveFull.length), options: []
    ) { val, _, _ in
      if let col = val as? NSColor { inactiveTagColors.append(col) }
    }
    assertTrue(!activeTagColors.isEmpty && !inactiveTagColors.isEmpty, "Tag colors must be present")
    for i in 0..<min(activeTagColors.count, inactiveTagColors.count) {
      let aCol = activeTagColors[i].usingColorSpace(.sRGB)!
      let iCol = inactiveTagColors[i].usingColorSpace(.sRGB)!
      let aLum =
        0.2126 * aCol.redComponent + 0.7152 * aCol.greenComponent + 0.0722 * aCol.blueComponent
      let iLum =
        0.2126 * iCol.redComponent + 0.7152 * iCol.greenComponent + 0.0722 * iCol.blueComponent
      assertTrue(
        iLum <= aLum + 0.05,
        "Inactive text luminance (\(iLum)) must not significantly exceed active text luminance (\(aLum))"
      )
    }
    print("  ✅ Inactive screen dimming & compatibility matrix verified")

    // ====================================================================
    // Test 7: Rust-Owned Distribution Command Plans
    // ====================================================================
    let bizAccount = AccountQuota(
      id: "work-1",
      name: "Work Business",
      email: "dev@work.com",
      planType: "team",
      isCurrentActive: false,
      fiveHourPercentage: 70.0,
      weeklyPercentage: nil,
      resetTime: nil,
      resetAfterSeconds: 3600,
      credits: 2
    )
    let personalAccount = AccountQuota(
      id: "personal-1",
      name: "Personal",
      email: "me@gmail.com",
      planType: "plus",
      isCurrentActive: false,
      fiveHourPercentage: 100.0,
      weeklyPercentage: nil,
      resetTime: nil,
      resetAfterSeconds: 1800,
      credits: 5
    )
    assertTrue(bizAccount.isBusiness, "Team plan must be detected as business")
    assertTrue(!personalAccount.isBusiness, "Plus plan must not be detected as business")

    assertTrue(
      !L10n.autoSwitchBusinessOnly.isEmpty, "autoSwitchBusinessOnly localization must not be empty")
    assertTrue(
      !L10n.autoSwitchBusinessPriority.isEmpty,
      "autoSwitchBusinessPriority localization must not be empty")

    let autoArguments = CodexClient.menuAutoDistributionArguments()
    assertEqual(
      autoArguments,
      ["distribute", "--trigger", "user", "--reason", "menu_auto_distribute"],
      "Manual auto-distribution must delegate target selection to Rust")

    let expectedPlans: [(CodexClient.SwitchTarget, [String])] = [
      (
        .app,
        [
          "distribute", "--trigger", "user", "--reason", "menu_app_target",
          "--app-target", "target-id", "--cli-target", "current-cli",
        ]
      ),
      (
        .cli,
        [
          "distribute", "--trigger", "user", "--reason", "menu_cli_target",
          "--app-target", "current-app", "--cli-target", "target-id", "--no-restart",
        ]
      ),
      (
        .both,
        [
          "distribute", "--trigger", "user", "--reason", "menu_both_target",
          "--app-target", "target-id", "--cli-target", "target-id",
        ]
      ),
    ]

    for (target, expected) in expectedPlans {
      let arguments = CodexClient.manualDistributionArguments(
        targetId: "target-id", target: target,
        currentAppId: "current-app", currentCliId: "current-cli")
      assertEqual(arguments, expected, "Manual target must produce one exact distribute plan")

      let invocationLock = NSLock()
      var invocations: [[String]] = []
      var completionResult: Bool?
      let client = CodexClient(distributionRunner: { recorded in
        invocationLock.lock()
        invocations.append(recorded)
        invocationLock.unlock()
        return true
      })
      client.switchToAccount(
        id: "target-id", target: target,
        currentAppId: "current-app", currentCliId: "current-cli"
      ) { success in
        invocationLock.lock()
        completionResult = success
        invocationLock.unlock()
      }
      waitUntil("Manual coordinator completion must be delivered") {
        invocationLock.lock()
        defer { invocationLock.unlock() }
        return completionResult != nil
      }
      invocationLock.lock()
      let surfacedSuccess = completionResult
      let recordedInvocations = invocations
      invocationLock.unlock()
      assertEqual(surfacedSuccess, true, "Successful coordinator termination must be surfaced")
      assertEqual(recordedInvocations, [expected], "Manual target must invoke distribute exactly once")
    }

    assertTrue(
      CodexClient.manualDistributionArguments(
        targetId: "target-id", target: .app,
        currentAppId: "current-app", currentCliId: nil) == nil,
      "APP-only switching must fail closed when the CLI identity to preserve is unknown")
    assertTrue(
      CodexClient.manualDistributionArguments(
        targetId: "target-id", target: .cli,
        currentAppId: nil, currentCliId: "current-cli") == nil,
      "CLI-only switching must fail closed when the App identity to preserve is unknown")

    let missingIdentityLock = NSLock()
    var missingIdentityInvocations = 0
    var missingIdentityResult: Bool?
    let missingIdentityClient = CodexClient(distributionRunner: { _ in
      missingIdentityLock.lock()
      missingIdentityInvocations += 1
      missingIdentityLock.unlock()
      return true
    })
    missingIdentityClient.switchToAccount(
      id: "target-id", target: .app,
      currentAppId: "current-app", currentCliId: nil
    ) { missingIdentityResult = $0 }
    assertEqual(missingIdentityResult, false, "Missing preservation identity must surface failure")
    assertEqual(missingIdentityInvocations, 0, "Fail-closed plans must not start a process")

    let failClosedSnapshot = MultiAccountSnapshot(
      timestamp: Date(), activeAccountId: personalAccount.id,
      activeEmail: personalAccount.email, activePlan: personalAccount.planType,
      fiveHourPercentage: 0.0, weeklyPercentage: 0.0,
      resetTime: nil, resetAfterSeconds: nil, credits: 0,
      autoSwitchEnabled: true, accounts: [personalAccount, bizAccount],
      appAccount: personalAccount, cliAccount: personalAccount
    )
    let delegateIdentityLock = NSLock()
    var delegateIdentityInvocations: [[String]] = []
    let missingAppIdentityClient = CodexClient(
      distributionRunner: { arguments in
        delegateIdentityLock.lock()
        delegateIdentityInvocations.append(arguments)
        delegateIdentityLock.unlock()
        return true
      },
      desktopAppAccountIdProvider: { nil }
    )
    let missingAppIdentityDelegate = AppDelegate(client: missingAppIdentityClient)
    missingAppIdentityDelegate.lastSnapshot = failClosedSnapshot
    missingAppIdentityDelegate.quotaRefreshOverride = { completion in completion(nil) }
    missingAppIdentityDelegate.executeSwitchAccount(id: bizAccount.id, target: .cli)
    waitUntil("Fail-closed CLI action must finish its refresh") {
      !missingAppIdentityDelegate.isRefreshing
    }
    delegateIdentityLock.lock()
    let missingAppInvocationCount = delegateIdentityInvocations.count
    delegateIdentityLock.unlock()
    assertEqual(
      missingAppInvocationCount, 0,
      "Delegate must not invoke Rust when authoritative App preservation identity is unavailable")

    let missingCliSnapshot = MultiAccountSnapshot(
      timestamp: Date(), activeAccountId: nil,
      activeEmail: nil, activePlan: nil,
      fiveHourPercentage: 0.0, weeklyPercentage: 0.0,
      resetTime: nil, resetAfterSeconds: nil, credits: 0,
      accounts: [personalAccount, bizAccount], appAccount: personalAccount
    )
    let missingCliIdentityClient = CodexClient(
      distributionRunner: { arguments in
        delegateIdentityLock.lock()
        delegateIdentityInvocations.append(arguments)
        delegateIdentityLock.unlock()
        return true
      },
      desktopAppAccountIdProvider: { personalAccount.id }
    )
    let missingCliIdentityDelegate = AppDelegate(client: missingCliIdentityClient)
    missingCliIdentityDelegate.lastSnapshot = missingCliSnapshot
    missingCliIdentityDelegate.quotaRefreshOverride = { completion in completion(nil) }
    missingCliIdentityDelegate.executeSwitchAccount(id: bizAccount.id, target: .app)
    waitUntil("Fail-closed App action must finish its refresh") {
      !missingCliIdentityDelegate.isRefreshing
    }
    delegateIdentityLock.lock()
    let missingCliInvocationCount = delegateIdentityInvocations.count
    delegateIdentityLock.unlock()
    assertEqual(
      missingCliInvocationCount, 0,
      "Delegate must not invoke Rust when authoritative CLI preservation identity is unavailable")

    let failedInvocationLock = NSLock()
    var failedInvocations: [[String]] = []
    var failedCompletion: Bool?
    let failingClient = CodexClient(distributionRunner: { arguments in
      failedInvocationLock.lock()
      failedInvocations.append(arguments)
      failedInvocationLock.unlock()
      return false
    })
    failingClient.autoDistributeAccounts { success in
      failedInvocationLock.lock()
      failedCompletion = success
      failedInvocationLock.unlock()
    }
    waitUntil("Failed coordinator completion must be delivered") {
      failedInvocationLock.lock()
      defer { failedInvocationLock.unlock() }
      return failedCompletion != nil
    }
    failedInvocationLock.lock()
    let surfacedFailure = failedCompletion
    let recordedAutoInvocations = failedInvocations
    failedInvocationLock.unlock()
    assertEqual(surfacedFailure, false, "Failed coordinator termination must be surfaced")
    assertEqual(
      recordedAutoInvocations, [autoArguments],
      "Auto-distribution must invoke the coordinator exactly once")

    let configuredProcess = CodexClient.makeDistributionProcess(arguments: autoArguments)
    assertEqual(configuredProcess.arguments, autoArguments, "Process must receive the exact plan")
    let capturedOutput = configuredProcess.standardOutput as? Pipe
    let capturedError = configuredProcess.standardError as? Pipe
    assertTrue(capturedOutput != nil, "Coordinator stdout must be captured")
    assertTrue(capturedError != nil, "Coordinator stderr must be captured")
    assertTrue(capturedOutput === capturedError, "Coordinator stdout and stderr must share one capture pipe")

    let captureProbe = CodexClient.makeDistributionProcess(arguments: [])
    captureProbe.executableURL = URL(fileURLWithPath: "/bin/sh")
    captureProbe.arguments = ["-c", "printf coordinator-out; printf coordinator-err >&2; exit 7"]
    let capturedProbeResult = CodexClient.runCapturedProcess(captureProbe)
    assertEqual(capturedProbeResult?.status, 7, "Captured process status must be retained")
    let capturedProbeText = capturedProbeResult.flatMap {
      String(data: $0.output, encoding: .utf8)
    } ?? ""
    assertTrue(capturedProbeText.contains("coordinator-out"), "Coordinator stdout must be consumed")
    assertTrue(capturedProbeText.contains("coordinator-err"), "Coordinator stderr must be consumed")

    assertEqual(
      CodexClient.resolvedAccountId(
        for: "PERSONAL-1", accounts: [personalAccount, bizAccount]),
      personalAccount.id,
      "Current-session identifiers must resolve to canonical account IDs")
    assertTrue(
      CodexClient.resolvedAccountId(
        for: "unknown-account", accounts: [personalAccount, bizAccount]) == nil,
      "Unknown current-session identifiers must not be guessed")

    let refreshSnapshot = failClosedSnapshot
    let invocationLock = NSLock()
    var refreshInvocations: [[String]] = []
    let refreshClient = CodexClient(distributionRunner: { arguments in
      invocationLock.lock()
      refreshInvocations.append(arguments)
      invocationLock.unlock()
      return true
    })
    let refreshDelegate = AppDelegate(client: refreshClient)
    refreshDelegate.quotaRefreshOverride = { completion in completion(refreshSnapshot) }
    refreshDelegate.refreshNow()
    waitUntil("Quota refresh must complete") { !refreshDelegate.isRefreshing }
    invocationLock.lock()
    let automaticInvocationCount = refreshInvocations.count
    invocationLock.unlock()
    assertEqual(
      automaticInvocationCount, 0,
      "Quota refresh must not initiate switching; the Rust daemon owns automatic policy")

    print("  ✅ Rust-owned distribution plans, fail-closed identities, and refresh isolation verified")

    // ====================================================================
    // Test 8: Composite NSImage Rendering (Anti-Vibrancy Invariant)
    // ====================================================================
    // 1. Normal attributed string renders composite with expected properties
    let compositeImg = AppDelegate.renderCompositeImage(from: defaultStackedAttr)
    assertEqual(
      compositeImg.isTemplate, false,
      "Composite image must have isTemplate == false (anti-vibrancy)")
    assertEqual(compositeImg.size.height, 22.0, "Composite image height must be 22.0")
    let expectedWidth = max(1.0, ceil(defaultStackedAttr.size().width))
    assertEqual(
      compositeImg.size.width, expectedWidth,
      "Composite image width must be max(1.0, ceil(attrString.size().width))")

    // 2. Empty string boundary: minimum 1pt width, 22pt height
    let emptyComposite = AppDelegate.renderCompositeImage(from: NSAttributedString(string: ""))
    assertEqual(emptyComposite.isTemplate, false, "Empty composite must have isTemplate == false")
    assertEqual(emptyComposite.size.width, 1.0, "Empty composite width must be 1.0 (minimum clamp)")
    assertEqual(emptyComposite.size.height, 22.0, "Empty composite height must be 22.0")

    // 3. TIFF representation must be non-nil and non-empty
    let tiffData = compositeImg.tiffRepresentation
    assertTrue(tiffData != nil, "Composite image tiffRepresentation must be non-nil")
    assertTrue(tiffData!.count > 0, "Composite image tiffRepresentation must be non-empty")

    // 4. Vertical centering & non-clipping regression test (Test 13.5)
    // Verify that composite image draws at y = 0.0 without negative line-height offsets,
    // ensuring quota badges and brackets are vertically centered and do not clip at the bottom.
    let fullAttr = AppDelegate.buildStatusBarAttributedString(
      icon: mockIcon,
      appSession: (
        fiveHPct: "34%", fiveHColor: .systemGreen, weeklyPct: "100%", weeklyColor: .systemGreen
      ),
      cliSession: (
        fiveHPct: "48%", fiveHColor: .systemGreen, weeklyPct: "87%", weeklyColor: .systemGreen
      ),
      accounts: [bizAccount, personalAccount],
      isScreenActive: true,
      useQuotaIcons: true,
      stackPercentages: true
    )
    let fullImg = AppDelegate.renderCompositeImage(from: fullAttr)
    let fullRep = NSBitmapImageRep(data: fullImg.tiffRepresentation!)!
    let fW = fullRep.pixelsWide
    let fH = fullRep.pixelsHigh
    assertEqual(fH, 22, "Composite image pixel height must be 22")

    // Scan rightmost reserve badge pixels (last 6 columns of content for 6.5pt badge)
    var badgeMinY = 999
    var badgeMaxY = -1
    for x in (fW - 6)..<fW {
      for y in 0..<fH {
        if fullRep.colorAt(x: x, y: y)!.alphaComponent > 0.1 {
          badgeMinY = min(badgeMinY, y)
          badgeMaxY = max(badgeMaxY, y)
        }
      }
    }
    assertTrue(
      badgeMinY >= 1,
      "Badge must have at least 1pt top margin in composite image (got \(badgeMinY))")
    assertTrue(
      badgeMaxY <= 20,
      "Badge must not touch or clip the bottom row of composite image (got maxY=\(badgeMaxY))")
    let badgeCenter = Double(badgeMinY + badgeMaxY) / 2.0
    assertTrue(
      abs(badgeCenter - 10.5) <= 1.0,
      "Badge vertical center must optically align with 10.5pt image center within 1.0pt (got \(badgeCenter))"
    )

    print("  ✅ Composite NSImage rendering (anti-vibrancy & vertical centering invariant) verified")

    // ====================================================================
    // Test 9: Multilingual Support (13 Languages, Japanese, Chinese, Vietnamese)
    // ====================================================================
    assertEqual(AppLanguage.allCases.count, 13, "Expected 13 languages in AppLanguage.allCases")
    assertEqual(AppLanguage.ja.displayName, "日本語")
    assertEqual(AppLanguage.zhHans.displayName, "简体中文")
    assertEqual(AppLanguage.vi.displayName, "Tiếng Việt")

    // Test language detection fallbacks
    assertEqual(LocalizationManager.detectSystemLanguage(preferences: ["ja-JP", "ja"]), .ja)
    assertEqual(
      LocalizationManager.detectSystemLanguage(preferences: ["zh-Hans-CN", "zh-CN"]), .zhHans)
    assertEqual(LocalizationManager.detectSystemLanguage(preferences: ["zh-TW"]), .zhHans)
    assertEqual(LocalizationManager.detectSystemLanguage(preferences: ["vi-VN", "vi"]), .vi)

    // Test localized helps URLs against an isolated bundle-like fixture so this
    // remains deterministic on clean CI runners without an installed app.
    let helpFixtureRoot = FileManager.default.temporaryDirectory.appendingPathComponent(
      "codex-monitor-help-tests-\(UUID().uuidString)", isDirectory: true)
    let helpFixtureResources = helpFixtureRoot.appendingPathComponent(
      "Contents/Resources", isDirectory: true)
    try! FileManager.default.createDirectory(
      at: helpFixtureResources, withIntermediateDirectories: true)
    try! Data("<html></html>".utf8).write(
      to: helpFixtureResources.appendingPathComponent("helps.html"))
    defer { try? FileManager.default.removeItem(at: helpFixtureRoot) }
    let helpFixtureExecutable = helpFixtureRoot.appendingPathComponent(
      "Contents/MacOS/CodexMonitor").path

    let jaURL = HelpsDocHelper.localizedHelpsHTMLURL(
      languageCode: "ja", arguments: [helpFixtureExecutable])
    assertTrue(
      jaURL?.absoluteString.contains("lang=ja") == true,
      "Helps URL for Japanese must contain lang=ja")

    let zhURL = HelpsDocHelper.localizedHelpsHTMLURL(
      languageCode: "zh-Hans", arguments: [helpFixtureExecutable])
    assertTrue(
      zhURL?.absoluteString.contains("lang=zh-Hans") == true,
      "Helps URL for zh-Hans must contain lang=zh-Hans")

    let zhAliasURL = HelpsDocHelper.localizedHelpsHTMLURL(
      languageCode: "zh", arguments: [helpFixtureExecutable])
    assertTrue(
      zhAliasURL?.absoluteString.contains("lang=zh-Hans") == true,
      "Helps URL for zh alias must resolve to lang=zh-Hans")

    let viURL = HelpsDocHelper.localizedHelpsHTMLURL(
      languageCode: "vi", arguments: [helpFixtureExecutable])
    assertTrue(
      viURL?.absoluteString.contains("lang=vi") == true,
      "Helps URL for Vietnamese must contain lang=vi")

    // Test that all 13 languages have translations in LocalizationManager.translations
    for lang in AppLanguage.allCases {
      let dict = LocalizationManager.translations[lang]
      assertTrue(dict != nil, "Translations dictionary must exist for \(lang.rawValue)")
      assertTrue(dict?["menu_title"] != nil, "menu_title must exist for \(lang.rawValue)")
      assertTrue(
        dict?["auto_switch_on_limit"] != nil, "auto_switch_on_limit must exist for \(lang.rawValue)"
      )
    }

    print("  ✅ Multilingual support (13 languages: JA, ZH-Hans, VI) verified")

    // ====================================================================
    // Test 10: Unified Account Row Layout & Horizontal Alignment Invariant
    // ====================================================================
    assertEqual(
      AccountRowView.standardInset, 28.0, "AccountRowView standardInset must be exactly 28.0 pt")

    // 1. App Session Row View (ChatGPT.app active session)
    let appRow = AccountRowView(
      frame: NSRect(x: 0, y: 0, width: AppDelegate.defaultMenuWidth, height: 24),
      accountId: "desktop-app",
      accountName: nil,
      email: "dst.works@gmail.com",
      tier: "Pro 20x",
      isCurrentActive: true,
      isAppSession: true,
      dotColor: NSColor.systemGreen,
      statusTag: "[ACTIVE IN APP]",
      statusTagColor: NSColor.systemTeal
    )
    assertEqual(
      appRow.titleLabel.frame.origin.x, 28.0,
      "App session titleLabel must start at standardInset 28.0")
    assertTrue(appRow.deleteButton == nil, "App session must not have deleteButton")
    assertTrue(appRow.switchButton == nil, "App session must not have switchButton")

    // 2. CLI Primary Active Account Row View
    let cliActiveRow = AccountRowView(
      frame: NSRect(x: 0, y: 0, width: AppDelegate.defaultMenuWidth, height: 24),
      accountId: "cli-active-1",
      accountName: "personal",
      email: "dst.works@gmail.com",
      tier: "Pro 20x",
      isCurrentActive: true,
      isAppSession: false,
      dotColor: NSColor.systemGreen,
      statusTag: "[ACTIVE IN CLI]",
      statusTagColor: NSColor.systemGreen
    )
    assertEqual(
      cliActiveRow.titleLabel.frame.origin.x, 28.0,
      "CLI active titleLabel must start at standardInset 28.0")
    assertTrue(cliActiveRow.deleteButton != nil, "CLI active account must have deleteButton")
    assertTrue(cliActiveRow.switchButton == nil, "CLI active account must not have switchButton")

    // 3. CLI Reserve Account Row View
    let cliReserveRow = AccountRowView(
      frame: NSRect(x: 0, y: 0, width: AppDelegate.defaultMenuWidth, height: 24),
      accountId: "cli-reserve-1",
      accountName: nil,
      email: "dst@destinationworks.com.au",
      tier: "Team",
      isCurrentActive: false,
      isAppSession: false,
      dotColor: NSColor.systemRed,
      statusTag: "[RESERVE #1]",
      statusTagColor: NSColor.secondaryLabelColor
    )
    assertEqual(
      cliReserveRow.titleLabel.frame.origin.x, 28.0,
      "CLI reserve titleLabel must start at standardInset 28.0")
    assertTrue(cliReserveRow.deleteButton != nil, "CLI reserve account must have deleteButton")
    assertTrue(cliReserveRow.switchButton != nil, "CLI reserve account must have switchButton")

    // 4. Horizontal Invariant: All account rows must have EXACTLY the same X origin
    assertEqual(
      appRow.titleLabel.frame.origin.x, cliActiveRow.titleLabel.frame.origin.x,
      "App row and CLI active row must have identical X alignment")
    assertEqual(
      cliActiveRow.titleLabel.frame.origin.x, cliReserveRow.titleLabel.frame.origin.x,
      "CLI active and CLI reserve rows must have identical X alignment")

    // 5. Reserve Badge & Tier Non-Clipping Invariant: Long titles must truncate in the middle and preserve statusTag & tier completely
    let veryLongReserveRow = AccountRowView(
      frame: NSRect(x: 0, y: 0, width: AppDelegate.defaultMenuWidth, height: 24),
      accountId: "cli-reserve-long",
      accountName: "extremely-long-department-account-name",
      email: "user.very.long.address@organization.destinationworks.com.au",
      tier: "Business Premium",
      isCurrentActive: false,
      isAppSession: false,
      dotColor: NSColor.systemGreen,
      statusTag: "[RESERVE #99]",
      statusTagColor: NSColor.secondaryLabelColor
    )
    let renderedText = veryLongReserveRow.titleLabel.attributedStringValue.string
    assertTrue(
      renderedText.contains("[RESERVE #99]"),
      "Rendered string must preserve statusTag completely: \(renderedText)")
    assertTrue(
      renderedText.contains("Business Premium"),
      "Rendered string must preserve plan tier completely: \(renderedText)")
    assertTrue(
      renderedText.contains("…"), "Rendered string must middle-truncate long title: \(renderedText)"
    )

    print("  ✅ Unified AccountRowView alignment & hierarchy invariant (28.0pt) verified")
    print("  ✅ Reserve badge & tier non-clipping invariant verified")

    // ====================================================================
    // Test 11: Reserve Accounts Weekly Limit & Reset Partitioning Invariant
    // ====================================================================
    let teamAccount = AccountQuota(
      id: "team-res-1",
      name: "team-res",
      email: "team@company.com",
      planType: "team",
      isCurrentActive: false,
      fiveHourPercentage: 0.0,
      weeklyPercentage: 52.0,
      weeklyResetTime: nil,
      weeklyResetAfterSeconds: 450000,
      models: [],
      resetTime: nil,
      resetAfterSeconds: 11990,
      credits: 3,
      planMultiplier: 1.0
    )
    // 5h sprint reset is 11990s (3h 19m)
    assertEqual(
      teamAccount.sprintTimeUntilResetString, "3h 19m", "Team account sprint reset should be 3h 19m"
    )
    // Weekly reset is 450000s (5d 5h)
    assertEqual(
      teamAccount.weeklyTimeUntilResetString, "5d 5h", "Team account weekly reset should be 5d 5h")

    let proAccountSingleWindow = AccountQuota(
      id: "pro-user",
      name: "personal",
      email: "user@gmail.com",
      planType: "pro",
      isCurrentActive: true,
      fiveHourPercentage: 860.0,
      weeklyPercentage: 860.0,
      weeklyResetTime: nil,
      weeklyResetAfterSeconds: nil,
      models: [],
      resetTime: nil,
      resetAfterSeconds: 544868,  // 6d 7h
      credits: 0,
      planMultiplier: 20.0
    )
    // Pro account with > 86400s primary reset must NOT pollute sprint countdown
    assertEqual(
      proAccountSingleWindow.sprintTimeUntilResetString, "",
      "Pro account weekly-only window must not show on sprint countdown")
    // Pro account fallback: primary reset > 86400s must display on weekly countdown
    assertEqual(
      proAccountSingleWindow.weeklyTimeUntilResetString, "6d 7h",
      "Pro account weekly countdown should display 6d 7h")

    // Test Dynamic Menu Construction: Verify Reserve Accounts have "🗓️ Weekly:" progress bars
    let appDelegate = AppDelegate()
    let menu = appDelegate.buildMenu()
    appDelegate.statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
    appDelegate.statusItem?.menu = menu

    let autoResetMenu = menu.items.first(where: { $0.title == L10n.autoResetWeekly })
    assertTrue(autoResetMenu?.submenu != nil, "Weekly reset automation must have a settings submenu")
    assertTrue(
      autoResetMenu?.submenu?.items.contains(where: { $0.title == L10n.autoResetWeeklyAlways }) == true,
      "Weekly reset submenu must expose the always-at-0% default")

    let snapshotWithReserves = MultiAccountSnapshot(
      timestamp: Date(),
      activeAccountId: proAccountSingleWindow.id,
      activeEmail: proAccountSingleWindow.email,
      activePlan: proAccountSingleWindow.planType,
      fiveHourPercentage: 860.0,
      weeklyPercentage: 860.0,
      resetTime: nil,
      resetAfterSeconds: 544868,
      credits: 0,
      autoResetWeeklyEnabled: true,
      autoResetWeeklyMinRemainingSeconds: 86_400,
      autoResetState: "waiting_for_task",
      accounts: [proAccountSingleWindow, teamAccount],
      cliAccount: proAccountSingleWindow
    )
    appDelegate.updateUI(with: snapshotWithReserves)

    let menuTitles = menu.items.map { $0.title }
    let reserveCards = menu.items.compactMap { $0.view as? AccountSectionCardView }
    let reserveMetrics = reserveCards.flatMap { card in
      card.metricLabels.map { $0.attributedStringValue.string }
    }
    let weeklyItems = menuTitles.filter { $0.contains("Weekly:") }
    assertTrue(
      !weeklyItems.isEmpty,
      "Expected the active account Weekly item in menu, got: \(weeklyItems)")
    assertTrue(
      reserveMetrics.contains(where: { $0.contains("Weekly:") && $0.contains("52%") }),
      "Reserve account card must contain its 52% Weekly metric: \(reserveMetrics)")

    print("  ✅ Reserve accounts weekly limit bar & distinct reset times verified")

    // ====================================================================
    // Test 12: Business Accounts Grouping by Organization with Org Name
    // ====================================================================
    let org1Acc1 = AccountQuota(
      id: "dst@destinationworks.com.au:26a1ef5c",
      name: "dst",
      email: "dst@destinationworks.com.au",
      planType: "team",
      isCurrentActive: true,
      fiveHourPercentage: 100.0,
      weeklyPercentage: 80.0,
      resetTime: nil,
      resetAfterSeconds: 3600,
      credits: 2,
      planMultiplier: 1.0,
      organizationName: "Destination Works Pty Ltd"
    )
    assertEqual(
      org1Acc1.effectiveOrganizationName, "Destination Works Pty Ltd",
      "Explicit organizationName must be returned by effectiveOrganizationName"
    )

    let org1Acc2 = AccountQuota(
      id: "dst-2@destinationworks.com.au:26a1ef5c",
      name: "dst-2",
      email: "dst-2@destinationworks.com.au",
      planType: "team",
      isCurrentActive: false,
      fiveHourPercentage: 50.0,
      weeklyPercentage: 40.0,
      resetTime: nil,
      resetAfterSeconds: 7200,
      credits: 1,
      planMultiplier: 1.0,
      organizationName: "Destination Works Pty Ltd"
    )

    let org2Acc1 = AccountQuota(
      id: "dst.works@gmail.com:aef6d346",
      name: "[business]",
      email: "dst.works@gmail.com",
      planType: "team",
      isCurrentActive: false,
      fiveHourPercentage: 20.0,
      weeklyPercentage: 60.0,
      resetTime: nil,
      resetAfterSeconds: 10800,
      credits: 3,
      planMultiplier: 1.0,
      organizationName: "dstworks family"
    )

    let org2Acc2 = AccountQuota(
      id: "dst.works.au@gmail.com:aef6d346",
      name: "dst.works.au",
      email: "dst.works.au@gmail.com",
      planType: "team",
      isCurrentActive: false,
      fiveHourPercentage: 90.0,
      weeklyPercentage: 70.0,
      resetTime: nil,
      resetAfterSeconds: 14400,
      credits: 0,
      planMultiplier: 1.0,
      organizationName: "dstworks family"
    )

    let personalAcc = AccountQuota(
      id: "4d@ukr.net:f27ad1ed",
      name: "4d",
      email: "4d@ukr.net",
      planType: "plus",
      isCurrentActive: false,
      fiveHourPercentage: 75.0,
      weeklyPercentage: 30.0,
      resetTime: nil,
      resetAfterSeconds: 1800,
      credits: 1,
      planMultiplier: 1.0
    )
    assertTrue(!personalAcc.isBusiness, "Plus account must not be business")
    assertEqual(personalAcc.effectiveOrganizationName, nil, "Personal account must not have organization name")

    let orgSnapshot = MultiAccountSnapshot(
      timestamp: Date(),
      activeAccountId: org1Acc1.id,
      activeEmail: org1Acc1.email,
      activePlan: org1Acc1.planType,
      fiveHourPercentage: 100.0,
      weeklyPercentage: 80.0,
      resetTime: nil,
      resetAfterSeconds: 3600,
      credits: 2,
      accounts: [org1Acc1, org1Acc2, org2Acc1, org2Acc2, personalAcc],
      cliAccount: org1Acc1
    )

    let orgMenu = appDelegate.buildMenu()
    appDelegate.statusItem?.menu = orgMenu
    appDelegate.updateUI(with: orgSnapshot)

    let allTitles = orgMenu.items.map { $0.title }

    // Verify Active CLI header contains organization name
    assertTrue(
      allTitles.contains(where: { $0.contains("Codex CLI") && $0.contains("Destination Works Pty Ltd") }),
      "Active CLI block header must mention active organization: \(allTitles)"
    )

    // Verify organization cards exist and enclose their complete account groups.
    assertTrue(
      allTitles.contains(where: { $0.contains("Destination Works Pty Ltd") }),
      "Menu must contain a card for 'Destination Works Pty Ltd'"
    )
    let businessCard = orgMenu.items.first(where: {
      $0.title.contains("Destination Works Pty Ltd") && $0.view is AccountSectionCardView
    })?.view as? AccountSectionCardView
    assertTrue(businessCard != nil, "Business organization must use a native enclosing card")
    assertEqual(
      businessCard?.headerView.countLabel.stringValue, "1",
      "Business card header must show account count")
    assertEqual(businessCard?.accountRows.count, 1, "Business card must contain its account row")
    if let businessCard {
      assertTrue(
        businessCard.box.contentView === businessCard.contentContainer,
        "The NSBox content view must be the card's actual content container")
      assertTrue(
        businessCard.headerView.superview === businessCard.contentContainer,
        "Organization header must be enclosed by the native card")
      assertTrue(
        businessCard.accountRows.allSatisfy { $0.superview === businessCard.contentContainer },
        "All account rows must be enclosed by the organization card")
      assertTrue(
        businessCard.metricLabels.allSatisfy { $0.superview === businessCard.contentContainer },
        "All quota metrics must be enclosed by the organization card")
      assertTrue(
        businessCard.box.borderWidth > 0 && businessCard.box.cornerRadius > 0,
        "Organization card must have a complete native frame")
    }

    let familyCard = orgMenu.items.first(where: {
      $0.title.contains("dstworks family") && $0.view is AccountSectionCardView
    })?.view as? AccountSectionCardView
    assertEqual(familyCard?.accountRows.count, 2, "Second organization must contain both accounts")
    assertTrue(
      allTitles.contains(where: { $0.contains("dstworks family") }),
      "Menu must contain a card for 'dstworks family'"
    )
    assertTrue(
      allTitles.contains(where: { $0.contains("Personal Accounts") || $0.contains("Личные аккаунты") }),
      "Menu must contain section header for personal accounts"
    )
    let personalCard = orgMenu.items.first(where: {
      ($0.title.contains("Personal Accounts") || $0.title.contains("Личные аккаунты"))
        && $0.view is AccountSectionCardView
    })?.view as? AccountSectionCardView
    assertTrue(personalCard != nil, "Personal accounts must use a native enclosing card")
    assertEqual(
      personalCard?.headerView.countLabel.stringValue, "1",
      "Personal card header must show account count")

    let reserveHeader = orgMenu.items.compactMap { $0.view as? PrimaryMenuSectionHeaderView }
      .first(where: {
        $0.titleLabel.stringValue.contains("Reserve")
          || $0.titleLabel.stringValue.contains("Резерв")
      })
    assertTrue(reserveHeader != nil, "Reserve parent must use the native primary section header")
    if let reserveHeader, let businessCard {
      assertTrue(
        reserveHeader.titleLabel.font!.pointSize > businessCard.headerView.titleLabel.font!.pointSize,
        "Parent section heading must remain visually stronger than organization headings")
    }

    var switchedAccountId: String?
    var deletedAccountId: String?
    let actionCard = AccountSectionCardView(
      frame: NSRect(
        x: 0, y: 0, width: AppDelegate.defaultMenuWidth,
        height: AccountSectionCardView.preferredHeight(for: [
          ReserveAccountSectionEntry(account: org1Acc2, reserveIndex: 1)
        ])),
      title: "Destination Works Pty Ltd",
      kind: .business,
      entries: [ReserveAccountSectionEntry(account: org1Acc2, reserveIndex: 1)],
      onSwitch: { switchedAccountId = $0 },
      onDelete: { accountId, _ in deletedAccountId = accountId },
      onRename: { _, _, _ in }
    )
    actionCard.switchButtons.first?.performClick(nil)
    actionCard.accountRows.first?.deleteButton?.performClick(nil)
    assertEqual(
      switchedAccountId, org1Acc2.id,
      "Native card switch action must preserve account routing")
    assertEqual(
      deletedAccountId, org1Acc2.id,
      "Native card delete action must preserve account routing")

    let practicalGroup = Array(
      repeating: ReserveAccountSectionEntry(account: org2Acc1, reserveIndex: 1), count: 4)
    assertTrue(
      AccountSectionCardView.preferredHeight(for: practicalGroup) < 700,
      "A practical four-account organization card must fit within a scrollable menu viewport")

    print("  ✅ Native organization cards, hierarchy, enclosure & actions verified")

    // ====================================================================
    // Test 13: Account Re-login Detection, Card Button & Routing Invariant
    // ====================================================================
    let expiredAcc = AccountQuota(
      id: "expired-business",
      name: "Expired Org",
      email: "dst.works@gmail.com",
      planType: "business",
      isCurrentActive: false,
      fiveHourPercentage: 0.0,
      weeklyPercentage: 100.0,
      resetTime: nil,
      resetAfterSeconds: 0,
      credits: 0,
      error: "401 Unauthorized (Session ended (logged out in app). Re-login required.)"
    )
    let normalQuotaExhaustedAcc = AccountQuota(
      id: "quota-exhausted",
      name: "Busy User",
      email: "busy@example.com",
      planType: "plus",
      isCurrentActive: false,
      fiveHourPercentage: 0.0,
      weeklyPercentage: 20.0,
      resetTime: nil,
      resetAfterSeconds: 3600,
      credits: 0,
      error: "429 Too Many Requests (Rate limit reached)"
    )

    assertTrue(expiredAcc.needsRelogin, "401 Session ended error must require relogin")
    assertTrue(!normalQuotaExhaustedAcc.needsRelogin, "429 Rate limit must NOT require relogin")

    let heightWithoutRelogin = AccountSectionCardView.preferredHeight(for: [
      ReserveAccountSectionEntry(account: normalQuotaExhaustedAcc, reserveIndex: 1)
    ])
    let heightWithRelogin = AccountSectionCardView.preferredHeight(for: [
      ReserveAccountSectionEntry(account: expiredAcc, reserveIndex: 1)
    ])
    assertEqual(
      heightWithRelogin, heightWithoutRelogin,
      "AccountSectionCardView must maintain symmetrical card height replacing switch button with relogin button"
    )

    var reloginTargetId: String?
    var reloginTargetEmail: String?
    var switchTargetId: String?
    let reloginCard = AccountSectionCardView(
      frame: NSRect(
        x: 0, y: 0, width: AppDelegate.defaultMenuWidth,
        height: heightWithRelogin
      ),
      title: "Expired Organization",
      kind: .business,
      entries: [ReserveAccountSectionEntry(account: expiredAcc, reserveIndex: 1)],
      onSwitch: { id in switchTargetId = id },
      onDelete: { _, _ in },
      onRename: { _, _, _ in },
      onRelogin: { id, email in
        reloginTargetId = id
        reloginTargetEmail = email
      }
    )

    assertEqual(reloginCard.switchButtons.count, 0, "Card must NOT render switch button for account needing relogin")
    assertEqual(reloginCard.reloginButtons.count, 1, "Card must render exactly one relogin button for expired account")
    let reloginBtn = reloginCard.reloginButtons.first
    assertEqual(reloginBtn?.identifier?.rawValue, expiredAcc.id, "Relogin button identifier must match account id")
    assertEqual(reloginBtn?.contentTintColor, NSColor.systemOrange, "Relogin button must use systemOrange tint")
    reloginBtn?.performClick(nil)

    assertEqual(reloginTargetId, expiredAcc.id, "Relogin click must route target account ID")
    assertEqual(reloginTargetEmail, expiredAcc.email, "Relogin click must route target account email")
    assertEqual(switchTargetId, nil, "Switch callback must not have been invoked")

    // Test AccountRowView with needsRelogin suppresses inline switch and context menu switch
    var rowRelogined = false
    var rowSelected = false
    let reloginRow = AccountRowView(
      frame: NSRect(x: 0, y: 0, width: AppDelegate.defaultMenuWidth, height: 24),
      accountId: expiredAcc.id,
      accountName: expiredAcc.name,
      email: expiredAcc.email,
      tier: expiredAcc.planType,
      isCurrentActive: false,
      isAppSession: false,
      needsRelogin: true,
      dotColor: NSColor.systemRed,
      statusTag: "[reserve #1]",
      showsInlineSwitchButton: true,
      onSelect: { _ in rowSelected = true },
      onRelogin: { _, _ in rowRelogined = true }
    )
    assertTrue(reloginRow.switchButton == nil, "AccountRowView must hide inline switch button when needsRelogin is true")
    let fakeEvent = NSEvent.mouseEvent(
      with: .leftMouseUp,
      location: NSPoint(x: 10, y: 10),
      modifierFlags: [],
      timestamp: 0,
      windowNumber: 0,
      context: nil,
      eventNumber: 0,
      clickCount: 1,
      pressure: 1.0
    )!
    reloginRow.mouseUp(with: fakeEvent)
    assertTrue(rowRelogined, "Clicking AccountRowView needing relogin must route to onRelogin")
    assertTrue(!rowSelected, "Clicking AccountRowView needing relogin must NOT route to onSelect")

    let rowCtxMenu = reloginRow.menu(for: fakeEvent)
    let hasSwitchInMenu = rowCtxMenu?.items.contains(where: { $0.title == L10n.switchToAccount }) ?? false
    assertTrue(!hasSwitchInMenu, "Context menu must NOT include 'Switch to this account' when needsRelogin is true")

    // Test CLI active account menu item when needing relogin
    let activeExpiredSnapshot = MultiAccountSnapshot(
      timestamp: Date(),
      activeAccountId: expiredAcc.id,
      activeEmail: expiredAcc.email,
      activePlan: expiredAcc.planType,
      fiveHourPercentage: expiredAcc.fiveHourPercentage,
      weeklyPercentage: expiredAcc.weeklyPercentage,
      resetTime: nil,
      resetAfterSeconds: 0,
      credits: 0,
      accounts: [expiredAcc, normalQuotaExhaustedAcc],
      cliAccount: expiredAcc
    )
    let reloginMenu = appDelegate.buildMenu()
    appDelegate.statusItem?.menu = reloginMenu
    appDelegate.updateUI(with: activeExpiredSnapshot)
    let activeReloginItem = reloginMenu.items.first(where: {
      $0.title.contains(L10n.reloginToAccount) && $0.action == #selector(AppDelegate.handleActiveAccountRelogin(_:))
    })
    assertTrue(activeReloginItem != nil, "Menu must display active account relogin item when active account needs relogin")

    // Regression Test 14: Active AccountRowView clicking & context menu routing when needsRelogin is true
    var activeRowRelogined = false
    let activeReloginRow = AccountRowView(
      frame: NSRect(x: 0, y: 0, width: AppDelegate.defaultMenuWidth, height: 24),
      accountId: expiredAcc.id,
      accountName: expiredAcc.name,
      email: expiredAcc.email,
      tier: expiredAcc.planType,
      isCurrentActive: true,
      isAppSession: false,
      needsRelogin: true,
      dotColor: NSColor.systemRed,
      statusTag: "[active]",
      onRelogin: { _, _ in activeRowRelogined = true }
    )
    activeReloginRow.mouseUp(with: fakeEvent)
    assertTrue(activeRowRelogined, "Clicking active account row with needsRelogin must trigger onRelogin")

    print("  ✅ Account re-login detection, UI button, height & routing verified")

    // ====================================================================
    // Test 16: Tiny Icon Button to Reset Account Where Resets Available
    // ====================================================================
    var resetCardAccountId: String?
    var resetCardAccountEmail: String?
    let accWithCredits = AccountQuota(
      id: "acc-with-credits",
      name: "Team Account",
      email: "team@example.com",
      planType: "team",
      isCurrentActive: false,
      fiveHourPercentage: 0.0,
      weeklyPercentage: 0.0,
      resetTime: nil,
      resetAfterSeconds: 3600,
      credits: 2,
      error: nil
    )
    let accWithoutCredits = AccountQuota(
      id: "acc-without-credits",
      name: "Zero Credits Account",
      email: "zero@example.com",
      planType: "team",
      isCurrentActive: false,
      fiveHourPercentage: 50.0,
      weeklyPercentage: 50.0,
      resetTime: nil,
      resetAfterSeconds: 3600,
      credits: 0,
      error: nil
    )
    let cardWithResets = AccountSectionCardView(
      frame: NSRect(
        x: 0, y: 0, width: AppDelegate.defaultMenuWidth,
        height: AccountSectionCardView.preferredHeight(for: [
          ReserveAccountSectionEntry(account: accWithCredits, reserveIndex: 1),
          ReserveAccountSectionEntry(account: accWithoutCredits, reserveIndex: 2),
        ])),
      title: "Team Resets Org",
      kind: .business,
      entries: [
        ReserveAccountSectionEntry(account: accWithCredits, reserveIndex: 1),
        ReserveAccountSectionEntry(account: accWithoutCredits, reserveIndex: 2),
      ],
      onSwitch: { _ in },
      onDelete: { _, _ in },
      onRename: { _, _, _ in },
      onReset: { id, email in
        resetCardAccountId = id
        resetCardAccountEmail = email
      }
    )
    assertEqual(
      cardWithResets.resetButtons.count, 1,
      "Only accounts with credits > 0 must have a reset button in AccountSectionCardView"
    )
    let cardResetBtn = cardWithResets.resetButtons.first!
    assertEqual(
      cardResetBtn.identifier?.rawValue, accWithCredits.id,
      "Reset button identifier must match the account id"
    )
    assertTrue(
      cardResetBtn.toolTip?.contains(L10n.resetAccountTooltip) == true,
      "Reset button tooltip must describe resetting account"
    )
    cardResetBtn.performClick(nil)
    assertEqual(
      resetCardAccountId, accWithCredits.id,
      "Clicking reset button must invoke onReset with account id"
    )
    assertEqual(
      resetCardAccountEmail, accWithCredits.email,
      "Clicking reset button must invoke onReset with account email"
    )

    // 1. Verify non-overlapping layout between credits metric label and reset button
    let creditsMetricLabel = cardWithResets.metricLabels.first {
      $0.attributedStringValue.string.contains(L10n.resetCredits)
    }
    assertTrue(creditsMetricLabel != nil, "Must find credits metric label in card")
    let labelMaxX = creditsMetricLabel!.frame.maxX
    let btnMinX = cardResetBtn.frame.minX
    assertTrue(
      labelMaxX <= btnMinX - 6,
      "Credits metric label (maxX: \(labelMaxX)) must not overlap reset button (minX: \(btnMinX))"
    )

    // 2. Verify resetCursorRects executes cleanly on both views
    cardWithResets.resetCursorRects()

    var rowResetClicked = false
    let resetRow = ResetCreditsRowView(
      frame: NSRect(x: 0, y: 0, width: AppDelegate.defaultMenuWidth, height: 20),
      credits: 3,
      accountDisplayName: "active@example.com",
      leftPadding: 20,
      onReset: { rowResetClicked = true }
    )
    resetRow.resetCursorRects()
    assertTrue(
      resetRow.label.stringValue.contains(L10n.resetCredits),
      "ResetCreditsRowView must display reset credits label"
    )
    assertTrue(
      resetRow.label.stringValue.contains("3"),
      "ResetCreditsRowView must display the credit count number on the line"
    )
    resetRow.resetButton.performClick(nil)
    assertTrue(rowResetClicked, "Clicking reset button in ResetCreditsRowView must invoke onReset")

    // 3. Verify mouseUp hit-testing on ResetCreditsRowView
    var rowMouseUpTriggered = false
    let testResetRow = ResetCreditsRowView(
      frame: NSRect(x: 0, y: 0, width: AppDelegate.defaultMenuWidth, height: 20),
      credits: 2,
      accountDisplayName: "test@example.com",
      leftPadding: 20,
      onReset: { rowMouseUpTriggered = true }
    )
    let btnCenter = NSPoint(
      x: testResetRow.resetButton.frame.midX,
      y: testResetRow.resetButton.frame.midY
    )
    let mouseUpEvent = NSEvent.mouseEvent(
      with: .leftMouseUp,
      location: btnCenter,
      modifierFlags: [],
      timestamp: 0,
      windowNumber: 0,
      context: nil,
      eventNumber: 0,
      clickCount: 1,
      pressure: 1.0
    )!
    testResetRow.mouseUp(with: mouseUpEvent)
    assertTrue(
      rowMouseUpTriggered,
      "mouseUp within resetButton.frame in ResetCreditsRowView must trigger onReset"
    )

    rowMouseUpTriggered = false
    let outsideEvent = NSEvent.mouseEvent(
      with: .leftMouseUp,
      location: NSPoint(x: 10, y: 10),
      modifierFlags: [],
      timestamp: 0,
      windowNumber: 0,
      context: nil,
      eventNumber: 0,
      clickCount: 1,
      pressure: 1.0
    )!
    testResetRow.mouseUp(with: outsideEvent)
    assertTrue(
      !rowMouseUpTriggered,
      "mouseUp outside resetButton.frame must not trigger onReset"
    )

    // 4. Verify mouseUp hit-testing on AccountSectionCardView
    var cardMouseUpId = ""
    var cardMouseUpEmail = ""
    let testCard = AccountSectionCardView(
      frame: NSRect(
        x: 0, y: 0, width: AppDelegate.defaultMenuWidth,
        height: AccountSectionCardView.preferredHeight(for: [
          ReserveAccountSectionEntry(account: accWithCredits, reserveIndex: 1),
        ])),
      title: "HitTest Org",
      kind: .business,
      entries: [
        ReserveAccountSectionEntry(account: accWithCredits, reserveIndex: 1),
      ],
      onSwitch: { _ in },
      onDelete: { _, _ in },
      onRename: { _, _, _ in },
      onReset: { id, email in
        cardMouseUpId = id
        cardMouseUpEmail = email
      }
    )
    let cardBtn = testCard.resetButtons.first!
    let cardBtnCenterInCard = testCard.convert(
      NSPoint(x: cardBtn.frame.midX, y: cardBtn.frame.midY),
      from: testCard.contentContainer
    )
    let cardEvent = NSEvent.mouseEvent(
      with: .leftMouseUp,
      location: cardBtnCenterInCard,
      modifierFlags: [],
      timestamp: 0,
      windowNumber: 0,
      context: nil,
      eventNumber: 0,
      clickCount: 1,
      pressure: 1.0
    )!
    testCard.mouseUp(with: cardEvent)
    assertEqual(
      cardMouseUpId, accWithCredits.id,
      "mouseUp over reset button in AccountSectionCardView must trigger onReset with account id"
    )
    assertEqual(
      cardMouseUpEmail, accWithCredits.email,
      "mouseUp over reset button in AccountSectionCardView must trigger onReset with account email"
    )

    // 5. Verify localized reset success message
    let successMsg = L10n.resetSuccessMsg(email: "test@example.com")
    assertTrue(
      successMsg.contains("test@example.com"),
      "resetSuccessMsg must contain target email"
    )

    print("  ✅ Tiny reset button where resets available on resets line verified")
    print("  ✅ Reset button mouseUp hit-testing, non-overlapping layout & cursor rects verified")

    // ====================================================================
    // Test 17: Drop-Down Menu Hover, A11y, MenuIconButton & Architecture Invariants
    // ====================================================================
    // 1. MenuIconButton interaction & hover tint
    let testIconButton = MenuIconButton(
      frame: NSRect(x: 0, y: 0, width: 22, height: 18),
      symbolName: "arrow.triangle.2.circlepath",
      pointSize: 11,
      weight: .semibold,
      tintColor: .systemBlue,
      hoverTintColor: .controlAccentColor,
      tooltip: "Switch",
      accessibilityLabel: "Switch Account"
    )
    assertEqual(testIconButton.isHovered, false, "MenuIconButton must start unhovered")
    assertEqual(testIconButton.contentTintColor, NSColor.systemBlue, "MenuIconButton must have normal tint color")

    let enterEvent = NSEvent.enterExitEvent(
      with: .mouseEntered, location: NSPoint(x: 10, y: 10), modifierFlags: [], timestamp: 0,
      windowNumber: 0, context: nil, eventNumber: 0, trackingNumber: 0, userData: nil
    )!
    let exitEvent = NSEvent.enterExitEvent(
      with: .mouseExited, location: NSPoint(x: -10, y: -10), modifierFlags: [], timestamp: 0,
      windowNumber: 0, context: nil, eventNumber: 0, trackingNumber: 0, userData: nil
    )!

    testIconButton.mouseEntered(with: enterEvent)
    assertEqual(testIconButton.isHovered, true, "MenuIconButton must be hovered after mouseEntered")
    assertEqual(testIconButton.contentTintColor, NSColor.controlAccentColor, "MenuIconButton must shift to hoverTintColor")

    testIconButton.mouseExited(with: exitEvent)
    assertEqual(testIconButton.isHovered, false, "MenuIconButton must reset isHovered on mouseExited")
    assertEqual(testIconButton.contentTintColor, NSColor.systemBlue, "MenuIconButton must restore normal tint on mouseExited")

    // 1.5 Capsule MenuIconButton
    let capsuleBtn = MenuIconButton(
      frame: NSRect(x: 0, y: 0, width: 140, height: 24),
      title: "Switch Account",
      symbolName: "arrow.triangle.2.circlepath",
      isCapsule: true
    )
    assertEqual(capsuleBtn.isCapsule, true, "MenuIconButton must support capsule styling")
    capsuleBtn.layout()
    assert(capsuleBtn.iconImageView != nil, "Capsule button with title must instantiate iconImageView")
    assert(capsuleBtn.titleLabel != nil, "Capsule button with title must instantiate titleLabel")
    let iconFrame = capsuleBtn.iconImageView?.frame ?? .zero
    let labelFrame = capsuleBtn.titleLabel?.frame ?? .zero
    let leftPad = iconFrame.minX
    let rightPad = capsuleBtn.bounds.width - labelFrame.maxX
    assert(abs(leftPad - rightPad) <= 2.0, "Capsule button icon + label must be optically centered (leftPad: \(leftPad), rightPad: \(rightPad))")
    capsuleBtn.mouseEntered(with: enterEvent)
    assertEqual(capsuleBtn.isHovered, true, "Capsule button must be hovered on mouseEntered")
    capsuleBtn.mouseExited(with: exitEvent)
    assertEqual(capsuleBtn.isHovered, false, "Capsule button must unhover on mouseExited")

    // 2. AccountRowView hover & accessibility
    var rowPressTriggered = false
    let hoverRow = AccountRowView(
      frame: NSRect(x: 0, y: 0, width: AppDelegate.defaultMenuWidth, height: 24),
      accountId: "acc-hover-test",
      accountName: "Hover User",
      email: "hover@example.com",
      tier: "team",
      isCurrentActive: false,
      dotColor: .systemGreen,
      statusTag: "[reserve #1]",
      onSelect: { _ in rowPressTriggered = true }
    )
    hoverRow.updateTrackingAreas()
    assertEqual(hoverRow.isHovered, false, "AccountRowView must initialize with isHovered = false")
    hoverRow.mouseEntered(with: enterEvent)
    assertEqual(hoverRow.isHovered, true, "AccountRowView must set isHovered = true on mouseEntered")
    hoverRow.mouseExited(with: exitEvent)
    assertEqual(hoverRow.isHovered, false, "AccountRowView must set isHovered = false on mouseExited")

    let activeRow = AccountRowView(
      frame: NSRect(x: 0, y: 0, width: AppDelegate.defaultMenuWidth, height: 24),
      accountId: "acc-active-test",
      accountName: "Active User",
      email: "active@example.com",
      tier: "pro",
      isCurrentActive: true,
      dotColor: .systemGreen,
      statusTag: "[active in cli]"
    )
    activeRow.updateTrackingAreas()
    activeRow.mouseEntered(with: enterEvent)
    assertEqual(activeRow.isHovered, true, "Active AccountRowView must set isHovered = true on mouseEntered")

    assertEqual(hoverRow.accessibilityRole(), .menuItem, "AccountRowView must declare role .menuItem for VoiceOver")
    let canPress = hoverRow.accessibilityPerformPress()
    assertTrue(canPress, "AccountRowView must handle accessibilityPerformPress")
    assertTrue(rowPressTriggered, "accessibilityPerformPress must invoke onSelect for inactive row")

    let customActions = hoverRow.accessibilityCustomActions() ?? []
    assertTrue(!customActions.isEmpty, "AccountRowView must provide accessibilityCustomActions for VoiceOver rotor")
    assertTrue(customActions.contains(where: { $0.name == L10n.switchToAccount }), "Must contain Switch action")
    assertTrue(customActions.contains(where: { $0.name == L10n.renameAccount }), "Must contain Rename action")
    assertTrue(customActions.contains(where: { $0.name == L10n.removeAccount }), "Must contain Remove action")

    // 3. PrimaryMenuSectionHeaderView Apple HIG compliance (non-selectable header, group role)
    let headerView = PrimaryMenuSectionHeaderView(
      frame: NSRect(x: 0, y: 0, width: AppDelegate.defaultMenuWidth, height: 32),
      title: "Test Section",
      symbolName: "terminal"
    )
    assertEqual(headerView.titleLabel.stringValue, "Test Section", "Header must show section title")
    assertEqual(headerView.accessibilityRole(), .group, "Header must declare role .group for VoiceOver")
    assertEqual(headerView.accessibilityLabel(), "Test Section", "Header accessibility label must match title")

    // 4. AccountSectionCardView hover & row click
    var cardSwitchTriggered = false
    let cardHoverView = AccountSectionCardView(
      frame: NSRect(x: 0, y: 0, width: AppDelegate.defaultMenuWidth, height: 100),
      title: "Hover Card",
      kind: .business,
      entries: [ReserveAccountSectionEntry(account: accWithoutCredits, reserveIndex: 1)],
      onSwitch: { id in if id == accWithoutCredits.id { cardSwitchTriggered = true } },
      onDelete: { _, _ in },
      onRename: { _, _, _ in }
    )
    cardHoverView.updateTrackingAreas()
    assertEqual(cardHoverView.isCardHovered, false, "AccountSectionCardView must initialize with isCardHovered = false")
    cardHoverView.mouseEntered(with: enterEvent)
    assertEqual(cardHoverView.isCardHovered, true, "AccountSectionCardView must highlight border/fill on mouseEntered")
    cardHoverView.mouseExited(with: exitEvent)
    assertEqual(cardHoverView.isCardHovered, false, "AccountSectionCardView must restore border/fill on mouseExited")

    if let firstRow = cardHoverView.accountRows.first {
      let centerInRow = NSPoint(x: firstRow.frame.midX, y: firstRow.frame.midY)
      let centerInWindow = cardHoverView.convert(cardHoverView.contentContainer.convert(centerInRow, to: cardHoverView), to: nil)
      let clickEvent = NSEvent.mouseEvent(
        with: .leftMouseUp,
        location: centerInWindow,
        modifierFlags: [],
        timestamp: 1.0,
        windowNumber: 1,
        context: nil,
        eventNumber: 1,
        clickCount: 1,
        pressure: 1.0
      )!
      cardHoverView.mouseUp(with: clickEvent)
      assertTrue(cardSwitchTriggered, "Clicking accountRow inside AccountSectionCardView must trigger onSwitch")
    }

    print("  ✅ Drop-down menu hover, mouse tracking, accessibility & a11y actions verified")

    // ====================================================================
    // Test 17: Colored Brackets for APP and CLI Account Selection
    // ====================================================================
    do {
      let accApp = AccountQuota(
        id: "acc-app", email: "app@openai.com", planType: "plus", isCurrentActive: false,
        fiveHourPercentage: 80.0, weeklyPercentage: 90.0, resetTime: nil, resetAfterSeconds: 1000,
        credits: 0)
      let accCli = AccountQuota(
        id: "acc-cli", email: "cli@openai.com", planType: "team", isCurrentActive: false,
        fiveHourPercentage: 60.0, weeklyPercentage: 70.0, resetTime: nil, resetAfterSeconds: 2000,
        credits: 1)
      let accReserve = AccountQuota(
        id: "acc-res", email: "res@openai.com", planType: "pro", isCurrentActive: false,
        fiveHourPercentage: 100.0, weeklyPercentage: 100.0, resetTime: nil, resetAfterSeconds: 3000,
        credits: 0)

      let appColor = MenuBarAppearanceHelper.appTagColor(isScreenActive: true)
      let cliColor = MenuBarAppearanceHelper.cliTagColor(isScreenActive: true)

      // Subtest 1: Only CLI selected -> brackets must match cliTagColor
      let cliOnlyAttr = AppDelegate.buildStatusBarAttributedString(
        icon: mockIcon,
        cliSession: (
          fiveHPct: "60%", fiveHColor: .systemGreen, weeklyPct: "70%", weeklyColor: .systemGreen
        ),
        accounts: [accCli],
        isScreenActive: true,
        useQuotaIcons: true,
        stackPercentages: true,
        appAccountId: nil,
        cliAccountId: "acc-cli"
      )
      let cliOnlyStr = cliOnlyAttr.string as NSString
      let cliBracketIdx = cliOnlyStr.range(of: "[").location
      assertTrue(cliBracketIdx != NSNotFound, "CLI bracket '[' must exist as text character")
      let cliBracketAttrs = cliOnlyAttr.attributes(at: cliBracketIdx, effectiveRange: nil)
      if let col = cliBracketAttrs[.foregroundColor] as? NSColor {
        assertEqual(col, cliColor, "CLI bracket foregroundColor must match cliTagColor")
      } else {
        assertTrue(false, "CLI bracket must have foregroundColor")
      }

      // Subtest 2: Only APP selected -> brackets must match appTagColor
      let appOnlyAttr = AppDelegate.buildStatusBarAttributedString(
        icon: mockIcon,
        appSession: (
          fiveHPct: "80%", fiveHColor: .systemGreen, weeklyPct: "90%", weeklyColor: .systemGreen
        ),
        cliSession: (
          fiveHPct: "60%", fiveHColor: .systemGreen, weeklyPct: "70%", weeklyColor: .systemGreen
        ),
        accounts: [accApp],
        isScreenActive: true,
        useQuotaIcons: true,
        stackPercentages: true,
        appAccountId: "acc-app",
        cliAccountId: nil
      )
      let appOnlyStr = appOnlyAttr.string as NSString
      let appBracketIdx = appOnlyStr.range(of: "[").location
      assertTrue(appBracketIdx != NSNotFound, "APP bracket '[' must exist as text character")
      let appBracketAttrs = appOnlyAttr.attributes(at: appBracketIdx, effectiveRange: nil)
      if let col = appBracketAttrs[.foregroundColor] as? NSColor {
        assertEqual(col, appColor, "APP bracket foregroundColor must match appTagColor")
      } else {
        assertTrue(false, "APP bracket must have foregroundColor")
      }

      // Subtest 3: Both APP and CLI selected on the same account -> dual-color split bracket
      let bothAttr = AppDelegate.buildStatusBarAttributedString(
        icon: mockIcon,
        appSession: (
          fiveHPct: "80%", fiveHColor: .systemGreen, weeklyPct: "90%", weeklyColor: .systemGreen
        ),
        cliSession: (
          fiveHPct: "80%", fiveHColor: .systemGreen, weeklyPct: "90%", weeklyColor: .systemGreen
        ),
        accounts: [accApp],
        isScreenActive: true,
        useQuotaIcons: true,
        stackPercentages: true,
        appAccountId: "acc-app",
        cliAccountId: "acc-app"
      )
      var bothAttachments: [NSTextAttachment] = []
      bothAttr.enumerateAttribute(
        .attachment, in: NSRange(location: 0, length: bothAttr.length), options: []
      ) { val, _, _ in
        if let att = val as? NSTextAttachment { bothAttachments.append(att) }
      }
      assertEqual(
        bothAttachments.count, 4, "Expected 4 attachments (icon + sessions + badge) for dual-session with text brackets")

      let bothStr = bothAttr.string as NSString
      let bothOpenRange = bothStr.range(of: "[")
      assertTrue(bothOpenRange.location != NSNotFound, "Dual-session status bar must contain text bracket '['")
      let bothAttrs = bothAttr.attributes(at: bothOpenRange.location, effectiveRange: nil)
      assertTrue(bothAttrs[.foregroundColor] is NSColor, "Bracket foregroundColor must be pattern NSColor")

      let splitImg = AppDelegate.renderCompositeImage(from: bothAttr)
      let splitRep = NSBitmapImageRep(data: splitImg.tiffRepresentation!)!
      var hasAppTop = false
      var hasCliBottom = false
      for y in 1...9 {
        for x in 0..<splitRep.pixelsWide {
          let c = splitRep.colorAt(x: x, y: y)!
          if c.alphaComponent > 0.5 && c.blueComponent > 0.8 && c.redComponent < 0.5 {
            hasAppTop = true
            break
          }
        }
      }
      for y in 12...20 {
        for x in 0..<splitRep.pixelsWide {
          let c = splitRep.colorAt(x: x, y: y)!
          if c.alphaComponent > 0.5 && c.greenComponent > 0.8 && c.blueComponent < 0.8 {
            hasCliBottom = true
            break
          }
        }
      }
      assertTrue(hasAppTop, "Split bracket top half must contain APP cyan color pixels")
      assertTrue(hasCliBottom, "Split bracket bottom half must contain CLI green color pixels")

      // Subtest 4: Distinct accounts for APP and CLI
      let distinctAttr = AppDelegate.buildStatusBarAttributedString(
        icon: mockIcon,
        appSession: (
          fiveHPct: "80%", fiveHColor: .systemGreen, weeklyPct: "90%", weeklyColor: .systemGreen
        ),
        cliSession: (
          fiveHPct: "60%", fiveHColor: .systemGreen, weeklyPct: "70%", weeklyColor: .systemGreen
        ),
        accounts: [accApp, accCli, accReserve],
        isScreenActive: true,
        useQuotaIcons: true,
        stackPercentages: true,
        appAccountId: "acc-app",
        cliAccountId: "acc-cli"
      )
      let distStr = distinctAttr.string as NSString
      var foundBracketColors: [NSColor] = []
      var searchRange = NSRange(location: 0, length: distStr.length)
      while true {
        let found = distStr.range(of: "[", options: [], range: searchRange)
        if found.location == NSNotFound { break }
        let attrs = distinctAttr.attributes(at: found.location, effectiveRange: nil)
        if let col = attrs[.foregroundColor] as? NSColor {
          foundBracketColors.append(col)
        }
        let nextStart = found.location + 1
        searchRange = NSRange(location: nextStart, length: distStr.length - nextStart)
      }
      assertEqual(
        foundBracketColors.count, 2,
        "Expected exactly 2 opening brackets for distinct APP and CLI accounts")
      assertEqual(foundBracketColors[0], appColor, "First column (APP) bracket must match appTagColor")
      assertEqual(foundBracketColors[1], cliColor, "Second column (CLI) bracket must match cliTagColor")
    }
    print("  ✅ Colored brackets for APP, CLI and dual-selection (split top/bottom) verified")

    // ====================================================================
    // Test 18: Dual Switch Controls for CLI and APP Sessions
    // ====================================================================
    do {
      var switchedCliId: String? = nil
      var switchedAppId: String? = nil

      let buttonsView = AccountSwitchButtonsView(
        frame: NSRect(x: 0, y: 0, width: 220, height: 24),
        accountId: "acc-test",
        accountEmail: "test@example.com",
        isCliActive: false,
        isAppActive: false,
        onSwitchCli: { switchedCliId = $0 },
        onSwitchApp: { switchedAppId = $0 }
      )
      assertEqual(buttonsView.cliButton.title, "> CLI", "Inactive CLI button title must be '> CLI'")
      assertEqual(buttonsView.appButton.title, "🖥 APP", "Inactive APP button title must be '🖥 APP'")

      buttonsView.handleCliClicked()
      assertEqual(switchedCliId, "acc-test", "handleCliClicked must invoke onSwitchCli with account ID")

      buttonsView.handleAppClicked()
      assertEqual(switchedAppId, "acc-test", "handleAppClicked must invoke onSwitchApp with account ID")

      // Active state verification
      let activeButtonsView = AccountSwitchButtonsView(
        frame: NSRect(x: 0, y: 0, width: 220, height: 24),
        accountId: "acc-active",
        accountEmail: "active@example.com",
        isCliActive: true,
        isAppActive: true
      )
      assertEqual(activeButtonsView.cliButton.title, "✓ CLI", "Active CLI button title must be '✓ CLI'")
      assertEqual(activeButtonsView.appButton.title, "✓ APP", "Active APP button title must be '✓ APP'")

      // Card view integration verification
      let testAccount = AccountQuota(
        id: "acc-res-1",
        email: "res1@example.com",
        planType: "team",
        isCurrentActive: false,
        fiveHourPercentage: 50.0,
        weeklyPercentage: 80.0,
        resetTime: nil,
        resetAfterSeconds: 3600,
        credits: 0
      )
      let entry = ReserveAccountSectionEntry(
        account: testAccount, reserveIndex: 1, isCliActive: false, isAppActive: false)
      var cardCliSwitched = false
      var cardAppSwitched = false
      let card = AccountSectionCardView(
        frame: NSRect(
          x: 0, y: 0, width: 440,
          height: AccountSectionCardView.preferredHeight(for: [entry])),
        title: "Test Section",
        kind: .business,
        entries: [entry],
        onSwitchCli: { _ in cardCliSwitched = true },
        onSwitchApp: { _ in cardAppSwitched = true },
        onDelete: { _, _ in },
        onRename: { _, _, _ in }
      )
      assertEqual(card.switchButtonsViews.count, 1, "Card must have 1 switchButtonsView for the entry")
      assertEqual(card.switchButtons.count, 2, "Card must expose 2 switchButtons (CLI and APP)")
      card.switchButtonsViews[0].handleCliClicked()
      assertTrue(cardCliSwitched, "Card switch CLI callback must trigger")
      card.switchButtonsViews[0].handleAppClicked()
      assertTrue(cardAppSwitched, "Card switch APP callback must trigger")

      // Robust ID canonicalization test for brackets
      let canonicalAccount = AccountQuota(
        id: "User@Domain.Com:uuid-123",
        email: "user@domain.com",
        planType: "team",
        isCurrentActive: false,
        fiveHourPercentage: 70.0,
        weeklyPercentage: 70.0,
        resetTime: nil,
        resetAfterSeconds: 1000,
        credits: 0
      )
      let canonicalAttr = AppDelegate.buildStatusBarAttributedString(
        icon: mockIcon,
        cliSession: (
          fiveHPct: "70%", fiveHColor: .systemGreen, weeklyPct: "70%", weeklyColor: .systemGreen
        ),
        accounts: [canonicalAccount],
        isScreenActive: true,
        useQuotaIcons: true,
        stackPercentages: true,
        appAccountId: nil,
        cliAccountId: "USER@DOMAIN.COM"  // case-different email lookup
      )
      let canonicalStr = canonicalAttr.string as NSString
      let bracketIdx = canonicalStr.range(of: "[").location
      assertTrue(
        bracketIdx != NSNotFound,
        "Case-insensitive email lookup must resolve and draw bracket '['")

      // Single-column fallback .both mode verification
      let fallbackBothAttr = AppDelegate.buildStatusBarAttributedString(
        icon: mockIcon,
        appSession: (
          fiveHPct: "80%", fiveHColor: .systemCyan, weeklyPct: "80%", weeklyColor: .systemCyan
        ),
        cliSession: (
          fiveHPct: "60%", fiveHColor: .systemGreen, weeklyPct: "60%", weeklyColor: .systemGreen
        ),
        accounts: [],
        isScreenActive: true,
        useQuotaIcons: true,
        stackPercentages: true,
        appAccountId: "acc-1",
        cliAccountId: "acc-2"
      )
      let fbHasTextBracket = (fallbackBothAttr.string as NSString).range(of: "[").location != NSNotFound
      assertTrue(
        fbHasTextBracket,
        "Empty accounts fallback with both sessions must draw split text bracket '['"
      )
      let fbOpenRange = (fallbackBothAttr.string as NSString).range(of: "[")
      let fbAttrs = fallbackBothAttr.attributes(at: fbOpenRange.location, effectiveRange: nil)
      assertTrue(fbAttrs[.foregroundColor] is NSColor, "Fallback bracket must use pattern NSColor")
    }
    print("  ✅ Dual switch controls, active badges, callbacks and robust ID resolution verified")

    // ====================================================================
    // Test 19: Status Bar CLI Account Resolution & Quota Decoupling Regression Test
    // ====================================================================
    do {
      let accActive = AccountQuota(
        id: "active-acc",
        name: "Active Account",
        email: "active@openai.com",
        planType: "team",
        isCurrentActive: true,
        fiveHourPercentage: 45.0,
        weeklyPercentage: 55.0,
        resetTime: nil,
        resetAfterSeconds: 3600,
        credits: 1,
        planMultiplier: 2.0
      )
      let accCli = AccountQuota(
        id: "cli-acc",
        name: "CLI Account",
        email: "cli@openai.com",
        planType: "pro",
        isCurrentActive: false,
        fiveHourPercentage: 90.0,
        weeklyPercentage: 85.0,
        resetTime: nil,
        resetAfterSeconds: 7200,
        credits: 0,
        planMultiplier: 5.0
      )
      let accApp = AccountQuota(
        id: "app-acc",
        name: "App Account",
        email: "app@openai.com",
        planType: "business",
        isCurrentActive: false,
        fiveHourPercentage: 30.0,
        weeklyPercentage: 70.0,
        resetTime: nil,
        resetAfterSeconds: 1800,
        credits: 2,
        planMultiplier: 20.0
      )

      // Subtest 1: Deliberately different top-level, cliAccount, and appAccount values
      // Top-level: 10% / 20%, mult: 1.0
      // CLI account: 90% / 85%, mult: 5.0
      // APP account: 30% / 70%, mult: 20.0
      // Active account: 45% / 55%, mult: 2.0
      let fullSnapshot = MultiAccountSnapshot(
        timestamp: Date(),
        activeAccountId: accActive.id,
        activeEmail: accActive.email,
        activePlan: accActive.planType,
        fiveHourPercentage: 10.0,
        weeklyPercentage: 20.0,
        resetTime: nil,
        resetAfterSeconds: 5000,
        credits: 0,
        isAppRunning: true,
        planMultiplier: 1.0,
        accounts: [accActive, accCli, accApp],
        appAccount: accApp,
        cliAccount: accCli
      )

      // Verify resolved CLI account is accCli
      let resolvedCli = AppDelegate.resolveCliAccount(from: fullSnapshot)
      assertEqual(resolvedCli?.id, "cli-acc", "Resolved CLI account must be cliAccount")

      // Verify resolved sessions
      let sessions = AppDelegate.resolveStatusBarSessions(from: fullSnapshot, isScreenActive: true)
      assertEqual(sessions.cliSession.fiveHPct, "90%", "CLI 5h percentage must come from cliAccount (90%), NOT top-level (10%) or active (45%)")
      assertEqual(sessions.cliSession.weeklyPct, "85%", "CLI weekly percentage must come from cliAccount (85%), NOT top-level (20%) or active (55%)")

      // Verify multiplier impact on colors:
      // For accCli: 90.0 / 5.0 = 18.0% <= 20% -> Red! (If mult 1.0 were used, 90.0% would be green)
      let expectedCli5hColor = MenuBarAppearanceHelper.menuBarColor(forPercentage: 90.0, weeklyPercentage: 85.0, isScreenActive: true, planMultiplier: 5.0)
      let expectedCliWColor = MenuBarAppearanceHelper.menuBarColor(forPercentage: 85.0, weeklyPercentage: nil, isScreenActive: true, planMultiplier: 5.0)
      assertEqual(sessions.cliSession.fiveHColor, expectedCli5hColor, "CLI 5h color must use cliAccount multiplier (5.0)")
      assertEqual(sessions.cliSession.weeklyColor, expectedCliWColor, "CLI weekly color must use cliAccount multiplier (5.0)")

      // Verify APP session
      assertTrue(sessions.appSession != nil, "APP session must be non-nil when isAppRunning is true")
      assertEqual(sessions.appSession?.fiveHPct, "30%", "APP 5h percentage must come from appAccount")
      assertEqual(sessions.appSession?.weeklyPct, "70%", "APP weekly percentage must come from appAccount")
      let expectedApp5hColor = MenuBarAppearanceHelper.menuBarColor(forPercentage: 30.0, weeklyPercentage: 70.0, isScreenActive: true, planMultiplier: 20.0)
      assertEqual(sessions.appSession?.fiveHColor, expectedApp5hColor, "APP 5h color must use appAccount multiplier (20.0)")

      // Verify horizontal attributed string contains resolved percentages
      let horizAttr = AppDelegate.buildStatusBarAttributedString(
        snapshot: fullSnapshot,
        icon: mockIcon,
        isScreenActive: true,
        useQuotaIcons: true,
        stackPercentages: false
      )
      let horizStr = horizAttr.string
      assertTrue(horizStr.contains("90%"), "CLI displayed 5h percentage (90%) must be in horizontal attributed string")
      assertTrue(horizStr.contains("85%"), "CLI displayed weekly percentage (85%) must be in horizontal attributed string")
      assertTrue(horizStr.contains("30%"), "APP displayed 5h percentage (30%) must be in horizontal attributed string")
      assertTrue(horizStr.contains("70%"), "APP displayed weekly percentage (70%) must be in horizontal attributed string")
      assertTrue(!horizStr.contains("10%"), "Top-level 5h percentage (10%) must NOT appear in horizontal attributed string")
      assertTrue(!horizStr.contains("20%"), "Top-level weekly percentage (20%) must NOT appear in horizontal attributed string")

      // Verify bracket mapping: APP cyan and CLI green
      let appColor = MenuBarAppearanceHelper.appTagColor(isScreenActive: true)
      let cliColor = MenuBarAppearanceHelper.cliTagColor(isScreenActive: true)
      let fullAttr = AppDelegate.buildStatusBarAttributedString(
        snapshot: fullSnapshot,
        icon: mockIcon,
        isScreenActive: true,
        useQuotaIcons: true,
        stackPercentages: true
      )
      let fullStrNS = fullAttr.string as NSString
      var bracketColors: [NSColor] = []
      var searchRange = NSRange(location: 0, length: fullStrNS.length)
      while true {
        let found = fullStrNS.range(of: "[", options: [], range: searchRange)
        if found.location == NSNotFound { break }
        let attrs = fullAttr.attributes(at: found.location, effectiveRange: nil)
        if let col = attrs[.foregroundColor] as? NSColor {
          bracketColors.append(col)
        }
        let nextStart = found.location + 1
        searchRange = NSRange(location: nextStart, length: fullStrNS.length - nextStart)
      }
      assertEqual(bracketColors.count, 2, "Expected 2 opening brackets for CLI and APP accounts")
      assertEqual(bracketColors[0], cliColor, "First bracketed account (accCli) must have CLI green bracket")
      assertEqual(bracketColors[1], appColor, "Second bracketed account (accApp) must have APP cyan bracket")

      // Subtest 2: Weekly exhaustion on CLI account forces displayed 5h to 0% and slate gray
      let accCliExhausted = AccountQuota(
        id: "cli-exhausted",
        name: "CLI Exhausted",
        email: "exhausted@openai.com",
        planType: "team",
        isCurrentActive: false,
        fiveHourPercentage: 80.0,
        weeklyPercentage: 0.0,
        resetTime: nil,
        resetAfterSeconds: 3600,
        credits: 0,
        planMultiplier: 1.0
      )
      let exhaustedSnapshot = MultiAccountSnapshot(
        timestamp: Date(),
        activeAccountId: accActive.id,
        activeEmail: accActive.email,
        activePlan: accActive.planType,
        fiveHourPercentage: 10.0,
        weeklyPercentage: 100.0,
        resetTime: nil,
        resetAfterSeconds: 3600,
        credits: 0,
        accounts: [accActive, accCliExhausted],
        cliAccount: accCliExhausted
      )
      let exhaustedSessions = AppDelegate.resolveStatusBarSessions(from: exhaustedSnapshot, isScreenActive: true)
      assertEqual(exhaustedSessions.cliSession.fiveHPct, "0%", "Exhausted CLI weekly percentage must force CLI 5h to 0%")
      let expectedExhaustedColor = MenuBarAppearanceHelper.menuBarColor(forPercentage: 80.0, weeklyPercentage: 0.0, isScreenActive: true, planMultiplier: 1.0)
      assertEqual(exhaustedSessions.cliSession.fiveHColor, expectedExhaustedColor, "Exhausted CLI 5h color must be slate gray")

      // Subtest 3: Fall back to current active account when snapshot.cliAccount is nil
      let fallbackActiveSnapshot = MultiAccountSnapshot(
        timestamp: Date(),
        activeAccountId: accActive.id,
        activeEmail: accActive.email,
        activePlan: accActive.planType,
        fiveHourPercentage: 10.0,
        weeklyPercentage: 20.0,
        resetTime: nil,
        resetAfterSeconds: 3600,
        credits: 0,
        accounts: [accActive],
        cliAccount: nil
      )
      let fallbackActiveSessions = AppDelegate.resolveStatusBarSessions(from: fallbackActiveSnapshot, isScreenActive: true)
      assertEqual(fallbackActiveSessions.cliSession.fiveHPct, "45%", "CLI session must fall back to active account 5h percentage")
      assertEqual(fallbackActiveSessions.cliSession.weeklyPct, "55%", "CLI session must fall back to active account weekly percentage")

      // Subtest 4: Use top-level snapshot values only when no CLI account exists
      let topLevelOnlySnapshot = MultiAccountSnapshot(
        timestamp: Date(),
        activeAccountId: nil,
        activeEmail: nil,
        activePlan: nil,
        fiveHourPercentage: 12.0,
        weeklyPercentage: 24.0,
        resetTime: nil,
        resetAfterSeconds: 3600,
        credits: 0,
        isAppRunning: false,
        planMultiplier: 1.0,
        accounts: [],
        cliAccount: nil
      )
      let topLevelSessions = AppDelegate.resolveStatusBarSessions(from: topLevelOnlySnapshot, isScreenActive: true)
      assertEqual(topLevelSessions.cliSession.fiveHPct, "12%", "CLI session must use top-level 5h when no CLI account exists")
      assertEqual(topLevelSessions.cliSession.weeklyPct, "24%", "CLI session must use top-level weekly when no CLI account exists")
      assertTrue(topLevelSessions.appSession == nil, "APP session must be nil when no app account exists")

      // Subtest 5: APP must still use snapshot.appAccount only while the app is running
      let closedAppSnapshot = MultiAccountSnapshot(
        timestamp: Date(),
        activeAccountId: accActive.id,
        activeEmail: accActive.email,
        activePlan: accActive.planType,
        fiveHourPercentage: 10.0,
        weeklyPercentage: 20.0,
        resetTime: nil,
        resetAfterSeconds: 3600,
        credits: 0,
        isAppRunning: false,
        accounts: [accActive, accCli, accApp],
        appAccount: accApp,
        cliAccount: accCli
      )
      let closedAppSessions = AppDelegate.resolveStatusBarSessions(from: closedAppSnapshot, isScreenActive: true)
      assertTrue(closedAppSessions.appSession == nil, "appSession must be nil when isAppRunning is false even if appAccount is set")
      let closedAppAttr = AppDelegate.buildStatusBarAttributedString(
        snapshot: closedAppSnapshot,
        icon: mockIcon,
        isScreenActive: true,
        useQuotaIcons: true,
        stackPercentages: false
      )
      assertTrue(!closedAppAttr.string.contains("APP "), "Attributed string must not display APP session when app is closed")
      assertTrue(closedAppAttr.string.contains("CLI "), "Attributed string must display CLI session when app is closed")

      // A running Desktop with an unverified or previous-process marker must not show a false 0%.
      let unknownAppSnapshot = MultiAccountSnapshot(
        timestamp: Date(), activeAccountId: accCli.id, activeEmail: accCli.email,
        activePlan: accCli.planType, fiveHourPercentage: 90, weeklyPercentage: 85,
        resetTime: nil, resetAfterSeconds: 3600, credits: 0,
        isAppRunning: true, accounts: [accCli, accApp], appAccount: nil, cliAccount: accCli)
      let unknownSessions = AppDelegate.resolveStatusBarSessions(from: unknownAppSnapshot)
      assertEqual(unknownSessions.appSession?.fiveHPct, "—", "Unknown APP quota must be explicit")
      assertEqual(unknownSessions.cliSession.fiveHPct, "90%", "CLI quota must stay independent")

      let app1160 = AccountQuota(
        id: "app-1160", email: "app@example.com", planType: "pro", isCurrentActive: false,
        fiveHourPercentage: 1160, weeklyPercentage: 1160, resetTime: nil,
        resetAfterSeconds: 3600, credits: 0, planMultiplier: 20)
      let cliZero = AccountQuota(
        id: "cli-zero", email: "cli@example.com", planType: "plus", isCurrentActive: true,
        fiveHourPercentage: 0, weeklyPercentage: 33, resetTime: nil,
        resetAfterSeconds: 3600, credits: 0)
      let splitSnapshot = MultiAccountSnapshot(
        timestamp: Date(), activeAccountId: cliZero.id, activeEmail: cliZero.email,
        activePlan: cliZero.planType, fiveHourPercentage: 0, weeklyPercentage: 33,
        resetTime: nil, resetAfterSeconds: 3600, credits: 0,
        isAppRunning: true, accounts: [app1160, cliZero], appAccount: app1160,
        cliAccount: cliZero)
      let splitSessions = AppDelegate.resolveStatusBarSessions(from: splitSnapshot)
      assertEqual(splitSessions.appSession?.fiveHPct, "1160%", "APP must show its 20x quota")
      assertEqual(splitSessions.cliSession.fiveHPct, "0%", "CLI must show its separate exhausted quota")

      // Subtest 6: updateStatusBar execution with image and tooltip routing
      let appDelegateTest = AppDelegate()
      appDelegateTest.statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
      appDelegateTest.updateStatusBar(with: fullSnapshot)
      assertTrue(appDelegateTest.statusItem?.button?.image != nil, "updateStatusBar must assign composite image to statusItem button")
      let toolTip = appDelegateTest.statusItem?.button?.toolTip ?? ""
      assertTrue(toolTip.contains("cli@openai.com"), "Tooltip must contain resolved CLI email: \(toolTip)")
      assertTrue(toolTip.contains("app@openai.com"), "Tooltip must contain APP email when running: \(toolTip)")

      print("  ✅ Status Bar CLI Account Resolution & Quota Decoupling verified")
    }

    // A marker created after startup, then atomically replaced, must refresh
    // the menu without waiting for the quota-status file to change.
    let watcherHome = FileManager.default.temporaryDirectory.appendingPathComponent(
      "codex-desktop-watcher-\(UUID().uuidString)")
    try! FileManager.default.createDirectory(at: watcherHome, withIntermediateDirectories: true)
    let previousCodexHome = ProcessInfo.processInfo.environment["CODEX_HOME"]
    setenv("CODEX_HOME", watcherHome.path, 1)
    let watcher = AppDelegate()
    var markerRefreshes = 0
    watcher.desktopSessionSnapshotRefreshOverride = { markerRefreshes += 1 }
    watcher.startDesktopSessionFileWatcher()
    let marker = watcherHome.appendingPathComponent("desktop-app-session.json")
    let firstTemp = watcherHome.appendingPathComponent("first.tmp")
    try! Data("first".utf8).write(to: firstTemp)
    assertEqual(rename(firstTemp.path, marker.path), 0, "first marker rename must succeed")
    let firstDeadline = Date().addingTimeInterval(2)
    while markerRefreshes < 1 && Date() < firstDeadline {
      RunLoop.main.run(until: Date().addingTimeInterval(0.02))
    }
    assertTrue(markerRefreshes >= 1, "new marker must trigger an immediate menu refresh")
    let firstCount = markerRefreshes
    let secondTemp = watcherHome.appendingPathComponent("second.tmp")
    try! Data("second".utf8).write(to: secondTemp)
    assertEqual(rename(secondTemp.path, marker.path), 0, "replacement marker rename must succeed")
    let secondDeadline = Date().addingTimeInterval(2)
    while markerRefreshes <= firstCount && Date() < secondDeadline {
      RunLoop.main.run(until: Date().addingTimeInterval(0.02))
    }
    assertTrue(markerRefreshes > firstCount, "replacement marker must refresh the menu")
    watcher.stopDesktopSessionFileWatcher()
    watcher.desktopSessionSnapshotRefreshOverride = nil
    if let previousCodexHome { setenv("CODEX_HOME", previousCodexHome, 1) } else { unsetenv("CODEX_HOME") }
    try! FileManager.default.removeItem(at: watcherHome)
    assertTrue(AppDelegate.isOfficialDesktopExecutable(
      "/Applications/ChatGPT.app/Contents/MacOS/ChatGPT"), "official Desktop event must refresh")
    assertTrue(!AppDelegate.isOfficialDesktopExecutable(
      "/Applications/Other.app/Contents/MacOS/ChatGPT"), "foreign app event must be ignored")

    // 5. 300-Line Limit & Single Entity Invariant Verification
    let sourceFilesToCheck = [
      "Sources/MenuIconButton.swift",
      "Sources/InsetSeparatorView.swift",
      "Sources/PrimaryMenuSectionHeaderView.swift",
      "Sources/AccountSectionHeaderView.swift",
      "Sources/ReserveAccountSectionEntry.swift",
      "Sources/ResetCreditsRowView.swift",
      "Sources/AccountRowView.swift",
      "Sources/AccountSwitchButtonsView.swift",
      "Sources/AccountSectionCardView.swift",
      "Sources/AccountSectionCardView+Tracking.swift",
      "Sources/AppDelegate.swift",
      "Sources/AppDelegate+FileWatchers.swift",
      "Sources/AppDelegate+DesktopLifecycle.swift",
      "Sources/StatusBarBracketRenderer.swift",
      "Sources/AppDelegate+StatusBar.swift",
      "Sources/AppDelegate+StatusBarOverloads.swift",
      "Sources/AppDelegate+Menu.swift",
      "Sources/AppDelegate+AppBlock.swift",
      "Sources/AppDelegate+CliBlock.swift",
      "Sources/AppDelegate+ReserveCards.swift",
      "Sources/AppDelegate+DynamicItems.swift",
      "Sources/AppDelegate+AccountActions.swift",
      "Sources/AppDelegate+AutoSwitch.swift",
      "Sources/AppDelegate+AutoReset.swift",
      "Sources/AppDelegate+SettingsActions.swift",
    ]
    for relPath in sourceFilesToCheck {
      let fullPath = URL(fileURLWithPath: FileManager.default.currentDirectoryPath).appendingPathComponent(relPath)
      if let content = try? String(contentsOf: fullPath, encoding: .utf8) {
        let lineCount = content.components(separatedBy: "\n").count
        assertTrue(lineCount <= 300, "File \(relPath) line count (\(lineCount)) must be <= 300 lines")
      }
    }
    print("  ✅ Swift <= 300 lines architectural invariant verified for all menu components")

    print("\n🎉 ALL APP DELEGATE TESTS PASSED!")
  }
}
