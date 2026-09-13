import AppKit
import Foundation

public final class SingleInstanceGuard {
  private var lockFd: Int32 = -1
  public let lockPath: String

  public static var defaultLockPath: String {
    let home =
      (ProcessInfo.processInfo.environment["HOME"]).flatMap { $0.isEmpty ? nil : $0 }
      ?? FileManager.default.homeDirectoryForCurrentUser.path
    let codexDir = (home as NSString).appendingPathComponent(".codex")
    try? FileManager.default.createDirectory(atPath: codexDir, withIntermediateDirectories: true)
    return (codexDir as NSString).appendingPathComponent("monitor.lock")
  }

  public init(lockPath: String = SingleInstanceGuard.defaultLockPath) {
    self.lockPath = lockPath
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
