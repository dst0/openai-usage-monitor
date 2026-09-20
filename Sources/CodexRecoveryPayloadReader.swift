import Foundation
import Darwin

/// Reads the private Rust-to-AppKit recovery hand-off without following a
/// path that changed after its security checks.
internal enum CodexRecoveryPayloadReader {
  internal static let maxPayloadBytes = 256 * 1024

  internal static func readData(from url: URL) -> Data? {
    var pathInfo = stat()
    guard url.path.withCString({ lstat($0, &pathInfo) == 0 }),
      isPrivateRegularFile(pathInfo),
      pathInfo.st_size >= 0,
      pathInfo.st_size <= off_t(maxPayloadBytes)
    else { return nil }

    let flags = O_RDONLY | O_CLOEXEC | O_NOFOLLOW
    let fd = url.path.withCString { open($0, flags) }
    guard fd >= 0 else { return nil }
    defer { close(fd) }

    var openedInfo = stat()
    guard fstat(fd, &openedInfo) == 0,
      isPrivateRegularFile(openedInfo),
      openedInfo.st_dev == pathInfo.st_dev,
      openedInfo.st_ino == pathInfo.st_ino,
      openedInfo.st_size >= 0,
      openedInfo.st_size <= off_t(maxPayloadBytes)
    else { return nil }

    let length = Int(openedInfo.st_size)
    let handle = FileHandle(fileDescriptor: fd, closeOnDealloc: false)
    let data = handle.readData(ofLength: length)

    var finalInfo = stat()
    guard data.count == length,
      fstat(fd, &finalInfo) == 0,
      isPrivateRegularFile(finalInfo),
      finalInfo.st_dev == openedInfo.st_dev,
      finalInfo.st_ino == openedInfo.st_ino,
      finalInfo.st_size == openedInfo.st_size
    else { return nil }
    return data
  }

  private static func isPrivateRegularFile(_ info: stat) -> Bool {
    let mode = info.st_mode
    let permissions = mode & 0o777
    return (mode & S_IFMT) == S_IFREG
      && info.st_uid == geteuid()
      && permissions & 0o077 == 0
      && permissions & 0o400 != 0
  }
}
