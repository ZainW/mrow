import AppKit
import SwiftUI

/// A picture in markdown: `![alt](src){width=… height=…}` (GitLab writes the size it was pasted
/// at), optionally wrapped in a link as `[![alt](src)](href)`.
struct MarkdownImage: Hashable {
  let alt: String
  let source: String
  let width: CGFloat?
  let height: CGFloat?
  let link: String?

  enum Segment: Hashable {
    case text(String)
    case image(MarkdownImage)
  }

  private var fileExtension: String {
    let path = source.split(separator: "?").first.map(String.init) ?? source
    return (path as NSString).pathExtension.lowercased()
  }

  var isVideo: Bool { ["mov", "mp4", "m4v", "webm"].contains(fileExtension) }

  /// CI badges and "Open in…" buttons: small, and meant to sit in a line of text.
  var isBadge: Bool { fileExtension == "svg" || (height ?? .infinity) <= 40 }

  // Linked image (groups 1–4) or a bare one (5–7).
  private static let pattern = try! NSRegularExpression(
    pattern: #"\[!\[([^\]]*)\]\(([^)\s]+)(?:\s+"[^"]*")?\)(\{[^}]*\})?\]\(([^)\s]+)\)"#
      + #"|!\[([^\]]*)\]\(([^)\s]+)(?:\s+"[^"]*")?\)(\{[^}]*\})?"#)

  /// Splits text around its pictures. Badges are left in the text unless `badges` is set.
  static func segments(_ text: String, badges: Bool = false) -> [Segment] {
    let ns = text as NSString
    var result: [Segment] = []
    var cursor = 0

    func addText(upTo end: Int) {
      let piece = ns.substring(with: NSRange(location: cursor, length: end - cursor))
        .trimmingCharacters(in: .whitespacesAndNewlines)
      if !piece.isEmpty { result.append(.text(piece)) }
    }

    for match in pattern.matches(in: text, range: NSRange(location: 0, length: ns.length)) {
      func group(_ index: Int) -> String? {
        let range = match.range(at: index)
        return range.location == NSNotFound ? nil : ns.substring(with: range)
      }
      let linked = group(2) != nil
      let attributes = size(group(linked ? 3 : 7))
      let image = MarkdownImage(
        alt: group(linked ? 1 : 5) ?? "", source: group(linked ? 2 : 6) ?? "",
        width: attributes.width, height: attributes.height, link: linked ? group(4) : nil)
      guard badges || !image.isBadge || image.isVideo else { continue }
      addText(upTo: match.range.location)
      result.append(.image(image))
      cursor = match.range.location + match.range.length
    }
    addText(upTo: ns.length)
    return result.isEmpty ? [.text(text)] : result
  }

  /// `{width=900 height=117}` or `{width="131" height="28"}`; percentages are ignored.
  private static func size(_ attributes: String?) -> (width: CGFloat?, height: CGFloat?) {
    guard let attributes else { return (nil, nil) }
    func value(_ name: String) -> CGFloat? {
      guard let range = attributes.range(of: name + #"\s*=\s*"?\d+(\.\d+)?"?(?![\d.%])"#, options: .regularExpression)
      else { return nil }
      let digits = attributes[range].drop { !$0.isNumber }.prefix { $0.isNumber || $0 == "." }
      return Double(digits).map { CGFloat($0) }
    }
    return (value("width"), value("height"))
  }

  private static let imageTag = try! NSRegularExpression(pattern: #"<img\b[^>]*>"#, options: .caseInsensitive)
  private static let attribute = try! NSRegularExpression(pattern: #"(\w+)\s*=\s*"([^"]*)""#)

  /// `<img`, in any case, found byte by byte: String's searches cost as much as parsing the rest.
  private static func hasImageTag(_ text: String) -> Bool {
    var text = text
    return text.withUTF8 { bytes in
      guard bytes.count >= 4 else { return false }
      for index in 0..<(bytes.count - 3) where bytes[index] == UInt8(ascii: "<") {
        if bytes[index + 1] | 0x20 == UInt8(ascii: "i"), bytes[index + 2] | 0x20 == UInt8(ascii: "m"),
          bytes[index + 3] | 0x20 == UInt8(ascii: "g")
        {
          return true
        }
      }
      return false
    }
  }

  /// `<img src alt width height>` as markdown, and `<a href>` around one as a linked image, the
  /// way bots write their buttons.
  static func markdownForHTML(_ text: String) -> String {
    guard hasImageTag(text) else { return text }
    let ns = text as NSString
    var output = ""
    var cursor = 0
    for match in imageTag.matches(in: text, range: NSRange(location: 0, length: ns.length)) {
      output += ns.substring(with: NSRange(location: cursor, length: match.range.location - cursor))
      let tag = ns.substring(with: match.range)
      var values: [String: String] = [:]
      for found in attribute.matches(in: tag, range: NSRange(location: 0, length: (tag as NSString).length)) {
        values[(tag as NSString).substring(with: found.range(at: 1)).lowercased()] =
          (tag as NSString).substring(with: found.range(at: 2))
      }
      if let src = values["src"], !src.isEmpty {
        let size = ["width", "height"].compactMap { name in values[name].map { "\(name)=\($0)" } }
        output += "![\(values["alt"] ?? "")](\(src))" + (size.isEmpty ? "" : "{\(size.joined(separator: " "))}")
      }
      cursor = match.range.location + match.range.length
    }
    output += ns.substring(from: cursor)
    return output.replacingOccurrences(
      of: #"<a\b[^>]*\bhref="([^"]+)"[^>]*>\s*(!\[[^\]]*\]\([^)]*\)(?:\{[^}]*\})?)\s*</a>"#, with: "[$2]($1)",
      options: [.regularExpression, .caseInsensitive])
  }
}

/// A picture loaded natively: shown at the size it was pasted at (never wider than the text),
/// with its space kept while it loads so the text below doesn't jump. A click opens it in Preview.
struct MarkdownImageView: View {
  let image: MarkdownImage
  let baseURL: URL?
  /// In a table, where the picture's width is what sizes its column.
  var inTable = false
  /// A status icon drawn at the height of the text beside it.
  var badgeHeight: CGFloat?
  private let resolved: (source: ImageSource, web: URL?)?

  @State private var loaded: LoadedImage?
  @State private var failed = false
  @State private var opening = false

  init(image: MarkdownImage, baseURL: URL?, inTable: Bool = false, badgeHeight: CGFloat? = nil) {
    self.image = image
    self.baseURL = baseURL
    self.inTable = inTable
    self.badgeHeight = badgeHeight
    resolved = ImageSource.resolve(image.source, base: baseURL)
  }

  var body: some View {
    if let resolved {
      if let badgeHeight {
        badge(resolved.source, height: badgeHeight)
      } else if image.isVideo {
        video(resolved.source, web: resolved.web)
      } else if failed {
        chip(symbol: "photo", title: image.alt.isEmpty ? resolved.source.fileName : image.alt, detail: "Couldn't load") {
          if let web = resolved.web { NSWorkspace.shared.open(web) }
        }
      } else {
        picture(resolved.source, web: resolved.web)
      }
    } else {
      chip(symbol: "photo", title: image.alt.isEmpty ? image.source : image.alt, detail: nil) {}
    }
  }

  private func picture(_ source: ImageSource, web: URL?) -> some View {
    let shown = loaded ?? ImageLoader.cached(source)
    let ratio: CGFloat =
      if let width = image.width, let height = image.height, width > 0, height > 0 { width / height }
      else if let size = shown?.size, size.height > 0 { size.width / size.height }
      else { 16 / 9 }
    let width = image.width ?? shown?.size.width ?? 480
    return ZStack {
      if let shown {
        Image(decorative: shown.image, scale: 1).resizable().interpolation(.high)
      } else {
        Rectangle().fill(Color.mrowWell)
        ProgressView().controlSize(.small)
      }
    }
    .aspectRatio(ratio, contentMode: .fit)
    // In a table the column is sized from the ideal width, and the picture shrinks to fit it.
    .frame(idealWidth: inTable ? width : nil, maxWidth: width)
    .clipShape(RoundedRectangle(cornerRadius: 6, style: .continuous))
    .overlay(RoundedRectangle(cornerRadius: 6, style: .continuous).strokeBorder(Color.mrowLine))
    .onTapGesture { open(shown, web: web) }
    .pointerStyle(.link)
    .help(image.alt.isEmpty ? source.fileName : image.alt)
    .accessibilityLabel(image.alt)
    .frame(maxWidth: inTable ? nil : .infinity, alignment: .leading)
    .task(id: source.key) {
      guard shown == nil else { return }
      loaded = await ImageLoader.shared.image(source)
      failed = loaded == nil
    }
  }

  private func badge(_ source: ImageSource, height: CGFloat) -> some View {
    let shown = loaded ?? ImageLoader.cached(source)
    let ratio: CGFloat =
      if let width = image.width, let declared = image.height, declared > 0 { width / declared }
      else if let size = shown?.size, size.height > 0 { size.width / size.height }
      else { 1 }
    return Group {
      if let shown { Image(decorative: shown.image, scale: 1).resizable().interpolation(.high) } else { Color.clear }
    }
    .frame(width: height * ratio, height: height)
    .help(image.alt)
    .task(id: source.key) {
      guard shown == nil else { return }
      loaded = await ImageLoader.shared.image(source)
    }
  }

  private func open(_ shown: LoadedImage?, web: URL?) {
    if let link = image.link, let url = URL(string: link, relativeTo: baseURL) {
      NSWorkspace.shared.open(url)
    } else if let file = shown?.file {
      NSWorkspace.shared.open(file)
    } else if let web {
      NSWorkspace.shared.open(web)
    }
  }

  /// Videos aren't played inline: a click fetches the file (through glab, for an upload) and opens
  /// it in QuickTime.
  private func video(_ source: ImageSource, web: URL?) -> some View {
    chip(
      symbol: "play.rectangle.fill", title: image.alt.isEmpty ? source.fileName : image.alt,
      detail: opening ? "Downloading…" : "Video", busy: opening
    ) {
      guard !opening else { return }
      opening = true
      Task {
        if let file = await ImageLoader.shared.file(source) {
          NSWorkspace.shared.open(file)
        } else if let web {
          NSWorkspace.shared.open(web)
        }
        opening = false
      }
    }
  }

  private func chip(
    symbol: String, title: String, detail: String?, busy: Bool = false, action: @escaping () -> Void
  ) -> some View {
    Button(action: action) {
      HStack(spacing: 8) {
        if busy {
          ProgressView().controlSize(.small)
        } else {
          Image(systemName: symbol).foregroundStyle(Color.mrowAccent)
        }
        Text(title).foregroundStyle(Color.mrowText).lineLimit(1).truncationMode(.middle)
        if let detail { Text(detail).foregroundStyle(Color.mrowTertiary) }
      }
      .font(.mrowUI(13))
      .padding(.horizontal, 12)
      .padding(.vertical, 8)
      .background(Color.mrowWell, in: RoundedRectangle(cornerRadius: 8, style: .continuous))
      .overlay(RoundedRectangle(cornerRadius: 8, style: .continuous).strokeBorder(Color.mrowLine))
    }
    .buttonStyle(.plain)
    .pointerStyle(.link)
    .frame(maxWidth: inTable ? nil : .infinity, alignment: .leading)
  }
}

extension String {
  /// A plain byte search. `contains` on a String goes through Foundation, which for long bot
  /// comments costs as much as parsing them.
  func has(_ needle: StaticString) -> Bool {
    let found = utf8.withContiguousStorageIfAvailable { haystack -> Bool in
      guard let base = haystack.baseAddress, haystack.count >= needle.utf8CodeUnitCount else { return false }
      return memmem(base, haystack.count, needle.utf8Start, needle.utf8CodeUnitCount) != nil
    }
    return found ?? contains(needle.description)
  }
}
