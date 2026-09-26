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

  // MARK: - Desktop Session Marker Watcher

  internal func startDesktopSessionFileWatcher() {
    stopDesktopSessionFileWatcher()
    let markerPath = CodexClient.desktopAppSessionURL.path
    guard FileManager.default.fileExists(atPath: markerPath) else {
      scheduleDesktopSessionWatcherRetry()
      return
    }
    let fd = open(markerPath, O_EVTONLY)
    guard fd >= 0 else {
      scheduleDesktopSessionWatcherRetry()
      return
    }
    desktopSessionWatcherFD = fd

    let source = DispatchSource.makeFileSystemObjectSource(
      fileDescriptor: fd, eventMask: [.write, .delete, .rename, .attrib],
      queue: DispatchQueue.main)
    source.setEventHandler { [weak self] in
      guard let self else { return }
      let flags = source.data
      self.desktopSessionUpdateWorkItem?.cancel()
      let update = DispatchWorkItem { [weak self] in
        self?.refreshDesktopSessionSnapshot()
      }
      self.desktopSessionUpdateWorkItem = update
      DispatchQueue.main.asyncAfter(deadline: .now() + 0.1, execute: update)

      if flags.contains(.delete) || flags.contains(.rename) {
        self.stopDesktopSessionFileWatcher()
        self.scheduleDesktopSessionWatcherRetry(after: 0.2)
      }
    }
    source.setCancelHandler { close(fd) }
    source.resume()
    desktopSessionWatcherSource = source
    refreshDesktopSessionSnapshot()
  }

  internal func stopDesktopSessionFileWatcher() {
    desktopSessionRestartWorkItem?.cancel()
    desktopSessionRestartWorkItem = nil
    desktopSessionWatcherSource?.cancel()
    desktopSessionWatcherSource = nil
    desktopSessionWatcherFD = -1
  }

  internal func refreshDesktopSessionSnapshot() {
    if let desktopSessionSnapshotRefreshOverride {
      desktopSessionSnapshotRefreshOverride()
      return
    }
    guard let snapshot = client.loadCachedSnapshot() else { return }
    lastSnapshot = snapshot
    updateUI(with: snapshot)
  }

  private func scheduleDesktopSessionWatcherRetry(after delay: TimeInterval = 1.0) {
    desktopSessionRestartWorkItem?.cancel()
    let retry = DispatchWorkItem { [weak self] in
      self?.startDesktopSessionFileWatcher()
    }
    desktopSessionRestartWorkItem = retry
    DispatchQueue.main.asyncAfter(deadline: .now() + delay, execute: retry)
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

      self.refreshDesktopSessionSnapshot()

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
