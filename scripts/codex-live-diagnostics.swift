import AppKit
import Foundation

/// Prints what the Monitor's live lookups find on this Mac: the installed Monitor CLI and the
/// running official Desktop. It only reads: it runs no CLI, reads no Codex home, and changes
/// nothing. Built and run only by `scripts/swift_live_diagnostics.sh --allow-live-system`,
/// never by a test.
@main
struct CodexLiveDiagnostics {
  static func main() {
    let cli = CodexClient.installedCLIExecutable()
    print("installed_cli=\(cli.path) executable=\(FileManager.default.isExecutableFile(atPath: cli.path))")
    if let desktop = CodexDesktopProcessIdentity.current() {
      print("desktop=running pid=\(desktop.pid) birth=\(desktop.birthID)")
    } else {
      print("desktop=not-running")
    }
  }
}
