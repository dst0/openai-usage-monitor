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
struct ScreenContrastTestRunner {
    static func main() {
        print("🧪 Running Screen Contrast, Vector Icons & Stacked Percentage Tests...")

        // ====================================================================
        // Test 1: QuotaColorType thresholds & colors
        // ====================================================================
        assertEqual(QuotaColorType.from(pct: 95.0), .green)
        assertEqual(QuotaColorType.from(pct: 50.1), .green)
        assertEqual(QuotaColorType.from(pct: 50.0), .yellow)
        assertEqual(QuotaColorType.from(pct: 15.1), .yellow)
        assertEqual(QuotaColorType.from(pct: 15.0), .red)
        assertEqual(QuotaColorType.from(pct: 0.0), .red)

        let redActiveColors = QuotaColorType.red.colors(isScreenActive: true)
        let redInactiveColors = QuotaColorType.red.colors(isScreenActive: false)
        assertEqual(redActiveColors.count, 3)
        assertEqual(redInactiveColors.count, 3)

        let greenInactiveColors = QuotaColorType.green.colors(isScreenActive: false)
        let inactiveGreenTop = greenInactiveColors[0].usingColorSpace(.sRGB)!
        assertTrue(inactiveGreenTop.greenComponent >= 0.75, "Inactive green top highlight green tone")
        assertTrue(inactiveGreenTop.greenComponent <= 0.90, "Inactive green top must not be neon oversaturated")

        let grayActiveColors = QuotaColorType.gray.colors(isScreenActive: true)
        let grayInactiveColors = QuotaColorType.gray.colors(isScreenActive: false)
        assertEqual(grayActiveColors.count, 3)
        assertEqual(grayInactiveColors.count, 3)
        let activeGrayMid = grayActiveColors[1].usingColorSpace(.sRGB)!
        assertTrue(abs(activeGrayMid.redComponent - activeGrayMid.greenComponent) < 0.05, "Active gray should be neutral")
        assertTrue(abs(activeGrayMid.redComponent - activeGrayMid.blueComponent) < 0.05, "Active gray should be neutral")

        print("  ✅ QuotaColorType thresholds and colors verified")

        // ====================================================================
        // Test 2: isWeeklyExhausted logic
        // ====================================================================
        assertTrue(MenuBarAppearanceHelper.isWeeklyExhausted(0.0), "0.0% is exhausted")
        assertTrue(MenuBarAppearanceHelper.isWeeklyExhausted(-5.0), "negative is exhausted")
        assertTrue(MenuBarAppearanceHelper.isWeeklyExhausted(0.245), "0.245% micro-remnant is exhausted")
        assertTrue(MenuBarAppearanceHelper.isWeeklyExhausted(0.49), "0.49% is exhausted")
        assertTrue(!MenuBarAppearanceHelper.isWeeklyExhausted(0.50), "0.50% is not exhausted")
        assertTrue(!MenuBarAppearanceHelper.isWeeklyExhausted(1.0), "1.0% is not exhausted")
        assertTrue(!MenuBarAppearanceHelper.isWeeklyExhausted(15.0), "15.0% is not exhausted")
        assertTrue(!MenuBarAppearanceHelper.isWeeklyExhausted(nil), "nil weekly is not exhausted")
        assertTrue(!MenuBarAppearanceHelper.isWeeklyExhausted(Double.nan), "NaN weekly is not exhausted")

        // Exhausted weekly quota overrides 100% 5h to slate gray
        let grayActive = MenuBarAppearanceHelper.menuBarColor(forPercentage: 100.0, weeklyPercentage: 0.0, isScreenActive: true).usingColorSpace(.sRGB)!
        let grayInactive = MenuBarAppearanceHelper.menuBarColor(forPercentage: 100.0, weeklyPercentage: 0.0, isScreenActive: false).usingColorSpace(.sRGB)!
        let grayRemnant = MenuBarAppearanceHelper.menuBarColor(forPercentage: 100.0, weeklyPercentage: 0.245, isScreenActive: true).usingColorSpace(.sRGB)!
        let greenActive = MenuBarAppearanceHelper.menuBarColor(forPercentage: 100.0, weeklyPercentage: 50.0, isScreenActive: true).usingColorSpace(.sRGB)!

        assertTrue(abs(grayActive.redComponent - grayActive.greenComponent) < 0.05, "Exhausted weekly color must be slate gray")
        assertTrue(greenActive.greenComponent > greenActive.redComponent + 0.3, "Healthy weekly color must be bright green")
        assertEqual(grayActive.redComponent, grayRemnant.redComponent, "Remnant weekly should yield identical gray to 0.0%")
        assertTrue(grayInactive.redComponent <= grayActive.redComponent, "Inactive gray must not be brighter than active gray")

        print("  ✅ isWeeklyExhausted and gray color override verified")

        // ====================================================================
        // Test 3: Mini Quota Icons (👾 Retro "5h" Sprint & 📅 Weekly)
        // ====================================================================
        let sprintIcon = MenuBarAppearanceHelper.makeSprintIcon(size: 8.0, isScreenActive: true)
        let weeklyIcon = MenuBarAppearanceHelper.makeWeeklyIcon(size: 8.0, isScreenActive: true)
        assertTrue(sprintIcon.size.width >= 10.0 && sprintIcon.size.width <= 12.0, "Sprint icon width should fit 9 cols + shadow")
        assertTrue(sprintIcon.size.height >= 8.0 && sprintIcon.size.height <= 10.0, "Sprint icon height should fit 7 rows + shadow")
        assertTrue(weeklyIcon.size.width >= 8.0 && weeklyIcon.size.width <= 11.0, "Weekly icon width should fit 9 cols + shadow")
        assertTrue(weeklyIcon.size.height >= 8.0 && weeklyIcon.size.height <= 11.0, "Weekly icon height should fit 9 rows + shadow")
        assertTrue(!sprintIcon.isTemplate, "Sprint icon must not be template")
        assertTrue(!weeklyIcon.isTemplate, "Weekly icon must not be template")

        let tiffSprint = sprintIcon.tiffRepresentation
        assertTrue(tiffSprint != nil && tiffSprint!.count > 100, "Sprint icon must rasterize cleanly")
        let weeklyDefault = MenuBarAppearanceHelper.makeWeeklyIcon(isScreenActive: true)
        assertEqual(weeklyDefault.size.width, 11.0, "Default weekly icon width must be 11.0")
        assertEqual(weeklyDefault.size.height, 11.0, "Default weekly icon height must be 11.0")
        assertTrue(weeklyDefault.tiffRepresentation != nil, "Default weekly icon must rasterize cleanly")

        print("  ✅ Mini Quota Icons generation and rasterization verified")

        // ====================================================================
        // Test 4: Stacked Percentage Indicator (2-Row Compact Layout)
        // ====================================================================
        let greenColor = MenuBarAppearanceHelper.menuBarColor(forPercentage: 100.0, isScreenActive: true)
        let redColor = MenuBarAppearanceHelper.menuBarColor(forPercentage: 10.0, isScreenActive: true)

        let (stackedActive, activeSize) = MenuBarAppearanceHelper.makeStackedValuesImage(
            fiveHPct: "48%",
            fiveHColor: greenColor,
            weeklyPct: "44%",
            weeklyColor: greenColor,
            isScreenActive: true,
            useQuotaIcons: true
        )
        assertEqual(activeSize.height, 20.5, "Stacked values height must be exactly 20.5pt")
        assertTrue(activeSize.width > 20.0 && activeSize.width < 48.0, "Stacked values width must be compact (20-48pt), was \(activeSize.width)")
        assertTrue(!stackedActive.isTemplate, "Stacked active image must not be template")

        let tiffStacked = stackedActive.tiffRepresentation
        assertTrue(tiffStacked != nil && tiffStacked!.count > 100, "Stacked image must rasterize to valid TIFF")

        // Inactive screen stacked image
        let (stackedInactive, inactiveSize) = MenuBarAppearanceHelper.makeStackedValuesImage(
            fiveHPct: "10%",
            fiveHColor: redColor,
            weeklyPct: "44%",
            weeklyColor: greenColor,
            isScreenActive: false,
            useQuotaIcons: true
        )
        assertEqual(inactiveSize.height, 20.5, "Inactive stacked values height must be 20.5pt")
        assertTrue(stackedInactive.tiffRepresentation != nil, "Inactive stacked image must rasterize cleanly")

        // Attachment creation and bounds
        let attachment = MenuBarAppearanceHelper.makeStackedValuesAttachment(
            fiveHPct: "48%",
            fiveHColor: greenColor,
            weeklyPct: "44%",
            weeklyColor: greenColor,
            isScreenActive: true,
            useQuotaIcons: true
        )
        assertTrue(attachment.image != nil, "Attachment must contain an NSImage")
        assertEqual(attachment.bounds.origin.x, 0.0, "Attachment x origin must be 0")
        assertEqual(attachment.bounds.origin.y, -5.0, "Attachment y origin must be -5.0 for baseline centering")
        assertEqual(attachment.bounds.size.height, 20.5, "Attachment height must be 20.5")
        assertEqual(attachment.bounds.size.width, activeSize.width, "Attachment width must match image width")

        // Text fallback mode (useQuotaIcons: false)
        let (textFallbackImg, textSize) = MenuBarAppearanceHelper.makeStackedValuesImage(
            fiveHPct: "48%",
            fiveHColor: greenColor,
            weeklyPct: "44%",
            weeklyColor: greenColor,
            isScreenActive: true,
            useQuotaIcons: false
        )
        assertEqual(textSize.height, 20.5, "Text fallback height must be 20.5pt")
        assertTrue(textSize.width > activeSize.width, "Text fallback with '5h: '/'Wk: ' must be wider than icon mode")
        assertTrue(textFallbackImg.tiffRepresentation != nil, "Text fallback must rasterize cleanly")

        print("  ✅ Stacked Percentage Indicator (2-Row Compact Layout) verified")

        // ====================================================================
        // Test 5: 3-Row Hybrid Quota Indicator with exhausted weekly
        // ====================================================================
        let hybridExhaustedWeekly = MenuBarAppearanceHelper.makeHybridQuotaIndicator(
            fiveHour: 100.0,
            weekly: 0.0,
            credits: 0,
            width: 13.0,
            height: 16.5,
            isScreenActive: true
        )
        assertEqual(hybridExhaustedWeekly.size.width, 13.0)
        assertEqual(hybridExhaustedWeekly.size.height, 16.5)

        let tiffBadge = hybridExhaustedWeekly.tiffRepresentation!
        let bitmap = NSBitmapImageRep(data: tiffBadge)!
        let sampleY = Int(Double(bitmap.pixelsHigh) * 0.22)
        let sampleX = Int(Double(bitmap.pixelsWide) * 0.50)
        let topColor = bitmap.colorAt(x: sampleX, y: sampleY)!.usingColorSpace(.sRGB)!

        // Over exhausted weekly, top sprint section must be slate gray, NOT bright green
        assertTrue(abs(topColor.redComponent - topColor.greenComponent) < 0.12, "Top sprint quota over exhausted weekly must be neutral slate gray, got R=\(topColor.redComponent), G=\(topColor.greenComponent)")
        assertTrue(topColor.greenComponent < topColor.redComponent + 0.12, "Top sprint quota over exhausted weekly must NOT be green")

        print("  ✅ 3-Row Hybrid Shield Badge with exhausted weekly verified")

        // ====================================================================
        // Test 6: Inactive Screen Typography Consistency & Drop Shadow Softness
        // ====================================================================
        let activeNumFont = MenuBarAppearanceHelper.numberFont(isScreenActive: true)
        let inactiveNumFont = MenuBarAppearanceHelper.numberFont(isScreenActive: false)
        let activeTraits = activeNumFont.fontDescriptor.object(forKey: .traits) as? [NSFontDescriptor.TraitKey: Any]
        let inactiveTraits = inactiveNumFont.fontDescriptor.object(forKey: .traits) as? [NSFontDescriptor.TraitKey: Any]
        let activeWeight = activeTraits?[.weight] as? CGFloat ?? 0.0
        let inactiveWeight = inactiveTraits?[.weight] as? CGFloat ?? 0.0
        assertTrue(inactiveWeight <= activeWeight, "Inactive font weight must not exceed active font weight")

        let activeShadow = MenuBarAppearanceHelper.textShadow(isScreenActive: true)
        let inactiveShadow = MenuBarAppearanceHelper.textShadow(isScreenActive: false)
        let activeShadowCol = (activeShadow.shadowColor ?? .black).usingColorSpace(.sRGB)!
        let inactiveShadowCol = (inactiveShadow.shadowColor ?? .black).usingColorSpace(.sRGB)!
        assertTrue(inactiveShadowCol.alphaComponent <= activeShadowCol.alphaComponent, "Inactive shadow must be softer than active to eliminate lantern glow")
        assertEqual(inactiveShadow.shadowBlurRadius, 1.0)

        let activeSepColor = MenuBarAppearanceHelper.separatorColor(isScreenActive: true).usingColorSpace(.sRGB)!
        let inactiveSepColor = MenuBarAppearanceHelper.separatorColor(isScreenActive: false).usingColorSpace(.sRGB)!
        assertTrue(inactiveSepColor.redComponent <= activeSepColor.redComponent, "Inactive separator must not be brighter than active")

        print("  ✅ Inactive Screen Typography & Shadow Softness verified")

        print("\n🎉 ALL SCREEN CONTRAST & STACKED TESTS PASSED!")
    }
}
