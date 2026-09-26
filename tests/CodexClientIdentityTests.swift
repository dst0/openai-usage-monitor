import Foundation
import Darwin

private func require(_ condition: @autoclosure () -> Bool, _ message: String) {
  if !condition() {
    fputs("FAIL: \(message)\n", stderr)
    exit(1)
  }
}

@main
struct CodexClientIdentityTests {
  static func main() {
    let cli = AccountQuota(
      id: "cli-account", email: "cli@example.com", planType: "pro", isCurrentActive: true,
      fiveHourPercentage: 90, weeklyPercentage: 80, resetTime: nil, resetAfterSeconds: 600,
      credits: 0)
    let app = AccountQuota(
      id: "app-account", email: "app@example.com", planType: "business", isCurrentActive: false,
      fiveHourPercentage: 30, weeklyPercentage: 20, resetTime: nil, resetAfterSeconds: 900,
      credits: 0)
    let accounts = [cli, app]

    require(
      CodexClient.resolveAppAccount(isAppRunning: true, markerID: app.id, accounts: accounts)?.id
        == app.id,
      "verified App marker must resolve the App account")
    require(
      CodexClient.resolveAppAccount(isAppRunning: true, markerID: nil, accounts: accounts) == nil,
      "missing App marker must leave App identity unknown")
    require(
      CodexClient.resolveAppAccount(
        isAppRunning: true, markerID: "old-account", accounts: accounts) == nil,
      "unmatched App marker must fail closed")

    let snapshot = MultiAccountSnapshot(
      timestamp: Date(), activeAccountId: cli.id, activeEmail: cli.email, activePlan: cli.planType,
      fiveHourPercentage: cli.fiveHourPercentage, weeklyPercentage: cli.weeklyPercentage,
      resetTime: nil, resetAfterSeconds: 600, credits: 0, isAppRunning: true,
      accounts: accounts, appAccount: nil, cliAccount: cli)
    require(snapshot.cliAccount?.id == cli.id, "CLI identity must remain available")
    require(snapshot.appAccount == nil, "missing App identity must not fall back to CLI")

    let now = Date()
    let formatter = ISO8601DateFormatter()
    formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
    let process = CodexDesktopProcessIdentity(
      pid: 4242, birthID: "\(Int(now.timeIntervalSince1970) - 60):000001")
    func marker(_ accountID: String, at date: Date, process: CodexDesktopProcessIdentity? = nil) -> Data {
      var object: [String: Any] = [
        "account_id": accountID,
        "updated_at": formatter.string(from: date),
      ]
      if let process {
        object["process"] = ["pid": Int(process.pid), "birth_id": process.birthID]
      }
      return try! JSONSerialization.data(withJSONObject: object)
    }

    require(
      CodexClient.validatedDesktopAppSessionAccountId(
        from: marker(app.id, at: now), currentProcess: process, now: now)
        == nil,
      "a recent but unbound marker must not claim the current Desktop process")

    require(
      CodexClient.validatedDesktopAppSessionAccountId(
        from: marker(app.id, at: now, process: process), currentProcess: process, now: now)
        == app.id,
      "current App marker bound to the exact process must be accepted")
    require(
      CodexClient.validatedDesktopAppSessionAccountId(
        from: marker(app.id, at: now, process: process),
        currentProcess: CodexDesktopProcessIdentity(pid: 4243, birthID: process.birthID), now: now)
        == nil,
      "a marker for another Desktop PID must be rejected")
    require(
      CodexClient.validatedDesktopAppSessionAccountId(
        from: marker(app.id, at: now, process: process),
        currentProcess: CodexDesktopProcessIdentity(pid: process.pid, birthID: "reused-pid"), now: now)
        == nil,
      "a marker for a reused Desktop PID must be rejected")
    require(
      CodexClient.validatedDesktopAppSessionAccountId(
        from: marker(app.id, at: now.addingTimeInterval(-61), process: process),
        currentProcess: process,
        now: now) == nil,
      "marker predating the current Desktop process must be rejected")
    let longLivedProcess = CodexDesktopProcessIdentity(
      pid: 4242, birthID: "\(Int(now.timeIntervalSince1970) - 172_800):000001")
    require(
      CodexClient.validatedDesktopAppSessionAccountId(
        from: marker(app.id, at: now.addingTimeInterval(-86_400), process: longLivedProcess),
        currentProcess: longLivedProcess, now: now) == app.id,
      "a long-lived Desktop process must retain its valid App binding")
    require(
      CodexClient.validatedDesktopAppSessionAccountId(
        from: marker(app.id, at: now.addingTimeInterval(2), process: process), currentProcess: process, now: now) == nil,
      "future App marker must be rejected")
    require(
      CodexClient.validatedDesktopAppSessionAccountId(
        from: Data("{\"account_id\":\"\(app.id)\"}".utf8), currentProcess: process, now: now) == nil,
      "marker without freshness metadata must be rejected")

    print("  ✅ App/CLI identity separation and stale-marker rejection verified")
  }
}
