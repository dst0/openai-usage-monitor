import AppKit
import Foundation

// MARK: - Quota Color Palette (Calibrated for macOS active and inactive display states)

public enum QuotaColorType: Sendable {
    case green, yellow, red, gray

    public func colors(isScreenActive: Bool = true) -> [NSColor] {
        if isScreenActive {
            switch self {
            case .green:
                return [
                    NSColor(red: 0.35, green: 0.98, blue: 0.45, alpha: 1.0), // top highlight
                    NSColor(red: 0.0, green: 0.76, blue: 0.22, alpha: 1.0),   // mid rich green
                    NSColor(red: 0.0, green: 0.45, blue: 0.10, alpha: 1.0)    // bottom dark shadow
                ]
            case .yellow:
                return [
                    NSColor(red: 1.0, green: 0.92, blue: 0.35, alpha: 1.0), // top highlight
                    NSColor(red: 0.98, green: 0.65, blue: 0.0, alpha: 1.0),  // mid rich amber
                    NSColor(red: 0.75, green: 0.42, blue: 0.0, alpha: 1.0)   // bottom dark shadow
                ]
            case .red:
                return [
                    NSColor(red: 1.0, green: 0.58, blue: 0.52, alpha: 1.0), // top highlight
                    NSColor(red: 0.98, green: 0.28, blue: 0.24, alpha: 1.0), // mid rich ruby
                    NSColor(red: 0.78, green: 0.12, blue: 0.14, alpha: 1.0)  // bottom dark crimson
                ]
            case .gray:
                return [
                    NSColor(red: 0.83, green: 0.84, blue: 0.86, alpha: 1.0), // top highlight
                    NSColor(red: 0.65, green: 0.66, blue: 0.68, alpha: 1.0), // mid rich slate gray
                    NSColor(red: 0.47, green: 0.48, blue: 0.50, alpha: 1.0)  // bottom dark shadow
                ]
            }
        } else {
            // Natural harmonious tones for inactive screen, avoiding neon oversaturation
            switch self {
            case .green:
                return [
                    NSColor(red: 0.30, green: 0.88, blue: 0.40, alpha: 1.0),
                    NSColor(red: 0.0, green: 0.68, blue: 0.20, alpha: 1.0),
                    NSColor(red: 0.0, green: 0.40, blue: 0.10, alpha: 1.0)
                ]
            case .yellow:
                return [
                    NSColor(red: 0.92, green: 0.85, blue: 0.30, alpha: 1.0),
                    NSColor(red: 0.88, green: 0.58, blue: 0.0, alpha: 1.0),
                    NSColor(red: 0.68, green: 0.38, blue: 0.0, alpha: 1.0)
                ]
            case .red:
                return [
                    NSColor(red: 0.90, green: 0.45, blue: 0.42, alpha: 1.0),
                    NSColor(red: 0.82, green: 0.22, blue: 0.20, alpha: 1.0),
                    NSColor(red: 0.62, green: 0.10, blue: 0.12, alpha: 1.0)
                ]
            case .gray:
                return [
                    NSColor(red: 0.75, green: 0.76, blue: 0.78, alpha: 1.0),
                    NSColor(red: 0.58, green: 0.59, blue: 0.60, alpha: 1.0),
                    NSColor(red: 0.42, green: 0.43, blue: 0.45, alpha: 1.0)
                ]
            }
        }
    }

    public var colors: [NSColor] {
        return colors(isScreenActive: true)
    }

    public static func from(pct: Double) -> QuotaColorType {
        if pct > 50.0 { return .green }
        if pct > 20.0 { return .yellow }
        return .red
    }
}

// MARK: - Menu Bar Appearance Helper

public struct MenuBarAppearanceHelper {

    /// Determines whether a weekly quota percentage indicates that the weekly limit is exhausted (≤ 0.0 or rounds to 0%).
    /// Handles OpenAI / WHAM micro-remnants (e.g. 0.00245 fraction -> 0.245% which displays as 0%).
    public static func isWeeklyExhausted(_ weeklyPercentage: Double?) -> Bool {
        guard let w = weeklyPercentage, !w.isNaN else { return false }
        return w < 0.5 || round(w) <= 0.0
    }

    public static func menuBarColor(
        forPercentage pct: Double,
        weeklyPercentage: Double? = nil,
        isScreenActive: Bool = true,
        planMultiplier: Double = 1.0
    ) -> NSColor {
        if isWeeklyExhausted(weeklyPercentage) {
            // Slate gray clearly indicates unusable state when weekly quota is exhausted
            return isScreenActive
                ? NSColor(white: 0.78, alpha: 1.0)
                : NSColor(white: 0.65, alpha: 1.0)
        }
        let tankPct = planMultiplier > 0.0 ? (pct / planMultiplier) : pct
        if isScreenActive {
            if tankPct > 50.0 {
                return NSColor(red: 0.0, green: 0.88, blue: 0.35, alpha: 1.0)
            } else if tankPct > 20.0 {
                return NSColor(red: 1.0, green: 0.78, blue: 0.0, alpha: 1.0)
            } else {
                return NSColor(red: 1.0, green: 0.46, blue: 0.38, alpha: 1.0)
            }
        } else {
            if tankPct > 50.0 {
                return NSColor(red: 0.0, green: 0.78, blue: 0.30, alpha: 1.0)
            } else if tankPct > 20.0 {
                return NSColor(red: 0.90, green: 0.70, blue: 0.0, alpha: 1.0)
            } else {
                return NSColor(red: 0.90, green: 0.38, blue: 0.32, alpha: 1.0)
            }
        }
    }

