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
    
    var windows: AnyObject?
    guard AXUIElementCopyAttributeValue(axApp, kAXWindowsAttribute as CFString, &windows) == .success,
          let winList = windows as? [AXUIElement] else {
        return false
    }
    
    var resumed = false
    func searchAndPress(el: AXUIElement, depth: Int = 0) {
        if depth > 25 { return }
        var role: AnyObject?
        AXUIElementCopyAttributeValue(el, kAXRoleAttribute as CFString, &role)
        var desc: AnyObject?
        AXUIElementCopyAttributeValue(el, kAXDescriptionAttribute as CFString, &desc)
        var title: AnyObject?
        AXUIElementCopyAttributeValue(el, kAXTitleAttribute as CFString, &title)
        
        let r = (role as? String) ?? ""
        let d = (desc as? String) ?? ""
        let t = (title as? String) ?? ""
        
        // Matches:
        // 1. Submit button in composer when turn is paused: AXButton with desc == "Resume"
        // 2. Banner button in "Queue paused because you interrupted": AXButton with title == "Resume"
        if r == "AXButton" && (d == "Resume" || t == "Resume") {
            let err = AXUIElementPerformAction(el, kAXPressAction as CFString)
            if err == .success {
                resumed = true
            }
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
