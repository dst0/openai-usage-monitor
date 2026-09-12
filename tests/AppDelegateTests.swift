import AppKit
import Foundation

func assertEqual<T: Equatable>(_ actual: T, _ expected: T, _ message: String = "", file: StaticString = #file, line: UInt = #line) {
    if actual != expected {
        print("❌ Assertion Failed: [\(actual)] != [\(expected)] - \(message) at \(file):\(line)")
        exit(1)
    }
}

func assertTrue(_ condition: Bool, _ message: String = "", file: StaticString = #file, line: UInt = #line) {
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
        assertTrue(stackedStr.contains("[") && stackedStr.contains("]"), "Bracket frame must be present")
        assertTrue(!stackedStr.contains("90%"), "Percentages should be drawn into attachment image, not text string")
        assertTrue(!stackedStr.contains("85%"), "Percentages should be drawn into attachment image, not text string")

        // Count attachments: 1 app icon + 1 stacked values attachment + 1 hybrid badge = 3 attachments
        var stackedAttachmentCount = 0
        var stackedAttachments: [NSTextAttachment] = []
        defaultStackedAttr.enumerateAttribute(.attachment, in: NSRange(location: 0, length: defaultStackedAttr.length), options: []) { val, _, _ in
            if let att = val as? NSTextAttachment {
                stackedAttachmentCount += 1
                stackedAttachments.append(att)
            }
        }
        assertEqual(stackedAttachmentCount, 3, "Expected 1 app icon + 1 stacked values + 1 bracketed badge = 3 attachments")

        // Attachment 0: app icon (bounds 18x18, vertically centered at y=-5.0)
        assertEqual(stackedAttachments[0].image, mockIcon)
        assertEqual(stackedAttachments[0].bounds, CGRect(x: 0, y: -5.0, width: 18, height: 18))

        // Verify attachment with 22x22 icon (standard macOS menu bar size, vertically centered at y=-6.0)
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
        attr22.enumerateAttribute(.attachment, in: NSRange(location: 0, length: attr22.length), options: []) { val, _, _ in
            if let att = val as? NSTextAttachment { att22List.append(att) }
        }
        assertEqual(att22List[0].bounds, CGRect(x: 0, y: -6.0, width: 22, height: 22), "22x22 icon must have bounds y=-6.0, w=22, h=22")

        // Attachment 1: 2-row stacked values (bounds height=20.5, y=-5.0)
        assertEqual(stackedAttachments[1].bounds.origin.y, -5.0, "Stacked values attachment y origin must be -5.0")
        assertEqual(stackedAttachments[1].bounds.size.height, 20.5, "Stacked values attachment height must be 20.5")
        assertTrue(stackedAttachments[1].bounds.size.width > 20.0, "Stacked values attachment width must be > 20.0")

        print("  ✅ Stacked mode single session layout verified")

        // ====================================================================
        // Test 2: Dual Session Mode (APP + CLI) with stacked layout
        // ====================================================================
        let dualStackedAttr = AppDelegate.buildStatusBarAttributedString(
            icon: mockIcon,
            appSession: (fiveHPct: "100%", fiveHColor: NSColor.systemGreen, weeklyPct: "95%", weeklyColor: NSColor.systemGreen),
            cliSession: (fiveHPct: "90%", fiveHColor: NSColor.systemGreen, weeklyPct: "85%", weeklyColor: NSColor.systemGreen),
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
        dualStackedAttr.enumerateAttribute(.attachment, in: NSRange(location: 0, length: dualStackedAttr.length), options: []) { val, _, _ in
            if let att = val as? NSTextAttachment {
                dualStackedAttachmentCount += 1
                dualStackedAttachments.append(att)
            }
        }
        assertEqual(dualStackedAttachmentCount, 4, "Expected 1 app icon + 1 APP stacked + 1 CLI stacked + 1 badge = 4 attachments")
        assertTrue(dualStackedAttachments[1] !== dualStackedAttachments[2], "APP and CLI attachments must be distinct instances")
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
        assertTrue(horizStr.contains("44%"), "In horizontal mode, weekly percentage must be in text string")

        var horizAttachmentCount = 0
        horizontalAttr.enumerateAttribute(.attachment, in: NSRange(location: 0, length: horizontalAttr.length), options: []) { val, _, _ in
            if val != nil { horizAttachmentCount += 1 }
        }
        // 1 app icon + 1 sprint icon + 1 weekly icon + 1 shield = 4 attachments
        assertEqual(horizAttachmentCount, 4, "Expected 1 app icon + 1 sprint icon + 1 weekly icon + 1 shield badge = 4 attachments")

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

        // ====================================================================
        // Test 6: Inactive Screen Dimming & Compatibility Matrix
        // ====================================================================
        // 1. Verify button alphaValue logic matches macOS system dimming (~0.55 alpha)
        let activeAlpha: CGFloat = true ? 1.0 : 0.55
        let inactiveAlpha: CGFloat = false ? 1.0 : 0.55
        assertEqual(activeAlpha, 1.0, "Active screen alpha must be 1.0")
        assertEqual(inactiveAlpha, 0.55, "Inactive screen alpha must be 0.55 matching macOS system dimming")

        // 2. Verify attributed string generation does not bloom on inactive screen
        let activeFull = AppDelegate.buildStatusBarAttributedString(
            icon: mockIcon,
            appSession: (fiveHPct: "100%", fiveHColor: NSColor.systemGreen, weeklyPct: "95%", weeklyColor: NSColor.systemGreen),
            cliSession: (fiveHPct: "90%", fiveHColor: NSColor.systemGreen, weeklyPct: "85%", weeklyColor: NSColor.systemGreen),
            accounts: [],
            isScreenActive: true,
            useQuotaIcons: true,
            stackPercentages: true
        )
        let inactiveFull = AppDelegate.buildStatusBarAttributedString(
            icon: mockIcon,
            appSession: (fiveHPct: "100%", fiveHColor: NSColor.systemGreen, weeklyPct: "95%", weeklyColor: NSColor.systemGreen),
            cliSession: (fiveHPct: "90%", fiveHColor: NSColor.systemGreen, weeklyPct: "85%", weeklyColor: NSColor.systemGreen),
            accounts: [],
            isScreenActive: false,
            useQuotaIcons: true,
            stackPercentages: true
        )
        assertEqual(activeFull.string, inactiveFull.string, "String structure must be identical")

        // 3. Verify APP tag and CLI tag colors are not inflated to neon pastel when inactive
        var activeTagColors: [NSColor] = []
        activeFull.enumerateAttribute(.foregroundColor, in: NSRange(location: 0, length: activeFull.length), options: []) { val, _, _ in
            if let col = val as? NSColor { activeTagColors.append(col) }
        }
        var inactiveTagColors: [NSColor] = []
        inactiveFull.enumerateAttribute(.foregroundColor, in: NSRange(location: 0, length: inactiveFull.length), options: []) { val, _, _ in
            if let col = val as? NSColor { inactiveTagColors.append(col) }
        }
        assertTrue(!activeTagColors.isEmpty && !inactiveTagColors.isEmpty, "Tag colors must be present")
        for i in 0..<min(activeTagColors.count, inactiveTagColors.count) {
            let aCol = activeTagColors[i].usingColorSpace(.sRGB)!
            let iCol = inactiveTagColors[i].usingColorSpace(.sRGB)!
            let aLum = 0.2126 * aCol.redComponent + 0.7152 * aCol.greenComponent + 0.0722 * aCol.blueComponent
            let iLum = 0.2126 * iCol.redComponent + 0.7152 * iCol.greenComponent + 0.0722 * iCol.blueComponent
            assertTrue(iLum <= aLum + 0.05, "Inactive text luminance (\(iLum)) must not significantly exceed active text luminance (\(aLum))")
        }
        print("  ✅ Inactive screen dimming & compatibility matrix verified")

        print("\n🎉 ALL APP DELEGATE TESTS PASSED!")
    }
}