    public static func dropdownColor(
        forPercentage pct: Double,
        weeklyPercentage: Double? = nil,
        planMultiplier: Double = 1.0
    ) -> NSColor {
        if isWeeklyExhausted(weeklyPercentage) {
            return NSColor(name: nil) { appearance in
                let isDark = appearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua
                return isDark
                    ? NSColor(white: 0.70, alpha: 1.0)
                    : NSColor(white: 0.45, alpha: 1.0)
            }
        }
        let tankPct = planMultiplier > 0.0 ? (pct / planMultiplier) : pct
        return NSColor(name: nil) { appearance in
            let isDark = appearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua
            if tankPct > 50.0 {
                return isDark
                    ? NSColor(red: 0.0, green: 0.88, blue: 0.38, alpha: 1.0)
                    : NSColor(red: 0.05, green: 0.62, blue: 0.22, alpha: 1.0)
            } else if tankPct > 20.0 {
                return isDark
                    ? NSColor(red: 1.0, green: 0.80, blue: 0.0, alpha: 1.0)
                    : NSColor(red: 0.82, green: 0.52, blue: 0.0, alpha: 1.0)
            } else {
                return isDark
                    ? NSColor(red: 1.0, green: 0.38, blue: 0.36, alpha: 1.0)
                    : NSColor(red: 0.85, green: 0.12, blue: 0.15, alpha: 1.0)
            }
        }
    }

    public static func numberFont(isScreenActive: Bool = true) -> NSFont {
        return NSFont.monospacedDigitSystemFont(ofSize: 12.5, weight: .semibold)
    }

    public static func labelFont(isScreenActive: Bool = true) -> NSFont {
        return NSFont.systemFont(ofSize: 11.5, weight: .semibold)
    }

    public static func separatorFont(isScreenActive: Bool = true) -> NSFont {
        return NSFont.systemFont(ofSize: 11.5, weight: .regular)
    }

    public static func bracketFont(isScreenActive: Bool = true) -> NSFont {
        let desc = NSFont.systemFont(ofSize: 21.0, weight: isScreenActive ? .regular : .medium).fontDescriptor.addingAttributes([
            .traits: [NSFontDescriptor.TraitKey.width: -0.3]
        ])
        return NSFont(descriptor: desc, size: 21.0) ?? NSFont.systemFont(ofSize: 21.0, weight: isScreenActive ? .regular : .medium)
    }

    public static func bracketBaselineOffset(isScreenActive: Bool = true) -> CGFloat {
        return -2.4
    }

    public static func separatorColor(isScreenActive: Bool = true) -> NSColor {
        return isScreenActive
            ? NSColor(white: 0.65, alpha: 1.0)
            : NSColor(white: 0.55, alpha: 1.0)
    }

    public static func textShadow(isScreenActive: Bool = true) -> NSShadow {
        let shadow = NSShadow()
        shadow.shadowColor = NSColor.black.withAlphaComponent(isScreenActive ? 0.55 : 0.30)
        shadow.shadowOffset = NSSize(width: 0, height: -0.5)
        shadow.shadowBlurRadius = 1.0
        return shadow
    }

    public static func bracketShadow(isScreenActive: Bool = true) -> NSShadow {
        let shadow = NSShadow()
        shadow.shadowColor = NSColor.black.withAlphaComponent(isScreenActive ? 0.90 : 0.85)
        shadow.shadowOffset = NSSize(width: 0.0, height: -0.5)
        shadow.shadowBlurRadius = 1.0
        return shadow
    }

    /// Soft omnidirectional shadow for red warning text to prevent top/all-edge wash-out on dark menu bar
    public static func redTextShadow(isScreenActive: Bool = true) -> NSShadow {
        let shadow = NSShadow()
        shadow.shadowColor = NSColor.black.withAlphaComponent(isScreenActive ? 0.95 : 0.65)
        shadow.shadowOffset = NSSize(width: 0, height: 0) // Radiates equally in all directions (top, bottom, left, right)
        shadow.shadowBlurRadius = isScreenActive ? 1.4 : 1.2
        return shadow
    }

    public static func isRedColor(_ color: NSColor) -> Bool {
        guard let rgb = color.usingColorSpace(.sRGB) else { return false }
        return rgb.redComponent > 0.75 && rgb.greenComponent < 0.60
    }

