import AppKit
import Foundation

// MARK: - Mouse, Cursor & Hover Tracking
extension AccountSectionCardView {
  internal func pushPointingCursor() {
    if !isCursorPushed {
      NSCursor.pointingHand.push()
      isCursorPushed = true
    }
  }

  internal func popPointingCursor() {
    if isCursorPushed {
      NSCursor.pop()
      isCursorPushed = false
    }
  }

  public override func viewDidMoveToWindow() {
    super.viewDidMoveToWindow()
    if window == nil { popPointingCursor() }
  }

  public override func updateTrackingAreas() {
    super.updateTrackingAreas()
    if let old = cardTrackingArea { removeTrackingArea(old) }
    let area = NSTrackingArea(
      rect: bounds,
      options: [
        .mouseEnteredAndExited, .mouseMoved, .cursorUpdate, .activeInActiveApp,
        .inVisibleRect,
      ], owner: self, userInfo: nil)
    addTrackingArea(area)
    self.cardTrackingArea = area
  }

  public override func resetCursorRects() {
    super.resetCursorRects()
    for btn in resetButtons + reloginButtons {
      addCursorRect(
        convert(btn.frame, from: contentContainer), cursor: .pointingHand)
    }
    for view in switchButtonsViews {
      if !view.isCliActive {
        addCursorRect(
          convert(view.cliButton.frame, from: view), cursor: .pointingHand)
      }
      if !view.isAppActive {
        addCursorRect(
          convert(view.appButton.frame, from: view), cursor: .pointingHand)
      }
    }
  }

  public override func mouseEntered(with event: NSEvent) {
    super.mouseEntered(with: event)
    isCardHovered = true
    box.borderColor = NSColor.labelColor.withAlphaComponent(0.38)
    box.fillColor = NSColor.quaternaryLabelColor.withAlphaComponent(0.08)
    box.needsDisplay = true
  }

  public override func mouseMoved(with event: NSEvent) {
    super.mouseMoved(with: event)
    let pt = contentContainer.convert(event.locationInWindow, from: nil)
    var targetBtn: NSButton? = (resetButtons + reloginButtons).first(where: {
      $0.frame.contains(pt)
    })
    if targetBtn == nil {
      for view in switchButtonsViews {
        let ptInView = view.convert(pt, from: contentContainer)
        if let btn = view.button(at: ptInView) {
          targetBtn = btn
          break
        }
      }
    }

    if hoveredButton !== targetBtn {
      (hoveredButton as? MenuIconButton)?.setHoveredExplicitly(false)
      (targetBtn as? MenuIconButton)?.setHoveredExplicitly(true)
      hoveredButton = targetBtn
      if targetBtn != nil { pushPointingCursor() } else { popPointingCursor() }
    }
  }

  public override func cursorUpdate(with event: NSEvent) {
    let pt = contentContainer.convert(event.locationInWindow, from: nil)
    var isOver = hoveredButton != nil || (resetButtons + reloginButtons).contains(where: { $0.frame.contains(pt) })
    if !isOver {
      for view in switchButtonsViews {
        let ptInView = view.convert(pt, from: contentContainer)
        if view.button(at: ptInView) != nil {
          isOver = true
          break
        }
      }
    }
    if isOver { NSCursor.pointingHand.set() } else { NSCursor.arrow.set() }
  }

  public override func mouseExited(with event: NSEvent) {
    super.mouseExited(with: event)
    isCardHovered = false
    popPointingCursor()
    (hoveredButton as? MenuIconButton)?.setHoveredExplicitly(false)
    hoveredButton = nil
    box.borderColor = NSColor.separatorColor.withAlphaComponent(0.65)
    box.fillColor = NSColor.controlBackgroundColor.withAlphaComponent(0.25)
    box.needsDisplay = true
    NSCursor.arrow.set()
  }

  public override func mouseUp(with event: NSEvent) {
    let point = contentContainer.convert(event.locationInWindow, from: nil)
    for btn in resetButtons where btn.frame.contains(point) {
      handleResetButton(btn)
      return
    }
    for view in switchButtonsViews {
      let ptInView = view.convert(point, from: contentContainer)
      if let btn = view.button(at: ptInView) {
        if btn === view.cliButton {
          view.handleCliClicked()
        } else {
          view.handleAppClicked()
        }
        return
      }
    }
    for btn in reloginButtons where btn.frame.contains(point) {
      handleReloginButton(btn)
      return
    }
    for row in accountRows where row.frame.contains(point) {
      enclosingMenuItem?.menu?.cancelTracking()
      if row.needsRelogin {
        onRelogin(row.accountId, row.accountEmail)
      } else {
        onSwitchCli(row.accountId)
      }
      return
    }
    super.mouseUp(with: event)
  }

  @objc internal func handleResetButton(_ sender: NSButton) {
    guard let id = sender.identifier?.rawValue, !id.isEmpty else { return }
    let email =
      entries.first(where: { $0.account.id == id })?.account.email ?? id
    enclosingMenuItem?.menu?.cancelTracking()
    onReset(id, email)
  }

  @objc internal func handleReloginButton(_ sender: NSButton) {
    guard let id = sender.identifier?.rawValue, !id.isEmpty else { return }
    let email =
      entries.first(where: { $0.account.id == id })?.account.email ?? ""
    enclosingMenuItem?.menu?.cancelTracking()
    onRelogin(id, email)
  }
}
