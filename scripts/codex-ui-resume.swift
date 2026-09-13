import Cocoa
import ApplicationServices

struct CandidateButton {
    let element: AXUIElement
    let isPlay: Bool
    let isSteer: Bool
    let isResume: Bool
    let y: CGFloat
}

/// Identifies the non-functional text button inside the "Queue paused because you interrupted" banner
func isBannerResume(title: String, desc: String, width: CGFloat, height: CGFloat) -> Bool {
    let t = title.lowercased().trimmingCharacters(in: .whitespacesAndNewlines)
    let d = desc.lowercased().trimmingCharacters(in: .whitespacesAndNewlines)
    if (t == "resume" || t == "возобновить") && d.isEmpty && width > 50 {
        return true
    }
    return false
}

/// Identifies the circular "Play" button at the bottom-right of the composer (white right-facing triangle)
func isPlayButton(title: String, desc: String, width: CGFloat, height: CGFloat) -> Bool {
    let t = title.lowercased().trimmingCharacters(in: .whitespacesAndNewlines)
    let d = desc.lowercased().trimmingCharacters(in: .whitespacesAndNewlines)
    if (d == "resume" || d == "возобновить" || d == "play" || d == "start") && (t.isEmpty || t == "▶" || t == ">") {
        return true
    }
    return false
}

func isResumeButton(title: String, desc: String) -> Bool {
    let t = title.lowercased().trimmingCharacters(in: .whitespacesAndNewlines)
    let d = desc.lowercased().trimmingCharacters(in: .whitespacesAndNewlines)
    if t == "resume" || d == "resume" || t == "retry" || d == "retry" ||
       t == "возобновить" || d == "возобновить" || t == "повторить" || d == "повторить" {
        return true
    }
    if t == "try again" || d == "try again" || t.starts(with: "try again") || d.starts(with: "try again") {
        return true
    }
    if t == "continue generating" || d == "continue generating" ||
       t.starts(with: "continue generating") || d.starts(with: "continue generating") ||
       t == "продолжить" || d == "продолжить" || t.starts(with: "продолжить") {
        return true
    }
    if d.contains("resume") || d.contains("try sending this queued message again") {
        return true
    }
    if t.starts(with: "resume") || t.starts(with: "retry") || t.starts(with: "возобновить") || t.starts(with: "повторить") {
        return true
    }
    return false
}

func isSteerButton(title: String, desc: String) -> Bool {
    let t = title.lowercased().trimmingCharacters(in: .whitespacesAndNewlines)
    let d = desc.lowercased().trimmingCharacters(in: .whitespacesAndNewlines)
    if t == "steer" || d == "steer" || t == "направить" || d == "направить" {
        return true
    }
    if d.contains("submit without interrupting") || d.contains("steer") {
        return true
    }
    return false
}

func isGeneratingButton(desc: String, title: String) -> Bool {
    let d = desc.lowercased().trimmingCharacters(in: .whitespacesAndNewlines)
    let t = title.lowercased().trimmingCharacters(in: .whitespacesAndNewlines)
    return d == "stop" || t == "stop" || d == "остановить" || d == "зупинити"
}

func pressButton(el: AXUIElement) -> Bool {
    _ = AXUIElementPerformAction(el, kAXPressAction as CFString)
    
    var posVal: AnyObject?
    AXUIElementCopyAttributeValue(el, kAXPositionAttribute as CFString, &posVal)
    var sizeVal: AnyObject?
    AXUIElementCopyAttributeValue(el, kAXSizeAttribute as CFString, &sizeVal)
    
    var point = CGPoint.zero
    var size = CGSize.zero
    if let pv = posVal { AXValueGetValue(pv as! AXValue, .cgPoint, &point) }
    if let sv = sizeVal { AXValueGetValue(sv as! AXValue, .cgSize, &size) }
    
    if size.width > 0 && size.height > 0 {
        let center = CGPoint(x: point.x + size.width / 2.0, y: point.y + size.height / 2.0)
        if let mouseDown = CGEvent(mouseEventSource: nil, mouseType: .leftMouseDown, mouseCursorPosition: center, mouseButton: .left),
           let mouseUp = CGEvent(mouseEventSource: nil, mouseType: .leftMouseUp, mouseCursorPosition: center, mouseButton: .left) {
            mouseDown.post(tap: .cghidEventTap)
            usleep(50000)
            mouseUp.post(tap: .cghidEventTap)
            return true
        }
    }
    return true
}