    public static func isScreenActive(
        itemScreenID: CGDirectDisplayID?,
        activeScreenID: CGDirectDisplayID?,
        screenCount: Int = NSScreen.screens.count
    ) -> Bool {
        guard screenCount > 1 else { return true }
        guard let itemID = itemScreenID, let activeID = activeScreenID else {
            return true
        }
        return itemID == activeID
    }

    public static func isCurrentScreenActive(itemWindow: NSWindow?) -> Bool {
        guard NSScreen.screens.count > 1 else { return true }
        guard let itemScreen = itemWindow?.screen,
              let activeScreen = NSScreen.main else {
            return true
        }
        let itemID = (itemScreen.deviceDescription[NSDeviceDescriptionKey("NSScreenNumber")] as? NSNumber)?.uint32Value
        let activeID = (activeScreen.deviceDescription[NSDeviceDescriptionKey("NSScreenNumber")] as? NSNumber)?.uint32Value
        return isScreenActive(
            itemScreenID: itemID,
            activeScreenID: activeID,
            screenCount: NSScreen.screens.count
        )
    }

    // MARK: - 3D Glossy Split Circle
    public static func makeGlossySplitCircle(
        leftPct: Double,
        rightPct: Double,
        diameter: CGFloat = 12.5,
        isScreenActive: Bool = true
    ) -> NSImage {
        return NSImage(size: NSSize(width: diameter, height: diameter), flipped: false) { rect in
            guard let ctx = NSGraphicsContext.current?.cgContext else { return false }

            let leftType = QuotaColorType.from(pct: leftPct)
            let rightType = QuotaColorType.from(pct: rightPct)

            // 1. Clip to circle
            ctx.saveGState()
            let circlePath = CGPath(ellipseIn: rect, transform: nil)
            ctx.addPath(circlePath)
            ctx.clip()

            let colorSpace = CGColorSpaceCreateDeviceRGB()

            // 2. Draw Left Half (5h sprint)
            ctx.saveGState()
            ctx.clip(to: CGRect(x: 0, y: 0, width: diameter / 2.0, height: diameter))
            let leftCGColors = leftType.colors(isScreenActive: isScreenActive).map { $0.cgColor } as CFArray
            if let leftGrad = CGGradient(colorsSpace: colorSpace, colors: leftCGColors, locations: [0.0, 0.55, 1.0]) {
                ctx.drawLinearGradient(leftGrad, start: CGPoint(x: 0, y: diameter), end: CGPoint(x: 0, y: 0), options: [])
            }
            ctx.restoreGState()

            // 3. Draw Right Half (Weekly quota)
            ctx.saveGState()
            ctx.clip(to: CGRect(x: diameter / 2.0, y: 0, width: diameter / 2.0, height: diameter))
            let rightCGColors = rightType.colors(isScreenActive: isScreenActive).map { $0.cgColor } as CFArray
            if let rightGrad = CGGradient(colorsSpace: colorSpace, colors: rightCGColors, locations: [0.0, 0.55, 1.0]) {
                ctx.drawLinearGradient(rightGrad, start: CGPoint(x: 0, y: diameter), end: CGPoint(x: 0, y: 0), options: [])
            }
            ctx.restoreGState()

            // 4. Subtle vertical micro-seam
            let seamAlpha: CGFloat = isScreenActive ? 0.35 : 0.20
            ctx.setFillColor(NSColor(white: 0.0, alpha: seamAlpha).cgColor)
            ctx.fill(CGRect(x: (diameter / 2.0) - 0.35, y: 0, width: 0.7, height: diameter))

            // 5. Glossy 3D specular highlight at the top
            let glossRect = CGRect(x: diameter * 0.18, y: diameter * 0.52, width: diameter * 0.64, height: diameter * 0.40)
            let glossPath = CGPath(ellipseIn: glossRect, transform: nil)
            ctx.saveGState()
            ctx.addPath(glossPath)
            ctx.clip()
            let highlightAlpha: CGFloat = isScreenActive ? 0.65 : 0.30
            let glossColors = [NSColor(white: 1.0, alpha: highlightAlpha).cgColor, NSColor(white: 1.0, alpha: 0.0).cgColor] as CFArray
            if let glossGrad = CGGradient(colorsSpace: colorSpace, colors: glossColors, locations: [0.0, 1.0]) {
                ctx.drawLinearGradient(glossGrad, start: CGPoint(x: 0, y: diameter * 0.92), end: CGPoint(x: 0, y: diameter * 0.52), options: [])
            }
            ctx.restoreGState()

            ctx.restoreGState()

            // 6. Deep 3D outer rim
            let rimAlpha: CGFloat = isScreenActive ? 0.65 : 0.60
            let rimLineWidth: CGFloat = 0.7
            ctx.setStrokeColor(NSColor(white: 0.0, alpha: rimAlpha).cgColor)
            ctx.setLineWidth(rimLineWidth)
            ctx.strokeEllipse(in: rect.insetBy(dx: 0.35, dy: 0.35))

            return true
        }
    }

