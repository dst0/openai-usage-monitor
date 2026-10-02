import Darwin
import Foundation

/// Test 5c: `BoundedCommand` reports how a command ended, stops one that outlives its deadline,
/// and leaves nothing it started holding a descriptor open, on either path.
///
/// A descendant's hold is observed through a FIFO in a `TestCodexHome`: the command opens it
/// for writing, writes one byte, and leaves a `sleep` holding it. The FIFO reads EOF only once
/// every writer is gone, so EOF proves the descendant is dead without reading the process list.
func runBoundedCommandTests() {
  let home = TestCodexHome(purpose: "bounded-command")
  checkExitStatusAndSignal(home)
  checkMissingExecutableFailsToStart(home)
  checkDescendantDiesAfterExit(home)
  checkHungCommandStopsAtItsDeadline(home)
  home.tearDown()
  print("  ✅ Bounded CLI runs stop at their deadline and leave no descendants")
}

private func checkExitStatusAndSignal(_ home: TestCodexHome) {
  let runner = BoundedCommand()
  let exits = script(home, "exits", "exit 7")
  assertEqual(runner.run(exits, [], timeout: 10), .exited(7), "The exit status must be reported")
  let echoes = script(home, "echoes", "[ \"$1 $2\" = 'config --flag' ] || exit 1; exit 0")
  assertEqual(
    runner.run(echoes, ["config", "--flag"], timeout: 10), .exited(0),
    "The arguments must reach the command")
  let killsItself = script(home, "signaled", "kill -TERM $$; sleep 5")
  assertEqual(
    runner.run(killsItself, [], timeout: 10), .signaled(SIGTERM), "A terminating signal must be reported")
}

private func checkMissingExecutableFailsToStart(_ home: TestCodexHome) {
  assertEqual(
    BoundedCommand().run(home.file("not-installed"), [], timeout: 10), .failedToStart,
    "A missing command must fail to start")
}

/// The command exits at once and leaves a `sleep` holding the FIFO: the runner kills the
/// group the command left behind.
private func checkDescendantDiesAfterExit(_ home: TestCodexHome) {
  let fifo = makeFIFO(home, "exit-holder")
  let reader = openForReading(fifo)
  let leaves = script(home, "leaves-holder", "exec 3>\"$1\"; printf x >&3; sleep 300 >&3 & exit 0")
  assertEqual(
    BoundedCommand().run(leaves, [fifo.path], timeout: 10), .exited(0),
    "A command that exits must report its status")
  assertEqual(drain(reader), "x", "The descendant must have held the FIFO and then let it go")
  close(reader)
}

/// The command never exits and neither does its descendant: both go at the deadline.
private func checkHungCommandStopsAtItsDeadline(_ home: TestCodexHome) {
  let fifo = makeFIFO(home, "hung-holder")
  let reader = openForReading(fifo)
  let hangs = script(home, "hangs", "exec 3>\"$1\"; printf x >&3; sleep 300 >&3 & exec sleep 300")
  let started = Date()
  assertEqual(
    BoundedCommand().run(hangs, [fifo.path], timeout: 2), .timedOut,
    "A command that outlives its deadline must time out")
  let waited = Date().timeIntervalSince(started)
  assertTrue(
    waited >= 2 && waited < 2 + BoundedCommand.reapGrace + 1,
    "A hung command must stop at its deadline, not \(waited) s later")
  assertEqual(drain(reader), "x", "The hung command and its descendant must let the FIFO go")
  close(reader)
}

private func script(_ home: TestCodexHome, _ name: String, _ body: String) -> URL {
  let url = home.file(name)
  try! Data("#!/bin/sh\n\(body)\n".utf8).write(to: url)
  assertEqual(chmod(url.path, 0o700), 0, "\(name) must be executable")
  return url
}

private func makeFIFO(_ home: TestCodexHome, _ name: String) -> URL {
  let url = home.file(name)
  assertEqual(mkfifo(url.path, 0o600), 0, "Cannot make the FIFO \(name)")
  return url
}

/// A reader opened first, so the command's open for writing does not block.
private func openForReading(_ fifo: URL) -> Int32 {
  let fd = open(fifo.path, O_RDONLY | O_NONBLOCK | O_CLOEXEC)
  assertTrue(fd >= 0, "Cannot open \(fifo.lastPathComponent) for reading")
  return fd
}

/// Everything written to `fd` until every writer has closed it, or what arrived when ten
/// seconds passed without that.
private func drain(_ fd: Int32) -> String {
  var received = Data()
  let deadline = Date().addingTimeInterval(10)
  var buffer = [UInt8](repeating: 0, count: 64)
  while Date() < deadline {
    let count = read(fd, &buffer, buffer.count)
    if count > 0 {
      received.append(contentsOf: buffer[0..<count])
    } else if count == 0 {
      return String(decoding: received, as: UTF8.self)
    } else if errno != EAGAIN && errno != EINTR {
      break
    } else {
      usleep(20_000)
    }
  }
  assertTrue(false, "A descendant still holds the FIFO")
  return String(decoding: received, as: UTF8.self)
}
