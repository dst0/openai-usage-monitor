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

    // Every client here reads a Codex home of this run, never the live ~/.codex.
    let identityHome = TestCodexHome(purpose: "cli-identity")
    defer { identityHome.tearDown() }
    let testHome = identityHome.url
    let client = identityHome.client()
    let staleCliCache: [String: Any] = [
      "active_account_id": "missing-account",
      "five_hour_percentage": 0.0,
      "accounts": [[
        "id": cli.id, "email": cli.email, "plan_type": cli.planType,
        "five_hour_percentage": cli.fiveHourPercentage, "is_active": true,
      ]],
    ]
    let staleCliData = try! JSONSerialization.data(withJSONObject: staleCliCache)
    try! staleCliData.write(to: testHome.appendingPathComponent("usage-status.json"))
    let staleCliSnapshot = client.loadCachedSnapshot()
    require(staleCliSnapshot != nil, "the synthetic status cache must load")
    require(staleCliSnapshot?.cliAccount == nil, "unknown CLI identity must not borrow a cached account")

    var oldCliCache: [String: Any] = [
      "active_account_id": cli.id,
      "cli_auth_file_id": "stale-auth-file",
      "five_hour_percentage": cli.fiveHourPercentage,
      "accounts": [[
        "id": cli.id, "email": cli.email, "plan_type": cli.planType,
        "five_hour_percentage": cli.fiveHourPercentage, "is_active": true,
      ]],
    ]
    try! JSONSerialization.data(withJSONObject: oldCliCache)
      .write(to: testHome.appendingPathComponent("usage-status.json"))
    try! Data("{}".utf8).write(to: testHome.appendingPathComponent("auth.json"))
    try! FileManager.default.setAttributes(
      [.posixPermissions: 0o600], ofItemAtPath: testHome.appendingPathComponent("auth.json").path)
    require(client.loadCachedSnapshot()?.cliAccount == nil,
      "a cache bound to an older auth file must not display its CLI quota")
    var authInfo = stat()
    let authPath = testHome.appendingPathComponent("auth.json").path
    require(authPath.withCString({ lstat($0, &authInfo) == 0 }), "synthetic auth must exist")
    oldCliCache["cli_auth_file_id"] =
      "\(authInfo.st_dev):\(authInfo.st_ino):\(authInfo.st_mtimespec.tv_sec):\(authInfo.st_mtimespec.tv_nsec):\(authInfo.st_size)"
    try! JSONSerialization.data(withJSONObject: oldCliCache)
      .write(to: testHome.appendingPathComponent("usage-status.json"))
    require(client.loadCachedSnapshot()?.cliAccount?.id == cli.id,
      "a cache bound to the current auth file must display the verified CLI quota")
    try! Data("{ }".utf8).write(to: testHome.appendingPathComponent("auth.json"), options: .atomic)
    require(client.loadCachedSnapshot()?.cliAccount == nil,
      "replacing auth after caching must invalidate the CLI quota")

    // Whether ChatGPT runs comes from the client's Desktop source, never the live process list.
    // Both answers are checked, so a client that read the real list would fail one of them on
    // any machine, with ChatGPT open or not.
    let appCache: [String: Any] = [
      "accounts": [["id": app.id, "email": app.email, "plan_type": app.planType, "is_active": false]]
    ]
    try! JSONSerialization.data(withJSONObject: appCache)
      .write(to: testHome.appendingPathComponent("usage-status.json"))
    let runningDesktop = CodexDesktopProcessIdentity(pid: 4242, birthID: "1:000001")
    let runningClient = identityHome.client(
      desktopProcess: { runningDesktop }, desktopAppAccountIdProvider: { app.id })
    let closedClient = identityHome.client(
      desktopProcess: { nil }, desktopAppAccountIdProvider: { app.id })
    require(runningClient.isCodexAppRunning(), "a running Desktop must be reported running")
    require(runningClient.loadCachedSnapshot()?.isAppRunning == true,
      "the snapshot must see the client's running Desktop")
    require(runningClient.loadCachedSnapshot()?.appAccount?.id == app.id,
      "a running Desktop must show its session's App account")
    require(!closedClient.isCodexAppRunning(), "a closed Desktop must be reported closed")
    require(closedClient.loadCachedSnapshot()?.isAppRunning == false,
      "the snapshot must see the client's closed Desktop")
    require(closedClient.loadCachedSnapshot()?.appAccount == nil,
      "a closed Desktop must show no App account")

    let now = Date()
    let formatter = ISO8601DateFormatter()
    formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
    let process = CodexDesktopProcessIdentity(
      pid: 4242, birthID: "\(Int(now.timeIntervalSince1970) - 60):000001")
    func marker(
      _ accountID: String, at date: Date, process: CodexDesktopProcessIdentity? = nil,
      authFileID: String? = nil
    ) -> Data {
      var object: [String: Any] = [
        "account_id": accountID,
        "updated_at": formatter.string(from: date),
      ]
      if let process {
        object["process"] = ["pid": Int(process.pid), "birth_id": process.birthID]
      }
      if let authFileID { object["auth_file_id"] = authFileID }
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
        from: marker(app.id, at: now, process: process, authFileID: "old-auth"),
        currentProcess: process, now: now, currentAuthFileID: "new-auth") == nil,
      "a changed shared auth file must invalidate an externally rebound App marker")
    require(
      CodexClient.validatedDesktopAppSessionAccountId(
        from: marker(app.id, at: now, process: process, authFileID: "current-auth"),
        currentProcess: process, now: now, currentAuthFileID: "current-auth") == app.id,
      "an externally rebound App marker may use only its exact shared auth file")
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

    let privateMarkerURL = testHome.appendingPathComponent("private-session.json")
    let privateMarker = marker(app.id, at: now, process: process, authFileID: "current-auth")
    try! privateMarker.write(to: privateMarkerURL)
    try! FileManager.default.setAttributes(
      [.posixPermissions: 0o600], ofItemAtPath: privateMarkerURL.path)
    require(CodexClient.readPrivateSessionMarkerData(at: privateMarkerURL) == privateMarker,
      "a private regular marker must be readable")
    let linkedMarkerURL = testHome.appendingPathComponent("linked-session.json")
    try! FileManager.default.createSymbolicLink(
      at: linkedMarkerURL, withDestinationURL: privateMarkerURL)
    require(CodexClient.readPrivateSessionMarkerData(at: linkedMarkerURL) == nil,
      "a symlinked marker must not be read")
    try! FileManager.default.setAttributes(
      [.posixPermissions: 0o644], ofItemAtPath: privateMarkerURL.path)
    require(CodexClient.readPrivateSessionMarkerData(at: privateMarkerURL) == nil,
      "a world-readable marker must not be read")

    print("  ✅ App/CLI identity separation and stale-marker rejection verified")
  }
}