    // MARK: - 3-Row Hybrid Quota Indicator (Rectangular with Uniform Width)
    // Row 1 (Top): 5-Hour sprint quota (tallest: 7.5 / 16.5)
    // Row 2 (Middle): Weekly quota (mid: 5.8 / 16.5)
    // Row 3 (Bottom): Reset credits / projected status (strip: 3.2 / 16.5)
    public static func makeHybridQuotaIndicator(
        fiveHour: Double,
        weekly: Double,
        credits: Int = 0,
        width: CGFloat = 13.0,
        height: CGFloat = 16.5,
        isScreenActive: Bool = true
    ) -> NSImage {
        return NSImage(size: NSSize(width: width, height: height), flipped: false) { rect in
            guard let ctx = NSGraphicsContext.current?.cgContext else { return false }

            let colorSpace = CGColorSpaceCreateDeviceRGB()
            let cornerR: CGFloat = 1.2
            // Inset by 0.4pt so the outer hairline stroke stays completely within bounds
            let badgeRect = CGRect(x: 0.4, y: 0.4, width: width - 0.8, height: height - 0.8)
            let contour = CGPath(roundedRect: badgeRect, cornerWidth: cornerR, cornerHeight: cornerR, transform: nil)

            // Proportional segment heights matching design calibration:
            // Top (5h sprint): tallest (7.5 pt of 16.5)
            // Mid (weekly remaining): slightly smaller (5.8 pt of 16.5)
            // Bottom (credits): compact strip (3.2 pt of 16.5)
            let topHeight: CGFloat = badgeRect.height * (7.5 / 16.5)
            let midHeight: CGFloat = badgeRect.height * (5.8 / 16.5)
            let stripHeight: CGFloat = badgeRect.height * (3.2 / 16.5)
            let midTop = badgeRect.minY + stripHeight + midHeight
            let midBottom = badgeRect.minY + stripHeight

            var f5hType = QuotaColorType.from(pct: fiveHour)
            let wType = QuotaColorType.from(pct: weekly)
            let cType: QuotaColorType = credits > 0 ? .green : (fiveHour > 20.0 ? .yellow : .red)

            // Never show green on top of red / exhausted weekly quota:
            if isWeeklyExhausted(weekly) || (wType == .red && f5hType == .green) {
                f5hType = .gray
            }

            ctx.saveGState()
            ctx.addPath(contour)
            ctx.clip()

            // 1. Top Section (5h sprint) - Tallest
            ctx.saveGState()
            ctx.clip(to: CGRect(x: badgeRect.minX, y: midTop, width: badgeRect.width, height: topHeight))
            let f5hColors = f5hType.colors(isScreenActive: isScreenActive).map { $0.cgColor } as CFArray
            if let grad = CGGradient(colorsSpace: colorSpace, colors: f5hColors, locations: [0.0, 0.55, 1.0]) {
                ctx.drawLinearGradient(grad, start: CGPoint(x: badgeRect.minX, y: badgeRect.maxY), end: CGPoint(x: badgeRect.minX, y: midTop), options: [])
            }
            ctx.restoreGState()

            // 2. Middle Section (Weekly remaining)
            ctx.saveGState()
            ctx.clip(to: CGRect(x: badgeRect.minX, y: midBottom, width: badgeRect.width, height: midHeight))
            let wColors = wType.colors(isScreenActive: isScreenActive).map { $0.cgColor } as CFArray
            if let grad = CGGradient(colorsSpace: colorSpace, colors: wColors, locations: [0.0, 0.55, 1.0]) {
                ctx.drawLinearGradient(grad, start: CGPoint(x: badgeRect.minX, y: midTop), end: CGPoint(x: badgeRect.minX, y: midBottom), options: [])
            }
            ctx.restoreGState()

            // 3. Bottom Strip (Reset Credits)
            ctx.saveGState()
            ctx.clip(to: CGRect(x: badgeRect.minX, y: badgeRect.minY, width: badgeRect.width, height: stripHeight))
            let cColors = cType.colors(isScreenActive: isScreenActive).map { $0.cgColor } as CFArray
            if let grad = CGGradient(colorsSpace: colorSpace, colors: cColors, locations: [0.0, 0.55, 1.0]) {
                ctx.drawLinearGradient(grad, start: CGPoint(x: badgeRect.minX, y: midBottom), end: CGPoint(x: badgeRect.minX, y: badgeRect.minY), options: [])
            }
            ctx.restoreGState()

            // 4. Dividing micro-seams
            let seamAlpha: CGFloat = isScreenActive ? 0.45 : 0.25
            ctx.setFillColor(NSColor(white: 0.0, alpha: seamAlpha).cgColor)
            ctx.fill(CGRect(x: badgeRect.minX, y: midTop - 0.35, width: badgeRect.width, height: 0.7))
            ctx.fill(CGRect(x: badgeRect.minX, y: midBottom - 0.35, width: badgeRect.width, height: 0.7))

            ctx.restoreGState()

            // 5. Crisp, thin dark/black outer rim ("тёмная, чёрная каёмочка снаружи")
            let rimAlpha: CGFloat = isScreenActive ? 0.85 : 0.80
            let rimLineWidth: CGFloat = 0.6 // Crisp thin black stroke
            ctx.saveGState()
            ctx.setStrokeColor(NSColor(white: 0.0, alpha: rimAlpha).cgColor)
            ctx.setLineWidth(rimLineWidth)
            ctx.addPath(contour)
            ctx.strokePath()
            ctx.restoreGState()

            return true
        }
    }

