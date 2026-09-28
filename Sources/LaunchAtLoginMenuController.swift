import AppKit
import Foundation

/// Keeps the Launch at Login menu item equal to what macOS reports about the login item.
///
/// Reads and changes run in order on a private serial queue: reading System Events login items
/// starts `osascript`, which can wait for System Events to launch or for an Automation prompt, and
/// must not hold up the menu. The item shows a checkmark only when a read confirms the login item,
/// no mark when a read confirms there is none, and a dash while the state is not read yet or
/// cannot be read. A toggle asks for the opposite of a confirmed checkmark, then shows the state
/// read back afterwards and reports a failure when that differs from the request.
final class LaunchAtLoginMenuController {
  /// Why a toggle did not reach the state it asked for.
  enum Failure: Equatable {
    /// Enabling was requested and macOS reports no login item.
    case notEnabled
    /// Disabling was requested and macOS still reports a login item.
    case notDisabled
    /// The login item could not be read back, so the change is not confirmed.
    case unconfirmed

    var message: String {
      switch self {
      case .notEnabled: return L10n.launchAtLoginEnableFailed
      case .notDisabled: return L10n.launchAtLoginDisableFailed
      case .unconfirmed: return L10n.launchAtLoginUnknown
      }
    }

    /// The failure for a toggle that asked for `enable` and read back `state`, if it failed.
    static func of(requested enable: Bool, readBack state: LoginItemState) -> Failure? {
      switch (enable, state) {
      case (true, .enabled), (false, .disabled): return nil
      case (_, .unknown): return .unconfirmed
      case (true, .disabled): return .notEnabled
      case (false, .enabled): return .notDisabled
      }
    }
  }

  private let manager: AutoLaunchManager
  private let queue: DispatchQueue
  /// Called on the main thread when a toggle ends in another state than it asked for.
  var reportFailure: (Failure) -> Void
  /// The last state read back from macOS; nil until the first read finishes.
  private(set) var state: LoginItemState?
  /// True from a toggle until its read-back is shown; the item is disabled meanwhile.
  private(set) var isChanging = false
  /// True from a read until its result is shown (or dropped for a newer one).
  private(set) var isRefreshing = false
  private var refreshAgain = false
  /// Advanced by every read and change, so a read that started earlier cannot replace a newer
  /// result.
  private var generation = 0

  /// The menu item that shows the state. Set again whenever the menu is rebuilt.
  var item: NSMenuItem? {
    didSet { render() }
  }

  init(
    manager: AutoLaunchManager,
    queue: DispatchQueue = DispatchQueue(label: "com.codex.monitor.launch-at-login"),
    reportFailure: @escaping (Failure) -> Void
  ) {
    self.manager = manager
    self.queue = queue
    self.reportFailure = reportFailure
  }

  /// Reads the login item again and shows the result. A request during a read runs one more read
  /// after it; a request during a toggle is dropped, because the toggle ends with a read.
  func refresh() {
    guard !isChanging else { return }
    guard !isRefreshing else {
      refreshAgain = true
      return
    }
    isRefreshing = true
    generation += 1
    let started = generation
    let manager = self.manager
    queue.async { [weak self] in
      let state = manager.state
      DispatchQueue.main.async {
        guard let self else { return }
        self.isRefreshing = false
        if started == self.generation {
          self.state = state
          self.render()
        }
        if self.refreshAgain {
          self.refreshAgain = false
          self.refresh()
        }
      }
    }
  }

  /// Asks for the opposite of a confirmed checkmark: disables a confirmed login item and enables
  /// one in every other state. Ignored while a toggle is under way.
  func toggle() {
    guard !isChanging else { return }
    isChanging = true
    refreshAgain = false
    generation += 1
    let enable = state != .enabled
    let manager = self.manager
    render()
    queue.async { [weak self] in
      let state = manager.setEnabled(enable)
      DispatchQueue.main.async {
        guard let self else { return }
        self.isChanging = false
        self.state = state
        self.render()
        if let failure = Failure.of(requested: enable, readBack: state) {
          self.reportFailure(failure)
        }
      }
    }
  }

  private func render() {
    guard let item else { return }
    item.isEnabled = !isChanging
    switch state {
    case .enabled?: item.state = .on
    case .disabled?: item.state = .off
    case .unknown?, nil: item.state = .mixed
    }
    item.toolTip = state == .unknown ? L10n.launchAtLoginUnknown : nil
  }
}
