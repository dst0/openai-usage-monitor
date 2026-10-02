import Foundation

// The menu asks the bundled uninstaller for the install lock before it quits.
// These scripts stand in for uninstall.sh; nothing here reads or changes the live system.
func runUninstallLockCheckTests() {
  let echoStatus = Data(
    """
    [ "$1" = "--check-install-lock" ] || exit 64
    exit 75
    """.utf8)
  assertEqual(
    AppDelegate.bundledUninstallerExitStatus(script: echoStatus, arguments: ["--check-install-lock"]),
    AppDelegate.uninstallInstallInProgressStatus,
    "the check passes its argument and reports the uninstaller's exit status")

  let free = Data("exit 0\n".utf8)
  assertEqual(
    AppDelegate.bundledUninstallerExitStatus(script: free, arguments: ["--check-install-lock"]), 0,
    "a free lock reports 0")

  let hangs = Data("sleep 30\n".utf8)
  let started = Date()
  assertTrue(
    AppDelegate.bundledUninstallerExitStatus(script: hangs, arguments: [], timeout: 0.3) == nil,
    "a check that does not finish in time reports nil")
  assertTrue(Date().timeIntervalSince(started) < 5, "the check is bounded by its timeout")

  assertEqual(AppDelegate.uninstallInstallInProgressStatus, 75, "uninstall.sh refuses with EX_TEMPFAIL")
  let bundled = URL(fileURLWithPath: #filePath).deletingLastPathComponent()
    .deletingLastPathComponent().appendingPathComponent("scripts/uninstall.sh")
  let source = (try? String(contentsOf: bundled, encoding: .utf8)) ?? ""
  assertTrue(
    source.contains("--check-install-lock) CHECK_INSTALL_LOCK=1"),
    "uninstall.sh accepts --check-install-lock")
  print("  ✅ Uninstall install-lock check verified")
}