    // MARK: - Contrast-Boosted Menu Bar Icon
    public static func makeBoostedIcon(from baseImage: NSImage, isScreenActive: Bool = true) -> NSImage {
        baseImage.isTemplate = false
        return baseImage
    }

    // High-contrast dual-color progress bar
    public static func makeColoredProgressBar(label: String, percentage: Double, maxPercentage: Double = 100.0, fillColor: NSColor) -> NSMutableAttributedString {
        let result = NSMutableAttributedString()

        result.append(NSAttributedString(string: label, attributes: [
            .font: NSFont.systemFont(ofSize: 12, weight: .semibold),
            .foregroundColor: NSColor.labelColor
        ]))

        let total = 10
        let denom = max(1.0, maxPercentage)
        let filled = max(0, min(total, Int(round((percentage / denom) * Double(total)))))
        let empty = total - filled

        let barFont = NSFont.systemFont(ofSize: 10, weight: .bold)

        if filled > 0 {
            result.append(NSAttributedString(string: String(repeating: "█", count: filled), attributes: [
                .font: barFont,
                .foregroundColor: fillColor
            ]))
        }
        if empty > 0 {
            result.append(NSAttributedString(string: String(repeating: "░", count: empty), attributes: [
                .font: barFont,
                .foregroundColor: NSColor.quaternaryLabelColor
            ]))
        }

        return result
    }

    // MARK: - Mini Quota Icons (👾 Retro "5h" Sprint & 📅 Weekly Limit)

    /// Generates a pixelated retro "5h" bitmap icon (styled like Windows 3.1 / Windows 95 system fonts)
    /// for the 5-hour sprint quota.
    ///
    /// The glyph matrix is 9 columns wide by 7 rows high (origin bottom-left):
    /// - '5' (4 cols x 7 rows)
    /// - 1 column space
    /// - 'h' (4 cols x 7 rows)
    /// Each pixel is rendered with a dark 1px drop shadow for that iconic classic Win3.1/Win95 relief.
    public static func makeSprintIcon(size: CGFloat = 8.0, isScreenActive: Bool = true) -> NSImage {
        // Glyph bitmap matrix: 7 rows from row 0 (bottom) to row 6 (top).
        // 9 columns: [0..3: '5', 4: gap, 5..8: 'h']
        let bitmapRows: [[UInt8]] = [
            // Row 0 (bottom of '5' & 'h' legs)
            [1, 1, 1, 0, 0, 1, 0, 0, 1],
            // Row 1
            [0, 0, 0, 1, 0, 1, 0, 0, 1],
            // Row 2
            [0, 0, 0, 1, 0, 1, 0, 0, 1],
            // Row 3 (middle crossbar of '5' & arch of 'h')
            [1, 1, 1, 0, 0, 1, 1, 1, 0],
            // Row 4 (upper arm of '5' & ascender of 'h')
            [1, 0, 0, 0, 0, 1, 0, 0, 0],
            // Row 5 (ascender of 'h')
            [1, 0, 0, 0, 0, 1, 0, 0, 0],
            // Row 6 (top bar of '5' & ascender of 'h')
            [1, 1, 1, 1, 0, 1, 0, 0, 0]
        ]

        let cols = 9
        let rows = 7
        // Pixel scale factor based on requested size (defaults to 1.0pt per pixel on Retina)
        let pixelSize: CGFloat = max(0.9, size / 7.0)
        let totalW = ceil(CGFloat(cols) * pixelSize) + 1.0 // extra pixel for shadow
        let totalH = ceil(CGFloat(rows) * pixelSize) + 1.0

        let img = NSImage(size: NSSize(width: totalW, height: totalH), flipped: false) { rect in
            guard let ctx = NSGraphicsContext.current?.cgContext else { return false }

            // Saturated, juicy sunlit electric gold palette:
            // Top highlight: luminous sunlit citrus
            // Mid body: rich, juicy electric gold
            // Bottom base: warm vibrant amber-gold
            let topColor = isScreenActive
                ? NSColor(red: 1.0, green: 1.0, blue: 0.40, alpha: 1.0)
                : NSColor(red: 0.95, green: 0.94, blue: 0.45, alpha: 1.0)
            let midColor = isScreenActive
                ? NSColor(red: 1.0, green: 0.88, blue: 0.02, alpha: 1.0)
                : NSColor(red: 0.92, green: 0.82, blue: 0.08, alpha: 1.0)
            let bottomColor = isScreenActive
                ? NSColor(red: 1.0, green: 0.74, blue: 0.0, alpha: 1.0)
                : NSColor(red: 0.88, green: 0.68, blue: 0.0, alpha: 1.0)

            let shadowAlpha: CGFloat = isScreenActive ? 0.52 : 0.35
            let shadowColor = NSColor(white: 0.0, alpha: shadowAlpha).cgColor
            let shadowOffset = CGSize(width: pixelSize * 0.60, height: -pixelSize * 0.60)

            // Pass 1: Crisp drop shadow for razor-sharp legibility on light wallpapers
            ctx.setFillColor(shadowColor)
            for r in 0..<rows {
                let y = CGFloat(r) * pixelSize + shadowOffset.height + 0.5
                for c in 0..<cols {
                    if bitmapRows[r][c] == 1 {
                        let x = CGFloat(c) * pixelSize + shadowOffset.width
                        ctx.fill(CGRect(x: x, y: y, width: pixelSize, height: pixelSize))
                    }
                }
            }

            // Pass 2: Juicy gradient clipped strictly to pixel glyph contours
            ctx.saveGState()
            for r in 0..<rows {
                let y = CGFloat(r) * pixelSize + 0.5
                for c in 0..<cols {
                    if bitmapRows[r][c] == 1 {
                        let x = CGFloat(c) * pixelSize
                        ctx.addRect(CGRect(x: x, y: y, width: pixelSize, height: pixelSize))
                    }
                }
            }
            ctx.clip()

            let colors = [topColor.cgColor, midColor.cgColor, bottomColor.cgColor] as CFArray
            let colorSpace = CGColorSpaceCreateDeviceRGB()
            if let grad = CGGradient(colorsSpace: colorSpace, colors: colors, locations: [0.0, 0.52, 1.0]) {
                let start = CGPoint(x: 0, y: CGFloat(rows) * pixelSize + 0.5)
                let end = CGPoint(x: 0, y: 0.5)
                ctx.drawLinearGradient(grad, start: start, end: end, options: [])
            }
            ctx.restoreGState()

            return true
        }
        img.isTemplate = false
        return img
    }

