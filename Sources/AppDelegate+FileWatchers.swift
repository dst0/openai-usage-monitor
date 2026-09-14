import AppKit
import Foundation

extension AppDelegate {
  // MARK: - Status File Watcher (Instant 0ms UI Updates)

  internal func startStatusFileWatcher() {
    stopStatusFileWatcher()
    let statusPath = CodexClient.statusFileURL.path
    guard FileManager.default.fileExists(atPath: statusPath) else { return }

    let fd = open(statusPath, O_EVTONLY)
    guard fd >= 0 else { return }
    self.fileWatcherFD = fd

    let source = DispatchSource.makeFileSystemObjectSource(
      fileDescriptor: fd,
      eventMask: [.write, .delete, .rename, .extend, .attrib],
      queue: DispatchQueue.main
    )

    source.setEventHandler { [weak self] in
      guard let self = self else { return }
      let flags = source.data

      self.statusUpdateWorkItem?.cancel()
      let updateItem = DispatchWorkItem { [weak self] in
        if let snap = self?.client.loadCachedSnapshot() {
          self?.lastSnapshot = snap
          self?.updateUI(with: snap)
        }
      }
      self.statusUpdateWorkItem = updateItem
      DispatchQueue.main.asyncAfter(deadline: .now() + 0.1, execute: updateItem)

      if flags.contains(.delete) || flags.contains(.rename) {
        self.stopStatusFileWatcher()
        self.statusRestartWorkItem?.cancel()
        let restartItem = DispatchWorkItem { [weak self] in
          self?.startStatusFileWatcher()
        }
        self.statusRestartWorkItem = restartItem
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.2, execute: restartItem)
      }
    }

    source.setCancelHandler { close(fd) }
    source.resume()
    self.fileWatcherSource = source
  }

  internal func stopStatusFileWatcher() {
    statusRestartWorkItem?.cancel()
    statusRestartWorkItem = nil
    fileWatcherSource?.cancel()
    fileWatcherSource = nil
    fileWatcherFD = -1
  }

  // MARK: - Auth File Watcher (Instant Auto-Save of Logins in Codex App)

  internal func startAuthFileWatcher() {
    stopAuthFileWatcher()
    let authPath = CodexClient.codexHome.appendingPathComponent("auth.json").path
    guard FileManager.default.fileExists(atPath: authPath) else {
      authRestartWorkItem?.cancel()
      let retryItem = DispatchWorkItem { [weak self] in
        self?.startAuthFileWatcher()
      }
      authRestartWorkItem = retryItem
      DispatchQueue.main.asyncAfter(deadline: .now() + 3.0, execute: retryItem)
      return
    }

    let fd = open(authPath, O_EVTONLY)
    guard fd >= 0 else { return }
    self.authWatcherFD = fd

    let source = DispatchSource.makeFileSystemObjectSource(
      fileDescriptor: fd,
      eventMask: [.write, .delete, .rename, .extend, .attrib],
      queue: DispatchQueue.main
    )

    source.setEventHandler { [weak self] in
      guard let self = self else { return }
      let flags = source.data

      self.authRefreshWorkItem?.cancel()
      let refreshItem = DispatchWorkItem { [weak self] in
        self?.refreshNow()
      }
      self.authRefreshWorkItem = refreshItem
      DispatchQueue.main.asyncAfter(deadline: .now() + 0.4, execute: refreshItem)

      if flags.contains(.delete) || flags.contains(.rename) {
        self.stopAuthFileWatcher()
        self.authRestartWorkItem?.cancel()
        let restartItem = DispatchWorkItem { [weak self] in
          self?.startAuthFileWatcher()
        }
        self.authRestartWorkItem = restartItem
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.5, execute: restartItem)
      }
    }

    source.setCancelHandler { close(fd) }
    source.resume()
    self.authWatcherSource = source
  }

  internal func stopAuthFileWatcher() {
    authRestartWorkItem?.cancel()
    authRestartWorkItem = nil
    authWatcherSource?.cancel()
    authWatcherSource = nil
    authWatcherFD = -1
  }
}
