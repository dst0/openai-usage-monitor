import Cocoa
import ApplicationServices

/// Searches the Accessibility hierarchy of ChatGPT / Codex and performs
/// the `Resume` action on any paused queue or interrupted turn button.
func resumeChatGPT() -> Bool {
    guard let app = NSRunningApplication.runningApplications(withBundleIdentifier: "com.openai.codex").first else {
        return false
    }
    
    let axApp = AXUIElementCreateApplication(app.processIdentifier)
    AXUIElementSetAttributeValue(axApp, "AXManualAccessibility" as CFString, true as CFTypeRef)
    AXUIElementSetAttributeValue(axApp, "AXEnhancedUserInterface" as CFString, true as CFTypeRef)
    
    var windows: AnyObject?
    guard AXUIElementCopyAttributeValue(axApp, kAXWindowsAttribute as CFString, &windows) == .success,
          let winList = windows as? [AXUIElement] else {
        return false
    }
    
    var resumed = false
    func searchAndPress(el: AXUIElement, depth: Int = 0) {
        if depth > 65 { return }
        var role: AnyObject?
        AXUIElementCopyAttributeValue(el, kAXRoleAttribute as CFString, &role)
        var desc: AnyObject?
        AXUIElementCopyAttributeValue(el, kAXDescriptionAttribute as CFString, &desc)
        var title: AnyObject?
        AXUIElementCopyAttributeValue(el, kAXTitleAttribute as CFString, &title)
        
        let r = (role as? String) ?? ""
        let d = (desc as? String) ?? ""
        let t = (title as? String) ?? ""
        
        let isButton = r == "AXButton" || r.contains("Button")
        let isResume = d.localizedCaseInsensitiveContains("resume") ||
                       t.localizedCaseInsensitiveContains("resume") ||
                       d.localizedCaseInsensitiveContains("retry") ||
                       t.localizedCaseInsensitiveContains("retry") ||
                       d.localizedCaseInsensitiveContains("возобновить") ||
                       t.localizedCaseInsensitiveContains("возобновить") ||
                       d.localizedCaseInsensitiveContains("повторить") ||
                       t.localizedCaseInsensitiveContains("повторить")
        
        if isButton && isResume {
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
                }
            }
            resumed = true
        }
        
        var children: AnyObject?
        if AXUIElementCopyAttributeValue(el, kAXChildrenAttribute as CFString, &children) == .success,
           let childList = children as? [AXUIElement] {
            for c in childList {
                searchAndPress(el: c, depth: depth + 1)
            }
        }
    }
    
    for win in winList {
        searchAndPress(el: win)
    }
    return resumed
}

exit(resumeChatGPT() ? 0 : 1)