    /// Generates a pixel-perfect retro calendar bitmap icon (Win95 style) for the weekly quota.
    ///
    /// The bitmap is 9 columns × 9 rows (origin bottom-left):
    /// - Row 8 (top):    binder pegs (2 filled pixels)
    /// - Rows 6-7:       blue header bar
    /// - Rows 0-5:       white body with date dots
    /// Each pixel has a dark 1px drop shadow for classic Win3.1/Win95 relief.
    public static func makeWeeklyIcon(size: CGFloat = 10.0, isScreenActive: Bool = true) -> NSImage {
        // Calendar bitmap: 9 rows from row 0 (bottom) to row 8 (top).
        // 0 = empty, 1 = body (white/light), 2 = header (blue), 3 = peg (dark), 4 = date dot (dark on body)
        let bitmapRows: [[UInt8]] = [
            // Row 0 (bottom edge of body)
            [1, 1, 1, 1, 1, 1, 1, 1, 1],
            // Row 1 (date dots bottom row)
            [1, 4, 1, 4, 1, 4, 1, 4, 1],
            // Row 2 (body spacer)
            [1, 1, 1, 1, 1, 1, 1, 1, 1],
            // Row 3 (date dots top row)
            [1, 4, 1, 4, 1, 4, 1, 1, 1],
            // Row 4 (body spacer)
            [1, 1, 1, 1, 1, 1, 1, 1, 1],
            // Row 5 (header bottom edge / divider)
            [2, 2, 2, 2, 2, 2, 2, 2, 2],
            // Row 6 (header)
            [2, 2, 2, 2, 2, 2, 2, 2, 2],
            // Row 7 (header top edge)
            [2, 2, 2, 2, 2, 2, 2, 2, 2],
            // Row 8 (binder pegs)
            [0, 0, 3, 0, 0, 0, 3, 0, 0]
        ]

        let cols = 9
        let rows = 9
        let pixelSize: CGFloat = max(0.9, size / 9.0)
        let totalW = ceil(CGFloat(cols) * pixelSize) + 1.0 // extra for shadow
        let totalH = ceil(CGFloat(rows) * pixelSize) + 1.0

        let img = NSImage(size: NSSize(width: totalW, height: totalH), flipped: false) { rect in
            guard let ctx = NSGraphicsContext.current?.cgContext else { return false }

            // Colors
            let bodyColor = isScreenActive
                ? NSColor(white: 0.95, alpha: 1.0).cgColor
                : NSColor(white: 0.85, alpha: 1.0).cgColor
            let headerColor = isScreenActive
                ? NSColor(red: 0.20, green: 0.70, blue: 1.0, alpha: 1.0).cgColor
                : NSColor(red: 0.18, green: 0.60, blue: 0.90, alpha: 1.0).cgColor
            let pegColor = isScreenActive
                ? NSColor(white: 0.35, alpha: 1.0).cgColor
                : NSColor(white: 0.30, alpha: 1.0).cgColor
            let dotColor = isScreenActive
                ? NSColor(white: 0.28, alpha: 1.0).cgColor
                : NSColor(white: 0.24, alpha: 1.0).cgColor
            let shadowColor = NSColor(white: 0.0, alpha: isScreenActive ? 0.75 : 0.45).cgColor

            let shadowOffset = CGSize(width: pixelSize * 0.75, height: -pixelSize * 0.75)

            // Pass 1: Draw shadow for all filled pixels
            ctx.setFillColor(shadowColor)
            for r in 0..<rows {
                let y = CGFloat(r) * pixelSize + shadowOffset.height + 0.5
                for c in 0..<cols {
                    if bitmapRows[r][c] != 0 {
                        let x = CGFloat(c) * pixelSize + shadowOffset.width
                        ctx.fill(CGRect(x: x, y: y, width: pixelSize, height: pixelSize))
                    }
                }
            }

            // Pass 2: Draw crisp pixel-art calendar
            for r in 0..<rows {
                let y = CGFloat(r) * pixelSize + 0.5
                for c in 0..<cols {
                    let val = bitmapRows[r][c]
                    if val == 0 { continue }
                    let color: CGColor
                    switch val {
                    case 1: color = bodyColor
                    case 2: color = headerColor
                    case 3: color = pegColor
                    case 4: color = dotColor
                    default: continue
                    }
                    ctx.setFillColor(color)
                    let x = CGFloat(c) * pixelSize
                    ctx.fill(CGRect(x: x, y: y, width: pixelSize, height: pixelSize))
                }
            }

            return true
        }
        img.isTemplate = false
        return img
    }

