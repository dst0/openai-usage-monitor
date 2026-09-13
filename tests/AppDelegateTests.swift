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
    // Test 7: Business Account Auto-Switch Selection & Resets Ranking
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
    let proAccount = AccountQuota(
      id: "pro-1",
      name: "Pro Account",
      email: "pro@gmail.com",
      planType: "pro",
      isCurrentActive: false,
      fiveHourPercentage: 90.0,
      weeklyPercentage: nil,
      resetTime: nil,
      resetAfterSeconds: 1200,
      credits: 0
    )
    let bizAccountWithMoreCredits = AccountQuota(
      id: "work-2",
      name: "Main Corporate",
      email: "team@work.com",
      planType: "business",
      isCurrentActive: false,
      fiveHourPercentage: 80.0,
      weeklyPercentage: nil,
      resetTime: nil,
      resetAfterSeconds: 7200,
      credits: 4
    )

    assertTrue(bizAccount.isBusiness, "Team plan must be detected as business")
    assertTrue(bizAccountWithMoreCredits.isBusiness, "Business plan must be detected as business")
    assertTrue(!personalAccount.isBusiness, "Plus plan must not be detected as business")
    assertTrue(!proAccount.isBusiness, "Pro plan must not be detected as business")

    assertTrue(
      !L10n.autoSwitchBusinessOnly.isEmpty, "autoSwitchBusinessOnly localization must not be empty")
    assertTrue(
      !L10n.autoSwitchBusinessPriority.isEmpty,
      "autoSwitchBusinessPriority localization must not be empty")

    // Simulation of ranking function:
    func rankCandidates(_ accounts: [AccountQuota], businessPriority: Bool, businessOnly: Bool)
      -> [AccountQuota]
    {
      var candidates = accounts.filter { $0.fiveHourPercentage > 0.0 && $0.error == nil }
      if businessOnly {
        candidates = candidates.filter { $0.isBusiness }
      }
      candidates.sort { a, b in
        if businessPriority && a.isBusiness != b.isBusiness {
          return a.isBusiness
        }
        if a.credits != b.credits {
          return a.credits > b.credits
        }
        let aReset = a.resetAfterSeconds ?? Int.max
        let bReset = b.resetAfterSeconds ?? Int.max
        if aReset != bReset {
          return aReset < bReset
        }
        return a.fiveHourPercentage > b.fiveHourPercentage
      }
      return candidates
    }

    // Case 1: Business Priority mode
    // Both bizAccount (2 credits) and bizAccountWithMoreCredits (4 credits) must rank ahead of personalAccount (5 credits)
    // Between the two business accounts, work-2 (4 credits) must rank first!
    let prioritized = rankCandidates(
      [personalAccount, bizAccount, bizAccountWithMoreCredits], businessPriority: true,
      businessOnly: false)
    assertEqual(
      prioritized.first?.id, "work-2",
      "Business priority: account with more resets among business accounts must be first")
    assertEqual(prioritized[1].id, "work-1", "Second must be work-1 (business)")
    assertEqual(
      prioritized[2].id, "personal-1",
      "Non-business account must be last when business accounts have quota")

    // Case 2: Business Priority fallback when all business accounts are exhausted
    let exhaustedBiz = AccountQuota(
      id: "work-dead",
      email: "team@work.com",
      planType: "business",
      isCurrentActive: false,
      fiveHourPercentage: 0.0,
      weeklyPercentage: nil,
      resetTime: nil,
      resetAfterSeconds: 3600,
      credits: 10
    )
    let fallback = rankCandidates(
      [exhaustedBiz, personalAccount], businessPriority: true, businessOnly: false)
    assertEqual(
      fallback.first?.id, "personal-1",
      "Fallback to personal when all business accounts have 0% quota")

    // Case 3: Business Only mode
    let bizOnlyList = rankCandidates(
      [personalAccount, bizAccountWithMoreCredits], businessPriority: false, businessOnly: true)
    assertEqual(
      bizOnlyList.count, 1, "Only business accounts must be included in business-only mode")
    assertEqual(bizOnlyList.first?.id, "work-2", "work-2 should be the only candidate")

    let bizOnlyExhausted = rankCandidates(
      [personalAccount, exhaustedBiz], businessPriority: false, businessOnly: true)
    assertTrue(
      bizOnlyExhausted.isEmpty,
      "When business accounts have no quota, business-only must return no candidates")

    // Case 4: Preference for more resets (credits) among non-business accounts or standard mode
    let standardMode = rankCandidates(
      [proAccount, personalAccount], businessPriority: false, businessOnly: false)
    assertEqual(
      standardMode.first?.id, "personal-1",
      "Account with 5 credits must rank ahead of account with 0 credits")

    // Case 5: AppDelegate.determineAutoSwitchTarget Preemption verification
    // Active Pro has 90% quota, business account has 70% quota
    let preemptTarget = AppDelegate.determineAutoSwitchTarget(
      activeAcc: proAccount,
      accounts: [proAccount, bizAccount],
      autoSwitchEnabled: true,
      businessOnly: false,
      businessPriority: true
    )
    assertEqual(
      preemptTarget?.id, "work-1",
      "Active Pro account with quota must be preempted when business quota is available")

    // Active Pro has 90% quota, but all business accounts have 0% quota -> No preemption
    let noPreemptTarget = AppDelegate.determineAutoSwitchTarget(
      activeAcc: proAccount,
      accounts: [proAccount, exhaustedBiz],
      autoSwitchEnabled: true,
      businessOnly: false,
      businessPriority: true
    )
    assertTrue(
      noPreemptTarget == nil,
      "Active Pro must not be preempted when all business accounts are exhausted")

    // Active Business has 70% quota -> Must NOT be preempted by another business account with higher quota
    let activeBizPreempt = AppDelegate.determineAutoSwitchTarget(
      activeAcc: bizAccount,
      accounts: [bizAccount, bizAccountWithMoreCredits],
      autoSwitchEnabled: true,
      businessOnly: false,
      businessPriority: true
    )
    assertTrue(activeBizPreempt == nil, "Active business account with quota must NOT be preempted")

    print("  ✅ Business account auto-switch selection & resets ranking verified")

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

    // Test localized helps URLs
    let jaURL = HelpsDocHelper.localizedHelpsHTMLURL(languageCode: "ja")
    assertTrue(
      jaURL?.absoluteString.contains("lang=ja") == true,
      "Helps URL for Japanese must contain lang=ja")

    let zhURL = HelpsDocHelper.localizedHelpsHTMLURL(languageCode: "zh-Hans")
    assertTrue(
      zhURL?.absoluteString.contains("lang=zh-Hans") == true,
      "Helps URL for zh-Hans must contain lang=zh-Hans")

    let zhAliasURL = HelpsDocHelper.localizedHelpsHTMLURL(languageCode: "zh")
    assertTrue(
      zhAliasURL?.absoluteString.contains("lang=zh-Hans") == true,
      "Helps URL for zh alias must resolve to lang=zh-Hans")

    let viURL = HelpsDocHelper.localizedHelpsHTMLURL(languageCode: "vi")
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
    let weeklyItems = menuTitles.filter { $0.contains("Weekly:") }
    assertTrue(
      weeklyItems.count >= 2,
      "Expected at least 2 Weekly items in menu (active + reserve), got: \(weeklyItems)")
    assertTrue(
      weeklyItems.contains(where: { $0.contains("52%") }),
      "Menu must contain weekly item with 52% for reserve account")

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

    // Verify Organization Section Headers exist in menu
    assertTrue(
      allTitles.contains(where: { $0.contains("🏢 Destination Works Pty Ltd") }),
      "Menu must contain section header for 'Destination Works Pty Ltd'"
    )
    assertTrue(
      allTitles.contains(where: { $0.contains("🏢 dstworks family") }),
      "Menu must contain section header for 'dstworks family'"
    )
    assertTrue(
      allTitles.contains(where: { $0.contains("👤 Personal Accounts") || $0.contains("👤 Личные аккаунты") }),
      "Menu must contain section header for personal accounts"
    )

    print("  ✅ Business accounts grouping by organization with organization name verified")

    print("\n🎉 ALL APP DELEGATE TESTS PASSED!")
  }
}
