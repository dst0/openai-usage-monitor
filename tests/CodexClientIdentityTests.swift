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
    func marker(_ accountID: String, at date: Date) -> Data {
      let object: [String: String] = [
        "account_id": accountID,
        "updated_at": formatter.string(from: date),
      ]
      return try! JSONSerialization.data(withJSONObject: object)
    }

    require(
      CodexClient.validatedDesktopAppSessionAccountId(from: marker(app.id, at: now), now: now)
        == app.id,
      "current App marker must be accepted")
    require(
      CodexClient.validatedDesktopAppSessionAccountId(
        from: marker(app.id, at: now.addingTimeInterval(-CodexClient.desktopAppSessionMaxAge - 1)),
        now: now) == nil,
      "stale App marker must be rejected")
    require(
      CodexClient.validatedDesktopAppSessionAccountId(
        from: marker(app.id, at: now.addingTimeInterval(2)), now: now) == nil,
      "future App marker must be rejected")
    require(
      CodexClient.validatedDesktopAppSessionAccountId(
        from: Data("{\"account_id\":\"\(app.id)\"}".utf8), now: now) == nil,
      "marker without freshness metadata must be rejected")

    print("  ✅ App/CLI identity separation and stale-marker rejection verified")
  }
}
