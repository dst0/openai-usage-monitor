import Cocoa
import Darwin
import Foundation

private func require(_ condition: @autoclosure () -> Bool, _ message: String) {
  if !condition() {
    fputs("FAIL: \(message)\n", stderr)
    exit(1)
  }
}

@main
struct CodexRecoveryPayloadReaderTests {
  static func main() {
    let root = FileManager.default.temporaryDirectory
      .appendingPathComponent("codex-recovery-payload-reader-\(UUID().uuidString)")
    try! FileManager.default.createDirectory(
      at: root, withIntermediateDirectories: true, attributes: [.posixPermissions: 0o700])
    defer { try? FileManager.default.removeItem(at: root) }

    let payload = root.appendingPathComponent("payload.json")
    try! Data("{\"version\":1}".utf8).write(to: payload)
    chmod(payload.path, 0o600)
    require(
      CodexRecoveryPayloadReader.readData(from: payload) != nil,
      "owner-only 0600 payload must be readable")

    chmod(payload.path, 0o644)
    require(
      CodexRecoveryPayloadReader.readData(from: payload) == nil,
      "group/other-readable payload must be rejected")

    chmod(payload.path, 0o400)
    require(
      CodexRecoveryPayloadReader.readData(from: payload) != nil,
      "stricter owner-only 0400 payload must be readable")

    let symlink = root.appendingPathComponent("payload-link.json")
    try! FileManager.default.createSymbolicLink(at: symlink, withDestinationURL: payload)
    require(
      CodexRecoveryPayloadReader.readData(from: symlink) == nil,
      "symlink payload must be rejected")

    let directory = root.appendingPathComponent("payload-directory")
    try! FileManager.default.createDirectory(
      at: directory, withIntermediateDirectories: false, attributes: [.posixPermissions: 0o700])
    require(
      CodexRecoveryPayloadReader.readData(from: directory) == nil,
      "non-regular payload must be rejected")

    let oversized = root.appendingPathComponent("oversized.json")
    try! Data(repeating: 0x61, count: CodexRecoveryPayloadReader.maxPayloadBytes + 1)
      .write(to: oversized)
    chmod(oversized.path, 0o600)
    require(
      CodexRecoveryPayloadReader.readData(from: oversized) == nil,
      "oversized payload must be rejected before decoding")

    print("  ✅ Recovery payload file validation verified")
  }
}
