import AppKit
import Darwin

/// The exact official Desktop process that owns an in-memory account session.
internal struct CodexDesktopProcessIdentity: Equatable {
  let pid: pid_t
  let birthID: String

  static func current() -> Self? {
    let executable = "/Applications/ChatGPT.app/Contents/MacOS/ChatGPT"
    let bundleIDs = ["com.openai.codex", "com.openai.chat"]
    let roots = bundleIDs.flatMap { NSRunningApplication.runningApplications(withBundleIdentifier: $0) }
      .filter { $0.executableURL?.path == executable }
    guard roots.count == 1 else { return nil }
    let pid = roots[0].processIdentifier
    guard pid > 1, let birthID = CodexRecoveryProcessIdentity.birth(for: pid) else { return nil }
    return Self(pid: pid, birthID: birthID)
  }
}
