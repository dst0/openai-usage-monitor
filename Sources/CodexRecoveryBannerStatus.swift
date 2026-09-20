import Cocoa

internal enum CodexRecoveryBannerStatus {
  static func indicator(_ status: String) -> String {
    switch status {
    case "completed": return "✓"
    case "in_progress": return "◐"
    case "failed": return "✕"
    case "skipped": return "↷"
    default: return "○"
    }
  }

  static func label(_ status: String) -> String {
    switch status {
    case "completed": return "Завершено"
    case "in_progress": return "Выполняется"
    case "failed": return "Ошибка"
    case "skipped": return "Пропущено"
    default: return "Ожидание"
    }
  }

  static func color(_ status: String) -> NSColor {
    switch status {
    case "completed": return .systemGreen
    case "in_progress": return .systemOrange
    case "failed": return .systemRed
    default: return .secondaryLabelColor
    }
  }
}
