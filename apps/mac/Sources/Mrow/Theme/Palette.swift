import AppKit
import SwiftUI

/// Mdow's colours: warm paper in light mode, neutral ink in dark mode, with one clay accent.
/// All are dynamic, so anything drawn with them follows the window's appearance.
enum Palette {
  /// The window itself: sidebar, header and the margin around the card.
  static let chrome = pair(0xF3F1EB, 0x131313)
  /// The card the merge request sits on.
  static let paper = pair(0xFDFCF9, 0x1B1B1B)
  /// Things that float: the active tab, menus, sheets.
  static let elevated = pair(0xFFFFFF, 0x242424)
  /// Recessed areas inside the paper, like code and diffs.
  static let well = pair(0xF5F3ED, 0x161616)

  static let text = pair(0x1E1C19, 0xECECEC)
  static let secondaryText = pair(0x6B665E, 0xA3A3A3)
  static let tertiaryText = pair(0x9E998F, 0x6E6E6E)

  /// Hairlines between surfaces.
  static let line = dynamic(light: ink(0.1), dark: glow(0.08))
  /// Lines that need to read as structure, like table rules.
  static let strongLine = dynamic(light: ink(0.16), dark: glow(0.14))
  static let hover = dynamic(light: ink(0.05), dark: glow(0.05))
  static let selected = dynamic(light: ink(0.085), dark: glow(0.085))

  static let accent = pair(0xB95A33, 0xE08A66)
  static let accentSoft = dynamic(
    light: rgb(0xB95A33).withAlphaComponent(0.12), dark: rgb(0xE08A66).withAlphaComponent(0.16))

  // Review states and diffs, in the same soft key as the code theme.
  static let green = pair(0x3F7D4E, 0x7BC48D)
  static let red = pair(0xB4443A, 0xE5847A)
  static let amber = pair(0x9A6B12, 0xD9AE5C)
  static let blue = pair(0x3C6A9E, 0x86AEDB)
  static let addedLine = dynamic(
    light: rgb(0x3F9A55).withAlphaComponent(0.11), dark: rgb(0x5FBF77).withAlphaComponent(0.11))
  static let removedLine = dynamic(
    light: rgb(0xC9483C).withAlphaComponent(0.09), dark: rgb(0xE5655A).withAlphaComponent(0.11))
  static let addedWord = dynamic(
    light: rgb(0x3F9A55).withAlphaComponent(0.22), dark: rgb(0x5FBF77).withAlphaComponent(0.22))
  static let removedWord = dynamic(
    light: rgb(0xC9483C).withAlphaComponent(0.2), dark: rgb(0xE5655A).withAlphaComponent(0.22))

  static func rgb(_ hex: UInt32) -> NSColor {
    NSColor(
      srgbRed: CGFloat((hex >> 16) & 0xFF) / 255, green: CGFloat((hex >> 8) & 0xFF) / 255,
      blue: CGFloat(hex & 0xFF) / 255, alpha: 1)
  }

  static func pair(_ light: UInt32, _ dark: UInt32) -> NSColor {
    dynamic(light: rgb(light), dark: rgb(dark))
  }

  static func dynamic(light: NSColor, dark: NSColor) -> NSColor {
    NSColor(name: nil) { appearance in
      appearance.bestMatch(from: [.aqua, .darkAqua]) == .darkAqua ? dark : light
    }
  }

  /// The light theme's warm near-black at an opacity.
  private static func ink(_ alpha: CGFloat) -> NSColor { rgb(0x3C2D1E).withAlphaComponent(alpha) }
  private static func glow(_ alpha: CGFloat) -> NSColor { NSColor(white: 1, alpha: alpha) }
}

extension Color {
  static let mrowChrome = Color(nsColor: Palette.chrome)
  static let mrowPaper = Color(nsColor: Palette.paper)
  static let mrowElevated = Color(nsColor: Palette.elevated)
  static let mrowWell = Color(nsColor: Palette.well)
  static let mrowText = Color(nsColor: Palette.text)
  static let mrowSecondary = Color(nsColor: Palette.secondaryText)
  static let mrowTertiary = Color(nsColor: Palette.tertiaryText)
  static let mrowLine = Color(nsColor: Palette.line)
  static let mrowStrongLine = Color(nsColor: Palette.strongLine)
  static let mrowHover = Color(nsColor: Palette.hover)
  static let mrowSelected = Color(nsColor: Palette.selected)
  static let mrowAccent = Color(nsColor: Palette.accent)
  static let mrowAccentSoft = Color(nsColor: Palette.accentSoft)
  static let mrowGreen = Color(nsColor: Palette.green)
  static let mrowRed = Color(nsColor: Palette.red)
  static let mrowAmber = Color(nsColor: Palette.amber)
  static let mrowBlue = Color(nsColor: Palette.blue)
  static let mrowAddedLine = Color(nsColor: Palette.addedLine)
  static let mrowRemovedLine = Color(nsColor: Palette.removedLine)
  static let mrowAddedWord = Color(nsColor: Palette.addedWord)
  static let mrowRemovedWord = Color(nsColor: Palette.removedWord)
}
