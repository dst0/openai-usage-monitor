import ApplicationServices
import Cocoa
import Darwin

/// Native, non-activating recovery panel driven by the private Rust payload.
///
/// The Rust coordinator owns the operation payload. This class owns the one
/// display lease and never receives session metadata in process arguments.
public final class CodexRecoveryBanner {
  public struct Payload: Decodable, Equatable {
    public let version: Int
    public let operation_id: String
    public let title: String
    public let explanation: String
    public let expected_process: ProcessIdentity
    public let saved_window: SavedWindow
    public let sessions: [Session]
    public let minimum_visible_ms: UInt64
    public let updated_at_unix_ms: UInt64
  }

  public struct ProcessIdentity: Decodable, Equatable {
    public let pid: Int32
    public let birth_identity: String
  }

  public struct Rect: Decodable, Equatable {
    public let x: CGFloat
    public let y: CGFloat
    public let width: CGFloat
    public let height: CGFloat

    fileprivate var cgRect: CGRect { CGRect(x: x, y: y, width: width, height: height) }
  }

  public struct SavedWindow: Decodable, Equatable {
    public let frame: Rect
    public let screen: Rect
  }

  public struct Session: Decodable, Equatable {
    public let project: String
    public let title: String
    public let short_id: String
    public let status: String
  }

  private let payloadURL: URL
  private var panel: NSPanel?
  private var ownerFD: Int32 = -1
  private var refreshTimer: Timer?
  private var visibleSince: Date?
  private var renderedPayload: Payload?
  private var rowLabels: [(indicator: NSTextField, label: NSTextField)] = []
  private var scrollView: NSScrollView?
  private var cardView: NSView?
  private var titleLabel: NSTextField?
  private var explanationLabel: NSTextField?

  public init(payloadURL: URL) {
    self.payloadURL = payloadURL
  }

  deinit { dismiss() }

  /// Creates exactly one panel and enters no run loop; the app host owns it.
  @discardableResult
  public func show() -> Bool {
    guard panel == nil, let payload = readPayload(), acquireDisplayOwnership() else { return false }
    guard CodexRecoveryProcessIdentity.birth(for: payload.expected_process.pid)
        == payload.expected_process.birth_identity else {
      releaseDisplayOwnership()
      return false
    }

    NSApplication.shared.setActivationPolicy(.accessory)
    renderedPayload = payload
    visibleSince = Date()
    let initialHeight = panelHeight(for: payload.sessions.count)
    let p = NSPanel(
      contentRect: Self.panelFrame(for: payload.saved_window.frame.cgRect, width: 540, height: initialHeight),
      styleMask: [.borderless, .nonactivatingPanel],
      backing: .buffered,
      defer: false
    )
    p.level = .statusBar
    p.isOpaque = false
    p.backgroundColor = .clear
    p.hasShadow = true
    p.ignoresMouseEvents = true
    p.hidesOnDeactivate = false
    // Keep the single panel on the Space containing the captured Codex
    // window. Joining every Space makes the recovery notice look duplicated
    // and can leave it detached from the window it describes.
    p.collectionBehavior = [.fullScreenAuxiliary, .stationary]

    let card = NSView(frame: NSRect(origin: .zero, size: p.contentRect(forFrameRect: p.frame).size))
    card.autoresizingMask = [.width, .height]
    card.wantsLayer = true
    card.layer?.cornerRadius = 14
    card.layer?.masksToBounds = true
    card.layer?.backgroundColor = NSColor(red: 0.08, green: 0.09, blue: 0.12, alpha: 0.98).cgColor
    card.layer?.borderWidth = 1.5
    card.layer?.borderColor = NSColor(red: 0.22, green: 0.25, blue: 0.32, alpha: 0.90).cgColor
    p.contentView = card
    panel = p
    cardView = card

    let title = NSTextField(labelWithString: payload.title)
    title.font = .systemFont(ofSize: 13, weight: .bold)
    title.textColor = NSColor(red: 1.0, green: 0.82, blue: 0.28, alpha: 1.0)
    title.frame = NSRect(x: 18, y: initialHeight - 30, width: 504, height: 18)
    title.lineBreakMode = .byTruncatingTail
    card.addSubview(title)
    titleLabel = title

    let explanation = NSTextField(labelWithString: payload.explanation)
    explanation.font = .systemFont(ofSize: 11, weight: .medium)
    explanation.textColor = NSColor(white: 0.93, alpha: 1.0)
    explanation.frame = NSRect(x: 18, y: initialHeight - 51, width: 504, height: 18)
    explanation.lineBreakMode = .byTruncatingTail
    card.addSubview(explanation)
    explanationLabel = explanation

    let scroll = NSScrollView(frame: NSRect(x: 18, y: 8, width: 504, height: max(0, initialHeight - 62)))
    scroll.drawsBackground = false
    scroll.hasVerticalScroller = payload.sessions.count > Self.maxVisibleRows
    scroll.autohidesScrollers = true
    scroll.borderType = .noBorder
    card.addSubview(scroll)
    scrollView = scroll

    renderRows(payload.sessions)
    p.orderFrontRegardless()
    writeReadyMarker()
    refreshTimer = Timer.scheduledTimer(withTimeInterval: 0.20, repeats: true) { [weak self] _ in
      self?.refresh()
    }
    RunLoop.current.add(refreshTimer!, forMode: .common)
    return true
  }

