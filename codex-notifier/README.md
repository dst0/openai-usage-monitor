# Codex Notifier

A lightweight, native macOS notification helper application for **OpenAI Codex Usage Monitor & Switcher**.

---

## 🎯 Purpose

When command-line scripts or background daemons send desktop notifications using `osascript -e 'display notification ...'`, macOS attributes the notification to the default AppleScript scripting component: **Script Editor** (`Script Editor.app`). This results in:
* Generic scroll-and-quill Script Editor icon.
* All background alerts bundled under the generic "Script Editor" group in macOS Notification Center.
* Inability to distinguish Codex switch notifications from other system or ad-hoc scripts.

**Codex Notifier** resolves this by providing a dedicated, native macOS application bundle (`Codex Notifier.app`) with its own registered `CFBundleIdentifier` (`com.codex.switcher.notifier`), background daemon flag (`LSUIElement`), and custom vector icon.

---

## 🏗️ Architecture

1. **Lightweight Native Cocoa Runner (`src/notify.m`)**:
   * Uses native Apple Cocoa `NSUserNotificationCenter` via Objective-C.
   * Implements `NSUserNotificationCenterDelegate` with `shouldPresentNotification: -> YES` to ensure alert banners display immediately even when invoked headlessly.
   * Zero third-party runtime dependencies; compiles directly in <0.2s using macOS built-in `clang`.
   * Accepts command-line arguments: `notify <title> [message] [subtitle]`.

2. **Headless App Bundle (`Info.plist`)**:
   * Packaged as `Codex Notifier.app`.
   * `LSUIElement = true` ensures the application runs silently in the background without creating a Dock icon, menu bar entry, or bouncing Dock animations.

3. **Pre-Rendered Vector Assets (`resources/`)**:
   * **Zero proprietary assets**: Vector source (`AppIcon.svg`) and compiled icon (`AppIcon.icns`) are included directly in the repository.
   * Features a dark squircle with subtle depth lighting and a stylized 6-petal geometric spiral knot in vibrant emerald/teal tones, capturing the distinct Codex aesthetic.

---

## 📁 Directory Structure

```
codex-notifier/
├── README.md                 # Component documentation
├── build.sh                  # Fast build script (compile, codesign, install, register)
├── Info.plist                # App bundle metadata and bundle identifier
├── src/
│   └── notify.m              # Native Objective-C notification runner
└── resources/
    ├── AppIcon.svg           # Human-editable vector source icon
    └── AppIcon.icns          # Pre-compiled multi-resolution macOS icon
```

---

## 🚀 Building and Installation

Run `build.sh` from this directory:

```bash
./build.sh
```

This will:
1. Compile `src/notify.m` to `build/Codex Notifier.app/Contents/MacOS/notify`.
2. Assemble the `.app` bundle with `Info.plist` and `resources/AppIcon.icns`.
3. Sign the bundle with an ad-hoc local signature (`codesign -fs -`).
4. Copy the app bundle to `~/Applications/Codex Notifier.app`.
5. Register the bundle with macOS LaunchServices (`lsregister`).

---

## 💻 CLI Usage

You can test or invoke the notifier directly from the terminal or shell scripts:

```bash
"$HOME/Applications/Codex Notifier.app/Contents/MacOS/notify" \
    "Codex Switcher" \
    "New Account Added: user@example.com" \
    "Codex app"
```

Arguments:
1. `title` (Required): Main alert title (e.g., `"Codex Switcher"`).
2. `message` (Optional): Body text of the notification.
3. `subtitle` (Optional): Bold subtitle displayed below the title.

---

## 🔗 Integration with Codex Switcher Daemon

In `codex-switcher` (`src/switcher.rs`), the notification dispatcher `send_macos_notification`:
1. Checks for the presence of `~/Applications/Codex Notifier.app/Contents/MacOS/notify`.
2. If present, executes it with the account switch details.
3. If not found, gracefully falls back to `osascript`.
