import AppKit
import Foundation

/// The Monitor's single-instance lock, `monitor.lock` in the Codex home.
///
/// Building a guard only names the lock. `tryAcquire()`, which the app calls once at launch,
/// creates the Codex home if it is missing and then the lock file, so constructing an
/// `AppDelegate` touches nothing on disk.
public final class SingleInstanceGuard {
  public static let lockFileName = "monitor.lock"

  private var lockFd: Int32 = -1
  public let lockPath: String

  public init(codexHome: URL) {
    self.lockPath = codexHome.appendingPathComponent(Self.lockFileName, isDirectory: false).path
  }

  public func tryAcquire() -> Bool {
    let parentDir = (lockPath as NSString).deletingLastPathComponent
    try? FileManager.default.createDirectory(atPath: parentDir, withIntermediateDirectories: true)

    let fd = open(lockPath, O_CREAT | O_RDWR, 0o600)
    guard fd >= 0 else { return false }

    if flock(fd, LOCK_EX | LOCK_NB) != 0 {
      close(fd)
      return false
    }

    self.lockFd = fd
    ftruncate(fd, 0)
    let pidStr = "\(getpid())\n"
    pidStr.withCString { ptr in
      _ = write(fd, ptr, strlen(ptr))
    }

    return true
  }

  public func release() {
    if lockFd >= 0 {
      flock(lockFd, LOCK_UN)
      close(lockFd)
      lockFd = -1
    }
  }

  public static func isAnotherInstanceRunning(bundleIdentifier: String? = nil) -> Bool {
    let bundleID = bundleIdentifier ?? Bundle.main.bundleIdentifier ?? "com.codex.monitor"
    let currentPID = getpid()
    let apps = NSRunningApplication.runningApplications(withBundleIdentifier: bundleID)
    return apps.contains { $0.processIdentifier != currentPID }
  }

  deinit {
    release()
  }
}
