import Darwin

internal enum CodexRecoveryProcessIdentity {
  static func birth(for pid: pid_t) -> String? {
    var info = proc_bsdinfo()
    let expectedSize = Int32(MemoryLayout<proc_bsdinfo>.size)
    let written = withUnsafeMutablePointer(to: &info) {
      proc_pidinfo(pid, PROC_PIDTBSDINFO, 0, $0, expectedSize)
    }
    guard written == expectedSize, info.pbi_status != UInt32(SZOMB) else { return nil }
    return "\(info.pbi_start_tvsec):\(String(format: "%06llu", info.pbi_start_tvusec))"
  }
}
