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
                    NSColor(red: 0.35, green: 0.98, blue: 0.45, alpha: 1.0),
                    NSColor(red: 0.0, green: 0.76, blue: 0.22, alpha: 1.0),
                    NSColor(red: 0.0, green: 0.45, blue: 0.10, alpha: 1.0)
                ]
            case .yellow:
                return [
                    NSColor(red: 1.0, green: 0.92, blue: 0.35, alpha: 1.0),
                    NSColor(red: 0.98, green: 0.65, blue: 0.0, alpha: 1.0),
                    NSColor(red: 0.75, green: 0.42, blue: 0.0, alpha: 1.0)
                ]
            case .red:
                return [
                    NSColor(red: 1.0, green: 0.42, blue: 0.42, alpha: 1.0),
                    NSColor(red: 0.88, green: 0.08, blue: 0.12, alpha: 1.0),
                    NSColor(red: 0.55, green: 0.02, blue: 0.04, alpha: 1.0)
                ]
            case .gray:
                return [
                    NSColor(red: 0.72, green: 0.73, blue: 0.75, alpha: 1.0),
                    NSColor(red: 0.52, green: 0.53, blue: 0.56, alpha: 1.0),
                    NSColor(red: 0.32, green: 0.33, blue: 0.35, alpha: 1.0)
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
                    NSColor(red: 0.95, green: 0.85, blue: 0.30, alpha: 1.0),
                    NSColor(red: 0.90, green: 0.58, blue: 0.0, alpha: 1.0),
                    NSColor(red: 0.68, green: 0.38, blue: 0.0, alpha: 1.0)
                ]
            case .red:
                return [
                    NSColor(red: 0.92, green: 0.38, blue: 0.38, alpha: 1.0),
                    NSColor(red: 0.80, green: 0.08, blue: 0.12, alpha: 1.0),
                    NSColor(red: 0.50, green: 0.02, blue: 0.04, alpha: 1.0)
                ]
            case .gray:
                return [
                    NSColor(red: 0.65, green: 0.66, blue: 0.68, alpha: 1.0),
                    NSColor(red: 0.48, green: 0.49, blue: 0.52, alpha: 1.0),
                    NSColor(red: 0.30, green: 0.31, blue: 0.33, alpha: 1.0)
                ]
            }
        }
    }

    public var colors: [NSColor] {
        return colors(isScreenActive: true)
    }

    public static func from(pct: Double) -> QuotaColorType {
        if pct > 50.0 { return .green }
        if pct > 15.0 { return .yellow }
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
                ? NSColor(white: 0.60, alpha: 1.0)
                : NSColor(white: 0.55, alpha: 1.0)
        }
        let tankPct = planMultiplier > 0.0 ? (pct / planMultiplier) : pct
        if isScreenActive {
            if tankPct > 50.0 {
                return NSColor(red: 0.0, green: 0.88, blue: 0.35, alpha: 1.0)
            } else if tankPct > 15.0 {
                return NSColor(red: 1.0, green: 0.78, blue: 0.0, alpha: 1.0)
            } else {
                return NSColor(red: 1.0, green: 0.18, blue: 0.22, alpha: 1.0)
            }
        } else {
            if tankPct > 50.0 {
                return NSColor(red: 0.0, green: 0.80, blue: 0.32, alpha: 1.0)
            } else if tankPct > 15.0 {
                return NSColor(red: 0.92, green: 0.70, blue: 0.0, alpha: 1.0)
            } else {
                return NSColor(red: 0.92, green: 0.18, blue: 0.20, alpha: 1.0)
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
                    ? NSColor(white: 0.65, alpha: 1.0)
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
            } else if tankPct > 15.0 {
                return isDark
                    ? NSColor(red: 1.0, green: 0.80, blue: 0.0, alpha: 1.0)
                    : NSColor(red: 0.82, green: 0.52, blue: 0.0, alpha: 1.0)
            } else {
                return isDark
                    ? NSColor(red: 1.0, green: 0.25, blue: 0.28, alpha: 1.0)
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
        return NSFont.systemFont(ofSize: 12.0, weight: .bold)
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
            let rimAlpha: CGFloat = isScreenActive ? 0.65 : 0.40
            let rimLineWidth: CGFloat = 0.7
            ctx.setStrokeColor(NSColor(white: 0.0, alpha: rimAlpha).cgColor)
            ctx.setLineWidth(rimLineWidth)
            ctx.strokeEllipse(in: rect.insetBy(dx: 0.35, dy: 0.35))

            return true
        }
    }

    // MARK: - 3-Row Hybrid Quota Indicator (Shield Badge)
    // Row 1 (Top semicircle): 5-Hour sprint quota
    // Row 2 (Middle rectangle): Weekly quota
    // Row 3 (Bottom strip): Reset credits / projected status
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
            let radius = width / 2.0
            let stripHeight: CGFloat = 3.5
            let midBottom: CGFloat = stripHeight
            let midTop: CGFloat = height - radius

            let rawF5hType = QuotaColorType.from(pct: fiveHour)
            let wType = QuotaColorType.from(pct: weekly)
            let f5hType: QuotaColorType
            if isWeeklyExhausted(weekly) || (wType == .red && rawF5hType == .green) {
                f5hType = .gray
            } else {
                f5hType = rawF5hType
            }
            let cType: QuotaColorType = credits > 0 ? .green : (fiveHour > 15.0 ? .yellow : .red)

            // Outer contour path: rounded bottom corners + straight sides + top semicircle
            let cornerR: CGFloat = 2.0
            let contour = CGMutablePath()
            contour.move(to: CGPoint(x: cornerR, y: 0))
            contour.addLine(to: CGPoint(x: width - cornerR, y: 0))
            contour.addQuadCurve(to: CGPoint(x: width, y: cornerR), control: CGPoint(x: width, y: 0))
            contour.addLine(to: CGPoint(x: width, y: midTop))
            contour.addArc(center: CGPoint(x: radius, y: midTop), radius: radius, startAngle: 0, endAngle: .pi, clockwise: false)
            contour.addLine(to: CGPoint(x: 0, y: cornerR))
            contour.addQuadCurve(to: CGPoint(x: cornerR, y: 0), control: CGPoint(x: 0, y: 0))
            contour.closeSubpath()

            ctx.saveGState()
            ctx.addPath(contour)
            ctx.clip()

            // 1. Top Section (5h sprint)
            ctx.saveGState()
            ctx.clip(to: CGRect(x: 0, y: midTop, width: width, height: radius))
            let f5hColors = f5hType.colors(isScreenActive: isScreenActive).map { $0.cgColor } as CFArray
            if let grad = CGGradient(colorsSpace: colorSpace, colors: f5hColors, locations: [0.0, 0.55, 1.0]) {
                ctx.drawLinearGradient(grad, start: CGPoint(x: 0, y: height), end: CGPoint(x: 0, y: midTop), options: [])
            }
            ctx.restoreGState()

            // 2. Middle Section (Weekly remaining)
            let midHeight = midTop - midBottom
            ctx.saveGState()
            ctx.clip(to: CGRect(x: 0, y: midBottom, width: width, height: midHeight))
            let wColors = wType.colors(isScreenActive: isScreenActive).map { $0.cgColor } as CFArray
            if let grad = CGGradient(colorsSpace: colorSpace, colors: wColors, locations: [0.0, 0.55, 1.0]) {
                ctx.drawLinearGradient(grad, start: CGPoint(x: 0, y: midTop), end: CGPoint(x: 0, y: midBottom), options: [])
            }
            ctx.restoreGState()

            // 3. Bottom Strip (Reset Credits)
            ctx.saveGState()
            ctx.clip(to: CGRect(x: 0, y: 0, width: width, height: stripHeight))
            let cColors = cType.colors(isScreenActive: isScreenActive).map { $0.cgColor } as CFArray
            if let grad = CGGradient(colorsSpace: colorSpace, colors: cColors, locations: [0.0, 0.55, 1.0]) {
                ctx.drawLinearGradient(grad, start: CGPoint(x: 0, y: stripHeight), end: CGPoint(x: 0, y: 0), options: [])
            }
            ctx.restoreGState()

            // 4. Dividing micro-seams
            let seamAlpha: CGFloat = isScreenActive ? 0.45 : 0.25
            ctx.setFillColor(NSColor(white: 0.0, alpha: seamAlpha).cgColor)
            ctx.fill(CGRect(x: 0, y: midTop - 0.35, width: width, height: 0.7))
            ctx.fill(CGRect(x: 0, y: midBottom - 0.35, width: width, height: 0.7))

            // 5. Glossy 3D specular highlight at top semicircle
            let glossRect = CGRect(x: width * 0.18, y: height - (radius * 0.9), width: width * 0.64, height: radius * 0.6)
            let glossPath = CGPath(ellipseIn: glossRect, transform: nil)
            ctx.saveGState()
            ctx.addPath(glossPath)
            ctx.clip()
            let highlightAlpha: CGFloat = isScreenActive ? 0.60 : 0.30
            let glossColors = [NSColor(white: 1.0, alpha: highlightAlpha).cgColor, NSColor(white: 1.0, alpha: 0.0).cgColor] as CFArray
            if let glossGrad = CGGradient(colorsSpace: colorSpace, colors: glossColors, locations: [0.0, 1.0]) {
                ctx.drawLinearGradient(glossGrad, start: CGPoint(x: 0, y: height * 0.98), end: CGPoint(x: 0, y: height - radius), options: [])
            }
            ctx.restoreGState()

            ctx.restoreGState()

            // 6. Deep 3D outer rim stroke
            let rimAlpha: CGFloat = isScreenActive ? 0.70 : 0.45
            let rimLineWidth: CGFloat = 0.75
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

            let fgColor = isScreenActive
                ? NSColor(red: 1.0, green: 0.85, blue: 0.20, alpha: 1.0).cgColor
                : NSColor(red: 0.90, green: 0.78, blue: 0.20, alpha: 1.0).cgColor
            let shadowColor = NSColor(white: 0.0, alpha: isScreenActive ? 0.65 : 0.35).cgColor

            let shadowOffset = CGSize(width: pixelSize * 0.75, height: -pixelSize * 0.75)

            // Pass 1: Draw dark 1px shadow (classic Win 3.1 / Win 95 drop shadow)
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

            // Pass 2: Draw crisp retro pixel glyphs in gold/amber
            ctx.setFillColor(fgColor)
            for r in 0..<rows {
                let y = CGFloat(r) * pixelSize + 0.5
                for c in 0..<cols {
                    if bitmapRows[r][c] == 1 {
                        let x = CGFloat(c) * pixelSize
                        ctx.fill(CGRect(x: x, y: y, width: pixelSize, height: pixelSize))
                    }
                }
            }

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
                ? NSColor(white: 0.92, alpha: 1.0).cgColor
                : NSColor(white: 0.82, alpha: 1.0).cgColor
            let headerColor = isScreenActive
                ? NSColor(red: 0.25, green: 0.65, blue: 1.0, alpha: 1.0).cgColor
                : NSColor(red: 0.22, green: 0.55, blue: 0.85, alpha: 1.0).cgColor
            let pegColor = isScreenActive
                ? NSColor(white: 0.35, alpha: 1.0).cgColor
                : NSColor(white: 0.30, alpha: 1.0).cgColor
            let dotColor = isScreenActive
                ? NSColor(white: 0.30, alpha: 1.0).cgColor
                : NSColor(white: 0.25, alpha: 1.0).cgColor
            let shadowColor = NSColor(white: 0.0, alpha: isScreenActive ? 0.65 : 0.35).cgColor

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
        let shadow = textShadow(isScreenActive: isScreenActive)
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
            .shadow: shadow,
            .kern: kern
        ]
        let wAttrs: [NSAttributedString.Key: Any] = [
            .font: font,
            .foregroundColor: weeklyColor,
            .shadow: shadow,
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
            .shadow: shadow,
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
            if useQuotaIcons {
                let sprintIcon = sprintIconSample ?? makeSprintIcon(size: 8.0, isScreenActive: isScreenActive)
                let sprintX = leftPad + (maxIconW - sprintIconW) / 2.0
                let iconRect = CGRect(
                    x: sprintX,
                    y: row1Y + (rowHeight - sprintIconH) / 2.0,
                    width: sprintIconW,
                    height: sprintIconH
                )
                sprintIcon.draw(in: iconRect)
            } else {
                let labelY = row1Y + (rowHeight - f5hLabelSize.height) / 2.0 - 0.5
                f5hLabelStr.draw(at: NSPoint(x: 0, y: labelY))
            }
            let f5hTextY = row1Y + (rowHeight - f5hTextSize.height) / 2.0 - 0.5
            f5hStr.draw(at: NSPoint(x: prefixWidth, y: f5hTextY))

            // Row 2: Weekly quota (Bottom tier: y in [0.0, rowHeight])
            let row2Y: CGFloat = 0.0
            if useQuotaIcons {
                let weeklyIcon = weeklyIconSample ?? makeWeeklyIcon(size: 10.0, isScreenActive: isScreenActive)
                let weeklyX = leftPad + (maxIconW - weeklyIconW) / 2.0
                let iconRect = CGRect(
                    x: weeklyX,
                    y: row2Y + (rowHeight - weeklyIconH) / 2.0,
                    width: weeklyIconW,
                    height: weeklyIconH
                )
                weeklyIcon.draw(in: iconRect)
            } else {
                let labelY = row2Y + (rowHeight - wLabelSize.height) / 2.0 - 0.5
                wLabelStr.draw(at: NSPoint(x: 0, y: labelY))
            }
            let wTextY = row2Y + (rowHeight - wTextSize.height) / 2.0 - 0.5
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
        attachment.bounds = CGRect(x: 0, y: -5.0, width: size.width, height: size.height)
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
