import AppKit

/// Code colours: soft, warm hues that sit on the paper in light mode and the ink in dark mode.
enum CodeTheme {
  public static func color(for kind: TokenKind) -> NSColor {
    switch kind {
    case .plain, .operator_, .punctuation: Palette.text
    case .keyword, .constant: keyword
    case .string: string
    case .number: number
    case .comment: comment
    case .type: type
    case .function, .property: function
    case .attribute: attribute
    case .tag, .heading: declaration
    case .preprocessor: keyword
    case .regex: number
    case .link: declaration
    case .deleted: deleted
    case .inserted: string
    }
  }

  /// Only markdown headings are bold, so code reads as an even texture.
  public static func isBold(_ kind: TokenKind) -> Bool {
    kind == .heading
  }

  private static let keyword = Palette.pair(0x9E3F6C, 0xE58AB4)
  private static let string = Palette.pair(0x3C7A36, 0xA6D28F)
  private static let number = Palette.pair(0xAD531C, 0xF0A46C)
  private static let comment = Palette.pair(0x99948A, 0x727272)
  private static let type = Palette.pair(0x2C6C8C, 0x7CC4E4)
  private static let function = Palette.pair(0x6249B0, 0xB9A5F5)
  private static let attribute = Palette.pair(0x86670F, 0xE3C178)
  private static let declaration = Palette.pair(0x2C6C8C, 0x7CC4E4)
  private static let deleted = Palette.pair(0xB3412E, 0xF08A7A)
}
