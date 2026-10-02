import Darwin
import Foundation

/// Runs a short CLI command with a deadline, for menu actions that must not wait forever.
///
/// The command starts in its own process group with only `/dev/null` on its standard
/// descriptors and no other inherited descriptor. When it exits, or when the deadline passes,
/// its whole group is killed while the command itself is still unreaped, so the group ID cannot
/// name an unrelated process yet and nothing it started outlives it holding a descriptor. A
/// descendant that leaves the group with `setsid` escapes. Like the window-task helper's
/// deadline in the Rust core, the group is not tied to the Monitor's own lifetime.
///
/// One runner serves one serial queue. A killed command that the kernel has not let go of yet
/// is reaped in the background, and the runner refuses to start another command until it has
/// been, so a late write can never land after a newer one.
final class BoundedCommand: @unchecked Sendable {
  enum Outcome: Equatable {
    /// The command exited with this status.
    case exited(Int32)
    /// A signal ended the command before the deadline.
    case signaled(Int32)
    /// The deadline passed; the command's process group was killed.
    case timedOut
    /// The command could not be started (missing or not executable, for example), or an
    /// earlier killed command has still not exited.
    case failedToStart
  }

  /// How often a running command is checked for exit.
  private static let pollInterval: useconds_t = 20_000
  /// How long a killed command is given to exit before it is left to a background reaper.
  static let reapGrace: TimeInterval = 2
  /// Holds while a killed command waits for the background reaper.
  private let lingering = DispatchGroup()

  /// Runs `executable` with `arguments` and returns how it ended, at most `timeout` seconds
  /// plus the reap grace later. Call it from one serial queue.
  func run(_ executable: URL, _ arguments: [String], timeout: TimeInterval) -> Outcome {
    guard lingering.wait(timeout: .now() + Self.reapGrace) == .success,
      let pid = Self.spawn(executable, arguments)
    else { return .failedToStart }
    let deadline = Date().addingTimeInterval(timeout)
    var timedOut = false
    while true {
      // Reaped elsewhere: `pid` may already name another process, so it must not be signalled.
      guard let exited = Self.hasExited(pid) else { return .failedToStart }
      if exited { break }
      if Date() >= deadline {
        timedOut = true
        break
      }
      usleep(Self.pollInterval)
    }
    // Exited or not, `pid` is unreaped and still names its group.
    _ = killpg(pid, SIGKILL)
    let reapDeadline = Date().addingTimeInterval(Self.reapGrace)
    while true {
      if let status = Self.reap(pid) {
        return timedOut ? .timedOut : Self.outcome(of: status)
      }
      if Date() >= reapDeadline { break }
      usleep(Self.pollInterval)
    }
    // A command stuck in the kernel can outlive SIGKILL for a while.
    lingering.enter()
    DispatchQueue.global(qos: .utility).async { [lingering] in
      var status: Int32 = 0
      while waitpid(pid, &status, 0) == -1 && errno == EINTR {}
      lingering.leave()
    }
    return .timedOut
  }

  /// Whether `pid` has exited, without reaping it, or nil when it is no longer this
  /// process's child to wait for.
  private static func hasExited(_ pid: pid_t) -> Bool? {
    while true {
      // Zeroed first: with WNOHANG and nothing to report, waitid leaves it unchanged.
      var info = siginfo_t()
      if waitid(P_PID, id_t(pid), &info, WEXITED | WNOHANG | WNOWAIT) == 0 {
        return info.si_pid == pid
      }
      if errno != EINTR { return nil }
    }
  }

  /// Reaps `pid` and returns its wait status, or nil while it runs. A pid that is gone
  /// (reaped elsewhere) reports a status no exit has.
  private static func reap(_ pid: pid_t) -> Int32? {
    var status: Int32 = 0
    while true {
      let result = waitpid(pid, &status, WNOHANG)
      if result == pid { return status }
      if result == 0 { return nil }
      if errno != EINTR { return -1 }
    }
  }

  private static func outcome(of status: Int32) -> Outcome {
    guard status != -1 else { return .failedToStart }
    // Without WUNTRACED, waitpid reports only an exit or a terminating signal.
    let signal = status & 0x7f
    return signal == 0 ? .exited((status >> 8) & 0xff) : .signaled(signal)
  }

  private static func spawn(_ executable: URL, _ arguments: [String]) -> pid_t? {
    var attributes: posix_spawnattr_t? = nil
    guard posix_spawnattr_init(&attributes) == 0 else { return nil }
    defer { posix_spawnattr_destroy(&attributes) }
    var actions: posix_spawn_file_actions_t? = nil
    guard posix_spawn_file_actions_init(&actions) == 0 else { return nil }
    defer { posix_spawn_file_actions_destroy(&actions) }

    // Its own group, no inherited descriptor but the three below, default signal handling,
    // and nothing blocked, whatever the Monitor's threads have set.
    let flags =
      Int32(POSIX_SPAWN_SETPGROUP) | Int32(POSIX_SPAWN_CLOEXEC_DEFAULT)
      | Int32(POSIX_SPAWN_SETSIGDEF) | Int32(POSIX_SPAWN_SETSIGMASK)
    var noSignals = sigset_t()
    sigemptyset(&noSignals)
    var allSignals = sigset_t()
    sigfillset(&allSignals)
    guard posix_spawnattr_setflags(&attributes, Int16(flags)) == 0,
      posix_spawnattr_setpgroup(&attributes, 0) == 0,
      posix_spawnattr_setsigmask(&attributes, &noSignals) == 0,
      posix_spawnattr_setsigdefault(&attributes, &allSignals) == 0
    else { return nil }
    let standardDescriptors = [
      (STDIN_FILENO, O_RDONLY), (STDOUT_FILENO, O_WRONLY), (STDERR_FILENO, O_WRONLY),
    ]
    for (descriptor, mode) in standardDescriptors {
      guard posix_spawn_file_actions_addopen(&actions, descriptor, "/dev/null", mode, 0) == 0
      else { return nil }
    }

    let argv = ([executable.path] + arguments).map { strdup($0) } + [nil]
    defer { argv.forEach { free($0) } }
    let environment =
      ProcessInfo.processInfo.environment.map { strdup("\($0.key)=\($0.value)") } + [nil]
    defer { environment.forEach { free($0) } }

    var pid: pid_t = 0
    let result = posix_spawn(&pid, executable.path, &actions, &attributes, argv, environment)
    return result == 0 ? pid : nil
  }
}