  public func dismiss() {
    refreshTimer?.invalidate()
    refreshTimer = nil
    panel?.orderOut(nil)
    panel = nil
    cardView = nil
    scrollView = nil
    rowLabels.removeAll()
    releaseDisplayOwnership()
  }

  public static let maxVisibleRows = 9

  public var isVisible: Bool { panel != nil }

  /// AppKit-global coordinates are retained, including negative secondary
  /// display origins. No primary-screen height is involved.
  public static func panelFrame(for windowFrame: CGRect, width: CGFloat, height: CGFloat) -> NSRect {
    NSRect(
      x: windowFrame.midX - width / 2,
      y: windowFrame.maxY - height - 12,
      width: width,
      height: height
    )
  }

  private func refresh() {
    guard let payload = readPayload() else {
      if let since = visibleSince, Date().timeIntervalSince(since) >= 5 { dismiss() }
      return
    }
    let sessionsChanged = renderedPayload?.sessions != payload.sessions
    renderedPayload = payload
    titleLabel?.stringValue = payload.title
    explanationLabel?.stringValue = payload.explanation
    if sessionsChanged { renderRows(payload.sessions) }
    guard let p = panel else { return }
    let targetFrame = currentCodexFrame(for: payload.expected_process) ?? payload.saved_window.frame.cgRect
    let height = panelHeight(for: payload.sessions.count)
    p.setFrame(Self.panelFrame(for: targetFrame, width: 540, height: height), display: true, animate: false)
    cardView?.frame = NSRect(origin: .zero, size: p.contentRect(forFrameRect: p.frame).size)
    titleLabel?.frame = NSRect(x: 18, y: height - 30, width: 504, height: 18)
    explanationLabel?.frame = NSRect(x: 18, y: height - 51, width: 504, height: 18)
    scrollView?.frame = NSRect(x: 18, y: 8, width: 504, height: max(0, height - 62))
  }

  private func renderRows(_ sessions: [Session]) {
    guard let scroll = scrollView else { return }
    let documentHeight = max(CGFloat(sessions.count) * 24, scroll.contentView.bounds.height)
    let document = NSView(frame: NSRect(x: 0, y: 0, width: 480, height: documentHeight))
    document.setAccessibilityRole(.list)
    rowLabels.removeAll()
    for (index, session) in sessions.enumerated() {
      let y = documentHeight - CGFloat(index + 1) * 24
      let indicator = NSTextField(labelWithString: CodexRecoveryBannerStatus.indicator(session.status))
      indicator.alignment = .center
      indicator.frame = NSRect(x: 0, y: y, width: 22, height: 22)
      indicator.textColor = CodexRecoveryBannerStatus.color(session.status)
      indicator.setAccessibilityValue(CodexRecoveryBannerStatus.label(session.status))
      let label = NSTextField(labelWithString: "\(session.project) / \(session.title) · \(session.short_id)")
      label.frame = NSRect(x: 26, y: y, width: 450, height: 22)
      label.font = .systemFont(ofSize: 12, weight: session.status == "in_progress" ? .bold : .medium)
      label.textColor = session.status == "failed" ? NSColor.systemRed : NSColor(white: 0.94, alpha: 1.0)
      label.lineBreakMode = .byTruncatingTail
      label.setAccessibilityRole(.staticText)
      document.addSubview(indicator)
      document.addSubview(label)
      rowLabels.append((indicator, label))
    }
    scroll.documentView = document
    scroll.hasVerticalScroller = sessions.count > Self.maxVisibleRows
    if sessions.count > Self.maxVisibleRows {
      scroll.contentView.scroll(to: NSPoint(x: 0, y: max(0, documentHeight - scroll.contentView.bounds.height)))
      scroll.reflectScrolledClipView(scroll.contentView)
    }
  }

