import Cocoa

@main
struct CodexRecoveryBannerGeometryTests {
  static func main() {
    let screen = CGRect(x: -2560, y: -339, width: 2560, height: 1440)
    let window = CodexRecoveryBanner.appKitFrame(
      for: CGRect(x: -2560, y: -139, width: 2560, height: 1330),
      on: CGRect(x: -2560, y: -169, width: 2560, height: 1440),
      within: screen
    )
    precondition(window.minY == -259 && window.maxY == 1071)
    let frame = CodexRecoveryBanner.panelFrame(
      for: window, within: screen, width: 540, height: 110
    )
    precondition(screen.contains(frame), "Banner must remain fully visible on the window display")
    precondition(frame.midX == window.midX, "Banner must stay centered over the ChatGPT window")

    let shifted = CodexRecoveryBanner.panelFrame(
      for: CGRect(x: 1800, y: 900, width: 700, height: 500),
      within: CGRect(x: 0, y: 0, width: 1920, height: 1080),
      width: 540, height: 110
    )
    precondition(shifted.maxX <= 1920 && shifted.maxY <= 1080)
    let partlyVisible = CGRect(x: -2800, y: -100, width: 400, height: 500)
    precondition(CodexRecoveryBanner.displayIndex(
      for: partlyVisible,
      among: [CGRect(x: -2560, y: -169, width: 2560, height: 1440)]
    ) == 0, "A visible edge must select its display even when the center is outside")
    precondition(CodexRecoveryBanner.displayIndex(
      for: CGRect(x: 100, y: -1100, width: 800, height: 800),
      among: [CGRect(x: 0, y: 0, width: 1440, height: 932),
              CGRect(x: 0, y: -1200, width: 1000, height: 1200)]
    ) == 1, "A vertically stacked display must use CoreGraphics bounds")
    print("Recovery banner geometry tests passed")
  }
}