/// Searches the Accessibility hierarchy of ChatGPT / Codex and performs
/// the circular `Play` action, `Steer` action, or turn `Resume` on any interrupted session.
func resumeChatGPT() -> (success: Bool, outcome: String) {
    guard let app = NSRunningApplication.runningApplications(withBundleIdentifier: "com.openai.codex").first ?? NSRunningApplication.runningApplications(withBundleIdentifier: "com.openai.chat").first else {
        return (false, "APP_NOT_FOUND")
    }
    
    app.activate(options: .activateIgnoringOtherApps)
    
    let axApp = AXUIElementCreateApplication(app.processIdentifier)
    AXUIElementSetAttributeValue(axApp, "AXManualAccessibility" as CFString, true as CFTypeRef)
    AXUIElementSetAttributeValue(axApp, "AXEnhancedUserInterface" as CFString, true as CFTypeRef)
    
    var windows: AnyObject?
    guard AXUIElementCopyAttributeValue(axApp, kAXWindowsAttribute as CFString, &windows) == .success,
          let winList = windows as? [AXUIElement] else {
        return (false, "NO_WINDOWS")
    }
    
    var candidates: [CandidateButton] = []
    var isAlreadyGenerating = false
    
    func collectButtons(el: AXUIElement, depth: Int = 0) {
        if depth > 75 { return }
        var role: AnyObject?
        AXUIElementCopyAttributeValue(el, kAXRoleAttribute as CFString, &role)
        var desc: AnyObject?
        AXUIElementCopyAttributeValue(el, kAXDescriptionAttribute as CFString, &desc)
        var title: AnyObject?
        AXUIElementCopyAttributeValue(el, kAXTitleAttribute as CFString, &title)
        
        let r = (role as? String) ?? ""
        let d = (desc as? String) ?? ""
        let t = (title as? String) ?? ""
        
        if r == "AXButton" || r.contains("Button") {
            var enabledVal: AnyObject?
            if AXUIElementCopyAttributeValue(el, kAXEnabledAttribute as CFString, &enabledVal) == .success,
               let en = enabledVal as? Bool, !en {
                // skip disabled buttons
            } else {
                var posVal: AnyObject?
                AXUIElementCopyAttributeValue(el, kAXPositionAttribute as CFString, &posVal)
                var sizeVal: AnyObject?
                AXUIElementCopyAttributeValue(el, kAXSizeAttribute as CFString, &sizeVal)
                var pt = CGPoint.zero
                var sz = CGSize.zero
                if let pv = posVal { AXValueGetValue(pv as! AXValue, .cgPoint, &pt) }
                if let sv = sizeVal { AXValueGetValue(sv as! AXValue, .cgSize, &sz) }
                
                if isGeneratingButton(desc: d, title: t) && sz.width < 45 && sz.height < 45 {
                    isAlreadyGenerating = true
                }
                
                // Skip the non-functional "Queue paused because you interrupted [Resume]" banner button
                if isBannerResume(title: t, desc: d, width: sz.width, height: sz.height) {
                    // Do not add banner button
                } else {
                    let play = isPlayButton(title: t, desc: d, width: sz.width, height: sz.height)
                    let steer = isSteerButton(title: t, desc: d)
                    let resume = isResumeButton(title: t, desc: d)
                    
                    if play || steer || resume {
                        if sz.width >= 16 && sz.height >= 16 {
                            candidates.append(CandidateButton(
                                element: el,
                                isPlay: play,
                                isSteer: steer,
                                isResume: resume,
                                y: pt.y
                            ))
                        }
                    }
                }
            }
        }
        
        var children: AnyObject?
        if AXUIElementCopyAttributeValue(el, kAXChildrenAttribute as CFString, &children) == .success,
           let childList = children as? [AXUIElement] {
            for c in childList {
                collectButtons(el: c, depth: depth + 1)
            }
        }
    }
    
    for win in winList {
        var posVal: AnyObject?
        AXUIElementCopyAttributeValue(win, kAXPositionAttribute as CFString, &posVal)
        var sizeVal: AnyObject?
        AXUIElementCopyAttributeValue(win, kAXSizeAttribute as CFString, &sizeVal)
        var pt = CGPoint.zero
        var sz = CGSize.zero
        if let pv = posVal { AXValueGetValue(pv as! AXValue, .cgPoint, &pt) }
        if let sv = sizeVal { AXValueGetValue(sv as! AXValue, .cgSize, &sz) }
        // Only inspect visible on-screen windows
        if pt.x >= -100 && pt.y >= 0 && sz.width > 300 && sz.height > 300 {
            collectButtons(el: win)
        }
    }
    
    if isAlreadyGenerating {
        print("ALREADY_ACTIVE")
        return (true, "ALREADY_ACTIVE")
    }
    
    if candidates.isEmpty {
        return (false, "NOT_FOUND")
    }
    
    // Sort descending by Y so bottom-most active controls take priority over scrollback history
    candidates.sort { $0.y > $1.y }
    
    guard let maxY = candidates.first?.y else { return (false, "NOT_FOUND") }
    // Focus on active interaction zone (bottom 250pt near lowest candidate)
    let activeZone = candidates.filter { $0.y >= maxY - 250 }
    
    // Priority order:
    // 1. Circular Play button at bottom-right of composer (white right-facing triangle) -> resumes queue directly
    if let playTarget = activeZone.first(where: { $0.isPlay }) {
        if pressButton(el: playTarget.element) {
            print("RESUMED_VIA_PLAY_BUTTON")
            return (true, "RESUMED_VIA_PLAY_BUTTON")
        }
    }
    
    // 2. Steer button on queued message row
    if let steerTarget = activeZone.first(where: { $0.isSteer }) {
        if pressButton(el: steerTarget.element) {
            print("RESUMED_VIA_STEER")
            return (true, "RESUMED_VIA_STEER")
        }
    }
    
    // 3. Native turn Resume/Retry button
    if let resumeTarget = activeZone.first(where: { $0.isResume }) {
        if pressButton(el: resumeTarget.element) {
            print("RESUMED_VIA_TURN_RESUME")
            return (true, "RESUMED_VIA_TURN_RESUME")
        }
    }
    
    return (false, "NOT_FOUND")
}

let result = resumeChatGPT()
exit(result.success ? 0 : 1)

