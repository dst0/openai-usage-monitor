import Cocoa
import Darwin

@main
struct RecoveryBannerMain {
  static func main() {
    guard CommandLine.arguments.count == 3,
      CommandLine.arguments[1] == "--payload",
      CommandLine.arguments[2].hasPrefix("/") else {
      fputs("PAYLOAD_ARGUMENT_REQUIRED\n", stderr)
      exit(2)
    }

    let banner = CodexRecoveryBanner(payloadURL: URL(fileURLWithPath: CommandLine.arguments[2]))
    guard banner.show() else {
      fputs("RECOVERY_BANNER_UNAVAILABLE\n", stderr)
      exit(1)
    }

    signal(SIGTERM) { _ in exit(0) }
    signal(SIGINT) { _ in exit(0) }
    Timer.scheduledTimer(withTimeInterval: 0.25, repeats: true) { _ in
      if !banner.isVisible { NSApplication.shared.terminate(nil) }
    }
    NSApplication.shared.run()
  }
}
