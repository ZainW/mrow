import AppKit
import os
import SwiftUI

/// Fonts and measurements for the window, matching Mdow's chrome.
enum Theme {
  /// The strip across the top of the window with the window buttons and actions.
  static let headerHeight: CGFloat = 40
  /// The margin between the card and the window's edges.
  static let gutter: CGFloat = 8
  static let cardRadius: CGFloat = 12
  /// Room for the close, minimise and zoom buttons at the left of the header.
  static let trafficLightsWidth: CGFloat = 76
  /// The widest the merge request's text column gets.
  static let readingWidth: CGFloat = 780

  /// Registers the bundled fonts for this process.
  static func registerFonts() {
    guard let folder = Bundle.main.resourceURL?.appendingPathComponent("Fonts"),
      let files = try? FileManager.default.contentsOfDirectory(at: folder, includingPropertiesForKeys: nil)
    else { return }
    for file in files where ["woff2", "ttf", "otf"].contains(file.pathExtension) {
      CTFontManagerRegisterFontsForURL(file as CFURL, .process, nil)
    }
  }

  /// Inter, falling back to the system font when it isn't bundled (e.g. `swift run`).
  static func ui(_ size: CGFloat, _ weight: NSFont.Weight = .regular) -> NSFont {
    cached("ui", size, weight) {
      variable(family: "Inter Variable", size: size, weight: weight) ?? .systemFont(ofSize: size, weight: weight)
    }
  }

  /// Geist Mono, for paths, branches, key hints and diffs.
  static func mono(_ size: CGFloat, _ weight: NSFont.Weight = .regular) -> NSFont {
    cached("mono", size, weight) {
      variable(family: "Geist Mono", size: size, weight: weight) ?? .monospacedSystemFont(ofSize: size, weight: weight)
    }
  }

  /// Charter, Mdow's reading face, for titles and descriptions.
  static func serif(_ size: CGFloat, _ weight: NSFont.Weight = .regular) -> NSFont {
    cached("serif", size, weight) { charter(size, weight) }
  }

  private static func charter(_ size: CGFloat, _ weight: NSFont.Weight) -> NSFont {
    let descriptor = NSFontDescriptor(fontAttributes: [
      .family: "Charter",
      .traits: [NSFontDescriptor.TraitKey.weight: weight.rawValue],
    ])
    return NSFont(descriptor: descriptor, size: size) ?? .systemFont(ofSize: size, weight: weight)
  }

  /// Every view asks for its font on every draw, and building one means matching descriptors
  /// in CoreText, so each face, size and weight is made once.
  /// NSFont is immutable, so sharing one across threads is safe.
  private static let fonts = OSAllocatedUnfairLock<[FontKey: NSFont]>(uncheckedState: [:])

  private struct FontKey: Hashable {
    let face: String
    let size: CGFloat
    let weight: CGFloat
  }

  private static func cached(_ face: String, _ size: CGFloat, _ weight: NSFont.Weight, make: () -> NSFont) -> NSFont {
    let key = FontKey(face: face, size: size, weight: weight.rawValue)
    if let font = fonts.withLockUnchecked({ $0[key] }) { return font }
    let font = make()
    fonts.withLockUnchecked { $0[key] = font }
    return font
  }

  /// The bundled fonts are variable, so weights are set on the weight axis rather than by name.
  private static func variable(family: String, size: CGFloat, weight: NSFont.Weight) -> NSFont? {
    guard let base = NSFont(descriptor: NSFontDescriptor(fontAttributes: [.family: family]), size: size),
      base.familyName == family
    else { return nil }
    let axis = NSNumber(value: 0x7767_6874)  // 'wght'
    let descriptor = base.fontDescriptor.addingAttributes([
      .variation: [axis: NSNumber(value: cssWeight(weight))]
    ])
    return NSFont(descriptor: descriptor, size: size) ?? base
  }

  /// `NSFont.Weight` on CSS's 100–900 scale.
  private static func cssWeight(_ weight: NSFont.Weight) -> Double {
    let steps: [(NSFont.Weight, Double)] = [
      (.ultraLight, 200), (.thin, 250), (.light, 300), (.regular, 400), (.medium, 500),
      (.semibold, 600), (.bold, 700), (.heavy, 800), (.black, 900),
    ]
    return steps.min { abs($0.0.rawValue - weight.rawValue) < abs($1.0.rawValue - weight.rawValue) }!.1
  }
}

extension Font {
  static func mrowUI(_ size: CGFloat, _ weight: NSFont.Weight = .regular) -> Font {
    Font(Theme.ui(size, weight) as CTFont)
  }

  static func mrowMono(_ size: CGFloat, _ weight: NSFont.Weight = .regular) -> Font {
    Font(Theme.mono(size, weight) as CTFont)
  }

  static func mrowSerif(_ size: CGFloat, _ weight: NSFont.Weight = .regular) -> Font {
    Font(Theme.serif(size, weight) as CTFont)
  }
}