  private func panelHeight(for count: Int) -> CGFloat {
    min(360, max(86, 62 + CGFloat(min(count, Self.maxVisibleRows)) * 24 + 8))
  }

  private func readPayload() -> Payload? {
    guard let data = CodexRecoveryPayloadReader.readData(from: payloadURL) else { return nil }
    guard let payload = try? JSONDecoder().decode(Payload.self, from: data),
      payload.version == 1,
      !payload.operation_id.isEmpty,
      !payload.title.isEmpty,
      !payload.explanation.isEmpty else { return nil }
    return payload
  }

  private func acquireDisplayOwnership() -> Bool {
    guard ownerFD < 0 else { return true }
    let parent = payloadURL.deletingLastPathComponent()
    var parentStat = stat()
    guard lstat(parent.path, &parentStat) == 0,
      (parentStat.st_mode & S_IFMT) == S_IFDIR,
      parentStat.st_uid == geteuid(), parentStat.st_mode & 0o077 == 0 else { return false }
    let lockURL = parent.appendingPathComponent("restore-banner.display.lock")
    let fd = lockURL.path.withCString { open($0, O_CREAT | O_RDWR | O_NOFOLLOW, 0o600) }
    guard fd >= 0 else { return false }
    var info = stat()
    guard fstat(fd, &info) == 0,
      (info.st_mode & S_IFMT) == S_IFREG,
      info.st_uid == geteuid(), info.st_mode & 0o077 == 0 else {
      close(fd)
      return false
    }
    guard flock(fd, LOCK_EX | LOCK_NB) == 0 else {
      close(fd)
      return false
    }
    ownerFD = fd
    return true
  }

  private func releaseDisplayOwnership() {
    guard ownerFD >= 0 else { return }
    _ = flock(ownerFD, LOCK_UN)
    close(ownerFD)
    ownerFD = -1
  }

  private func writeReadyMarker() {
    let marker = payloadURL.deletingLastPathComponent().appendingPathComponent("restore-banner.ready")
    let data = Data("visible\n".utf8)
    FileManager.default.createFile(
      atPath: marker.path,
      contents: data,
      attributes: [.posixPermissions: 0o600]
    )
    chmod(marker.path, 0o600)
  }

  private func currentCodexFrame(for identity: ProcessIdentity) -> CGRect? {
    guard identity.pid > 1,
      CodexRecoveryProcessIdentity.birth(for: identity.pid) == identity.birth_identity,
      Darwin.kill(identity.pid, 0) == 0 else { return nil }
    let windows = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID) as? [[String: Any]] ?? []
    var best: (frame: CGRect, area: CGFloat, named: Bool)?
    for info in windows {
      guard (info[kCGWindowOwnerPID as String] as? NSNumber)?.int32Value == identity.pid,
        (info[kCGWindowLayer as String] as? NSNumber)?.intValue == 0,
        (info[kCGWindowAlpha as String] as? NSNumber)?.doubleValue ?? 0 > 0,
        let boundsValue = info[kCGWindowBounds as String] else { continue }
      guard let frame = CGRect(dictionaryRepresentation: boundsValue as! CFDictionary),
        frame.width >= 300, frame.height >= 250 else { continue }
      let named = (info[kCGWindowName as String] as? String) == "ChatGPT"
      let area = frame.width * frame.height
      if best == nil || (named && !best!.named) || (named == best!.named && area > best!.area) {
        best = (frame, area, named)
      }
    }
    return best?.frame
  }

}
