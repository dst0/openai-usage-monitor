import AppKit

extension AppDelegate {
  internal static func isOfficialDesktopExecutable(_ path: String?) -> Bool {
    path == "/Applications/ChatGPT.app/Contents/MacOS/ChatGPT"
  }

  internal func startDesktopLifecycleObservers() {
    stopDesktopLifecycleObservers()
    let notifications: [Notification.Name] = [
      NSWorkspace.didLaunchApplicationNotification,
      NSWorkspace.didTerminateApplicationNotification,
    ]
    desktopLifecycleObservers = notifications.map { name in
      NSWorkspace.shared.notificationCenter.addObserver(
        forName: name, object: nil, queue: .main
      ) { [weak self] note in
        guard let app = note.userInfo?[NSWorkspace.applicationUserInfoKey] as? NSRunningApplication,
          Self.isOfficialDesktopExecutable(app.executableURL?.path)
        else { return }
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.1) { [weak self] in
          self?.refreshDesktopSessionSnapshot()
        }
      }
    }
  }

  internal func stopDesktopLifecycleObservers() {
    for observer in desktopLifecycleObservers {
      NSWorkspace.shared.notificationCenter.removeObserver(observer)
    }
    desktopLifecycleObservers.removeAll()
  }
}