    // MARK: - Stacked Percentage Indicator (2-Row Compact Layout)

    /// Generates a crisp vector 2-row stacked percentage image for the status bar.
    /// Row 1 (Top): 5-Hour sprint icon / label + percentage
    /// Row 2 (Bottom): Weekly limit icon / label + percentage
    public static func makeStackedValuesImage(
        fiveHPct: String,
        fiveHColor: NSColor,
        weeklyPct: String,
        weeklyColor: NSColor,
        isScreenActive: Bool = true,
        useQuotaIcons: Bool = true
    ) -> (image: NSImage, size: NSSize) {
        let font = NSFont.monospacedDigitSystemFont(
            ofSize: 10.5,
            weight: .bold
        )
        let defaultShadow = textShadow(isScreenActive: isScreenActive)
        let redShadow = redTextShadow(isScreenActive: isScreenActive)
        let kern: CGFloat = 0.2

        let sprintIconSample = useQuotaIcons ? makeSprintIcon(size: 8.0, isScreenActive: isScreenActive) : nil
        let sprintIconW: CGFloat = sprintIconSample?.size.width ?? 10.0
        let sprintIconH: CGFloat = sprintIconSample?.size.height ?? 8.0
        let weeklyIconSample = useQuotaIcons ? makeWeeklyIcon(size: 10.0, isScreenActive: isScreenActive) : nil
        let weeklyIconW: CGFloat = weeklyIconSample?.size.width ?? 10.0
        let weeklyIconH: CGFloat = weeklyIconSample?.size.height ?? 10.0
        let iconSpacing: CGFloat = 1.5
        let totalHeight: CGFloat = 20.5
        let rowHeight = totalHeight / 2.0 // 10.25 pt

        let f5hAttrs: [NSAttributedString.Key: Any] = [
            .font: font,
            .foregroundColor: fiveHColor,
            .shadow: isRedColor(fiveHColor) ? redShadow : defaultShadow,
            .kern: kern
        ]
        let wAttrs: [NSAttributedString.Key: Any] = [
            .font: font,
            .foregroundColor: weeklyColor,
            .shadow: isRedColor(weeklyColor) ? redShadow : defaultShadow,
            .kern: kern
        ]

        let f5hStr = NSAttributedString(string: fiveHPct, attributes: f5hAttrs)
        let wStr = NSAttributedString(string: weeklyPct, attributes: wAttrs)

        let f5hTextSize = f5hStr.size()
        let wTextSize = wStr.size()

        // Text fallback labels ("5h: ", "Wk: ") when useQuotaIcons is false
        let labelF = NSFont.systemFont(ofSize: 8.5, weight: .bold)
        let labelAttrs: [NSAttributedString.Key: Any] = [
            .font: labelF,
            .foregroundColor: NSColor.white,
            .shadow: defaultShadow,
            .kern: 0.1
        ]
        let f5hLabelStr = NSAttributedString(string: "5h: ", attributes: labelAttrs)
        let wLabelStr = NSAttributedString(string: "Wk: ", attributes: labelAttrs)
        let f5hLabelSize = f5hLabelStr.size()
        let wLabelSize = wLabelStr.size()

        let leftPad: CGFloat = useQuotaIcons ? 1.5 : 0.0
        let maxIconW = max(sprintIconW, weeklyIconW)
        let prefixWidth: CGFloat = leftPad + (useQuotaIcons
            ? (maxIconW + iconSpacing)
            : (max(f5hLabelSize.width, wLabelSize.width) + iconSpacing))

        let textWidth = max(f5hTextSize.width, wTextSize.width)
        let totalWidth = ceil(prefixWidth + textWidth + (useQuotaIcons ? 1.0 : 0.0))

        let img = NSImage(size: NSSize(width: totalWidth, height: totalHeight), flipped: false) { rect in
            guard let _ = NSGraphicsContext.current?.cgContext else { return false }

            // Row 1: 5-Hour sprint (Top tier: y in [rowHeight, totalHeight])
            let row1Y = rowHeight
            let row1CenterY = row1Y + (rowHeight / 2.0)
            let f5hBaseline = row1CenterY - (font.capHeight / 2.0)
            let f5hTextY = f5hBaseline - abs(font.descender)

            if useQuotaIcons {
                let sprintIcon = sprintIconSample ?? makeSprintIcon(size: 8.0, isScreenActive: isScreenActive)
                let sprintX = leftPad + (maxIconW - sprintIconW) / 2.0
                let sprintIconY = row1CenterY - (sprintIconH / 2.0)
                let iconRect = CGRect(
                    x: sprintX,
                    y: sprintIconY,
                    width: sprintIconW,
                    height: sprintIconH
                )
                sprintIcon.draw(in: iconRect)
            } else {
                let f5hLabelBaseline = row1CenterY - (labelF.capHeight / 2.0)
                let f5hLabelY = f5hLabelBaseline - abs(labelF.descender)
                f5hLabelStr.draw(at: NSPoint(x: 0, y: f5hLabelY))
            }
            f5hStr.draw(at: NSPoint(x: prefixWidth, y: f5hTextY))

            // Row 2: Weekly quota (Bottom tier: y in [0.0, rowHeight])
            let row2Y: CGFloat = 0.0
            let row2CenterY = row2Y + (rowHeight / 2.0)
            let wBaseline = row2CenterY - (font.capHeight / 2.0)
            let wTextY = wBaseline - abs(font.descender)

            if useQuotaIcons {
                let weeklyIcon = weeklyIconSample ?? makeWeeklyIcon(size: 10.0, isScreenActive: isScreenActive)
                let weeklyX = leftPad + (maxIconW - weeklyIconW) / 2.0
                let weeklyIconY = row2CenterY - (weeklyIconH / 2.0)
                let iconRect = CGRect(
                    x: weeklyX,
                    y: weeklyIconY,
                    width: weeklyIconW,
                    height: weeklyIconH
                )
                weeklyIcon.draw(in: iconRect)
            } else {
                let wLabelBaseline = row2CenterY - (labelF.capHeight / 2.0)
                let wLabelY = wLabelBaseline - abs(labelF.descender)
                wLabelStr.draw(at: NSPoint(x: 0, y: wLabelY))
            }
            wStr.draw(at: NSPoint(x: prefixWidth, y: wTextY))

            return true
        }
        img.isTemplate = false
        return (img, NSSize(width: totalWidth, height: totalHeight))
    }

    /// Convenience overload with useModelIcons argument label for cross-monitor compatibility.
    public static func makeStackedValuesImage(
        fiveHPct: String,
        fiveHColor: NSColor,
        weeklyPct: String,
        weeklyColor: NSColor,
        isScreenActive: Bool = true,
        useModelIcons: Bool
    ) -> (image: NSImage, size: NSSize) {
        return makeStackedValuesImage(
            fiveHPct: fiveHPct,
            fiveHColor: fiveHColor,
            weeklyPct: weeklyPct,
            weeklyColor: weeklyColor,
            isScreenActive: isScreenActive,
            useQuotaIcons: useModelIcons
        )
    }

    /// Generates an NSTextAttachment containing the 2-row stacked percentage indicator.
    public static func makeStackedValuesAttachment(
        fiveHPct: String,
        fiveHColor: NSColor,
        weeklyPct: String,
        weeklyColor: NSColor,
        isScreenActive: Bool = true,
        useQuotaIcons: Bool = true
    ) -> NSTextAttachment {
        let (image, size) = makeStackedValuesImage(
            fiveHPct: fiveHPct,
            fiveHColor: fiveHColor,
            weeklyPct: weeklyPct,
            weeklyColor: weeklyColor,
            isScreenActive: isScreenActive,
            useQuotaIcons: useQuotaIcons
        )
        let attachment = NSTextAttachment()
        attachment.image = image
        attachment.bounds = CGRect(x: 0, y: -6.5, width: size.width, height: size.height)
        return attachment
    }

    /// Convenience overload with useModelIcons argument label for cross-monitor compatibility.
    public static func makeStackedValuesAttachment(
        fiveHPct: String,
        fiveHColor: NSColor,
        weeklyPct: String,
        weeklyColor: NSColor,
        isScreenActive: Bool = true,
        useModelIcons: Bool
    ) -> NSTextAttachment {
        return makeStackedValuesAttachment(
            fiveHPct: fiveHPct,
            fiveHColor: fiveHColor,
            weeklyPct: weeklyPct,
            weeklyColor: weeklyColor,
            isScreenActive: isScreenActive,
            useQuotaIcons: useModelIcons
        )
    }
}
