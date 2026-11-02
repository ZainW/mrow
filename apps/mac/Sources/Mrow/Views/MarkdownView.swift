import SwiftUI

/// The block structure of GitLab-flavoured markdown, enough for descriptions and comments:
/// headings, paragraphs, lists and task lists, code, quotes, tables, rules and collapsible
/// `<details>` sections (which bots nest several deep). Inline markup is
/// left to `AttributedString(markdown:)`, and images to `MarkdownImage`.
enum MarkdownBlock: Hashable {
  enum Marker: Hashable {
    case bullet
    case number(String)
    case task(Bool)
  }

  case heading(Int, String)
  case paragraph(String)
  case listItem(indent: Int, marker: Marker, text: String)
  case code(language: String?, text: String)
  case quote(String)
  case table(header: [String], rows: [[String]])
  case rule
  /// `<details>`: the body stays unparsed until it's opened, so long bot comments cost little.
  case details(summary: String, open: Bool, body: String)

  static func parse(_ source: String) -> [MarkdownBlock] {
    sections(clean(source))
  }

  /// Splits out top-level `<details>` elements, matching nested ones, and parses the rest.
  private static func sections(_ text: String) -> [MarkdownBlock] {
    guard text.has("<details") else { return lines(text) }
    var blocks: [MarkdownBlock] = []
    var rest = Substring(text)
    while let open = rest.range(of: "<details"), let tagEnd = rest[open.upperBound...].firstIndex(of: ">") {
      blocks += lines(String(rest[..<open.lowerBound]))
      let isOpen = rest[open.upperBound..<tagEnd].contains("open")
      var depth = 1
      var cursor = rest.index(after: tagEnd)
      var bodyEnd = rest.endIndex
      var after = rest.endIndex
      // An unclosed section runs to the end, as in a browser.
      while depth > 0, let close = rest[cursor...].range(of: "</details>") {
        if let nested = rest[cursor..<close.lowerBound].range(of: "<details") {
          depth += 1
          cursor = nested.upperBound
        } else {
          depth -= 1
          cursor = close.upperBound
          if depth == 0 { (bodyEnd, after) = (close.lowerBound, close.upperBound) }
        }
      }
      var body = rest[rest.index(after: tagEnd)..<bodyEnd]
      var summary = "Details"
      if let start = body.range(of: "<summary"), let end = body[start.upperBound...].firstIndex(of: ">"),
        let close = body[end...].range(of: "</summary>")
      {
        summary = body[body.index(after: end)..<close.lowerBound].trimmingCharacters(in: .whitespacesAndNewlines)
        body = body[close.upperBound...]
      }
      blocks.append(.details(summary: summary, open: isOpen, body: String(body)))
      rest = rest[after...]
    }
    return blocks + lines(String(rest))
  }

  private static func lines(_ text: String) -> [MarkdownBlock] {
    let lines = text.components(separatedBy: "\n")
    var blocks: [MarkdownBlock] = []
    var paragraph: [String] = []
    var index = 0

    func flush() {
      let text = paragraph.joined(separator: "\n").trimmingCharacters(in: .whitespacesAndNewlines)
      if !text.isEmpty { blocks.append(.paragraph(text)) }
      paragraph = []
    }

    while index < lines.count {
      let line = lines[index]
      let trimmed = line.trimmingCharacters(in: .whitespaces)

      if trimmed.hasPrefix("```") || trimmed.hasPrefix("~~~") {
        flush()
        let fence = String(trimmed.prefix(3))
        let language = trimmed.dropFirst(3).trimmingCharacters(in: .whitespaces)
        var code: [String] = []
        index += 1
        while index < lines.count, !lines[index].trimmingCharacters(in: .whitespaces).hasPrefix(fence) {
          code.append(lines[index])
          index += 1
        }
        blocks.append(.code(language: language.isEmpty ? nil : language, text: code.joined(separator: "\n")))
        index += 1
        continue
      }

      if trimmed.isEmpty {
        flush()
        index += 1
        continue
      }

      if let heading = heading(trimmed) {
        flush()
        blocks.append(heading)
        index += 1
        continue
      }

      if isRule(trimmed) {
        // "Title\n---" is a setext heading; a lone --- is a rule.
        if !paragraph.isEmpty, trimmed.hasPrefix("-") || trimmed.hasPrefix("=") {
          let text = paragraph.joined(separator: " ")
          paragraph = []
          blocks.append(.heading(trimmed.hasPrefix("=") ? 1 : 2, text))
        } else {
          flush()
          blocks.append(.rule)
        }
        index += 1
        continue
      }

      if trimmed.contains("|"), index + 1 < lines.count, isTableSeparator(lines[index + 1]) {
        flush()
        let header = cells(trimmed)
        var rows: [[String]] = []
        index += 2
        while index < lines.count, lines[index].contains("|"),
          !lines[index].trimmingCharacters(in: .whitespaces).isEmpty
        {
          rows.append(cells(lines[index]))
          index += 1
        }
        blocks.append(.table(header: header, rows: rows))
        continue
      }

      if trimmed.hasPrefix(">") {
        flush()
        var quoted: [String] = []
        while index < lines.count {
          let next = lines[index].trimmingCharacters(in: .whitespaces)
          guard next.hasPrefix(">") else { break }
          quoted.append(String(next.dropFirst().drop { $0 == " " }))
          index += 1
        }
        blocks.append(.quote(quoted.joined(separator: "\n")))
        continue
      }

      if let item = listItem(line) {
        flush()
        blocks.append(item)
        index += 1
        // Indented lines that aren't items themselves continue the item.
        while index < lines.count {
          let next = lines[index]
          let nextTrimmed = next.trimmingCharacters(in: .whitespaces)
          guard !nextTrimmed.isEmpty, next.hasPrefix("  ") || next.hasPrefix("\t"), listItem(next) == nil,
            !nextTrimmed.hasPrefix("```"), case .listItem(let indent, let marker, let text) = blocks.last
          else { break }
          blocks[blocks.count - 1] = .listItem(indent: indent, marker: marker, text: text + "\n" + nextTrimmed)
          index += 1
        }
        continue
      }

      paragraph.append(line)
      index += 1
    }
    flush()
    return blocks
  }

  /// Drops HTML comments (GitLab templates are full of them) and turns the HTML that bots and
  /// templates write into markdown. `<details>` and `<summary>` are kept for `sections`.
  private static func clean(_ source: String) -> String {
    var text = source.has("\r\n") ? source.replacingOccurrences(of: "\r\n", with: "\n") : source
    guard text.has("<") else { return text }
    if text.has("<!--") {
      text = text.replacingOccurrences(of: "<!--[\\s\\S]*?-->", with: "", options: .regularExpression)
    }
    guard text.has("```") || text.has("~~~") else { return html(text) }
    // Code keeps its tags: only the text between fences is rewritten.
    var output: [String] = []
    var prose: [String] = []
    var fence: String?
    for line in text.components(separatedBy: "\n") {
      let trimmed = line.trimmingCharacters(in: .whitespaces)
      if let open = fence {
        output.append(line)
        if trimmed.hasPrefix(open) { fence = nil }
      } else if trimmed.hasPrefix("```") || trimmed.hasPrefix("~~~") {
        if !prose.isEmpty { output.append(html(prose.joined(separator: "\n"))) }
        prose = []
        fence = String(trimmed.prefix(3))
        output.append(line)
      } else {
        prose.append(line)
      }
    }
    if !prose.isEmpty { output.append(html(prose.joined(separator: "\n"))) }
    return output.joined(separator: "\n")
  }

  private static let tag = try! NSRegularExpression(
    pattern: #"<(/?)(br|strong|b|em|i|code|li|ul|ol|p|div|span|blockquote|sub|sup|picture|source)\b[^>]*>"#,
    options: .caseInsensitive)

  /// One pass over the tags: inline ones become markdown, wrappers are dropped.
  private static func html(_ text: String) -> String {
    guard text.has("<") else { return text }
    let ns = text as NSString
    var output = ""
    var cursor = 0
    for match in tag.matches(in: text, range: NSRange(location: 0, length: ns.length)) {
      output += ns.substring(with: NSRange(location: cursor, length: match.range.location - cursor))
      let closing = match.range(at: 1).length > 0
      switch ns.substring(with: match.range(at: 2)).lowercased() {
      case "br": output += inTableRow(ns, at: match.range.location) ? "\u{2028}" : "\n"
      case "strong", "b": output += "**"
      case "em", "i": output += "_"
      case "code": output += "`"
      case "li": output += closing ? "" : "\n- "
      case "ul", "ol": output += "\n"
      default: break
      }
      cursor = match.range.location + match.range.length
    }
    output += ns.substring(from: cursor)
    output = MarkdownImage.markdownForHTML(output)
    guard output.has("</a>") else { return output }
    return output.replacingOccurrences(
      of: #"<a\b[^>]*\bhref="([^"]+)"[^>]*>([^<]*)</a>"#, with: "[$2]($1)", options: [.regularExpression, .caseInsensitive])
  }

  /// A <br> in a table row breaks the line inside its cell rather than ending the row.
  private static func inTableRow(_ text: NSString, at location: Int) -> Bool {
    let newline = text.range(of: "\n", options: .backwards, range: NSRange(location: 0, length: location))
    let start = newline.location == NSNotFound ? 0 : newline.location + 1
    let line = text.substring(with: NSRange(location: start, length: location - start))
    return line.trimmingCharacters(in: .whitespaces).hasPrefix("|")
  }

  private static func heading(_ line: String) -> MarkdownBlock? {
    let hashes = line.prefix { $0 == "#" }.count
    guard (1...6).contains(hashes), line.dropFirst(hashes).first == " " else { return nil }
    let text = line.dropFirst(hashes).trimmingCharacters(in: .whitespaces)
    return .heading(hashes, text.replacingOccurrences(of: "#+$", with: "", options: .regularExpression))
  }

  private static func isRule(_ line: String) -> Bool {
    let compact = line.replacingOccurrences(of: " ", with: "")
    guard compact.count >= 3, let first = compact.first, "-*_=".contains(first) else { return false }
    return compact.allSatisfy { $0 == first }
  }

  private static func isTableSeparator(_ line: String) -> Bool {
    let trimmed = line.trimmingCharacters(in: .whitespaces)
    guard trimmed.contains("-") else { return false }
    return trimmed.allSatisfy { "|-: ".contains($0) }
  }

  private static func cells(_ line: String) -> [String] {
    var trimmed = line.trimmingCharacters(in: .whitespaces)
    if trimmed.hasPrefix("|") { trimmed.removeFirst() }
    if trimmed.hasSuffix("|") { trimmed.removeLast() }
    return trimmed.components(separatedBy: "|").map { $0.trimmingCharacters(in: .whitespaces) }
  }

  private static func listItem(_ line: String) -> MarkdownBlock? {
    let spaces = line.prefix { $0 == " " || $0 == "\t" }
      .reduce(0) { $0 + ($1 == "\t" ? 4 : 1) }
    let rest = line.drop { $0 == " " || $0 == "\t" }
    var marker: Marker
    var text: Substring
    if let first = rest.first, "-*+".contains(first), rest.dropFirst().first == " " {
      marker = .bullet
      text = rest.dropFirst(2)
    } else {
      let digits = rest.prefix { $0.isNumber }
      guard !digits.isEmpty, digits.count < 4 else { return nil }
      let after = rest.dropFirst(digits.count)
      guard let dot = after.first, dot == "." || dot == ")", after.dropFirst().first == " " else { return nil }
      marker = .number(String(digits))
      text = after.dropFirst(2)
    }
    if text.hasPrefix("[ ] ") || text.hasPrefix("[x] ") || text.hasPrefix("[X] ") {
      marker = .task(text.dropFirst(1).first != " ")
      text = text.dropFirst(4)
    }
    return .listItem(indent: spaces / 2, marker: marker, text: String(text))
  }
}

/// Markdown laid out natively, in Charter like Mdow's reader (or Inter, for compact places).
struct MarkdownView: View {
  var source: String
  /// Where relative links and uploads point, e.g. the project's web URL.
  var baseURL: URL?
  var size: CGFloat = 15.5
  var serif = true

  var body: some View {
    // Column gives each block all the height it needs, so text wraps rather than truncating.
    Column(spacing: size * 0.75) {
      ForEach(Array(MarkdownCache.blocks(source).enumerated()), id: \.offset) { _, block in
        blockView(for: block)
      }
    }
    .textSelection(.enabled)
    .frame(maxWidth: .infinity, alignment: .leading)
  }

  @ViewBuilder private func blockView(for block: MarkdownBlock) -> some View {
    switch block {
    case .heading(let level, let text):
      inline(text, size: headingSize(level), weight: .semibold)
        .padding(.top, level <= 2 ? size * 0.5 : size * 0.25)
    case .paragraph(let text):
      flow(text, lineSpacing: size * 0.3)
    case .listItem(let indent, let marker, let text):
      HStack(alignment: .firstTextBaseline, spacing: 8) {
        markerView(marker).frame(minWidth: 14, alignment: .trailing)
        flow(text, lineSpacing: size * 0.25)
      }
      .padding(.leading, CGFloat(indent) * 20)
      .padding(.vertical, -size * 0.3)
    case .code(let language, let text):
      CodeBlock(info: language, text: text, size: size)
    case .quote(let text):
      HStack(spacing: 12) {
        RoundedRectangle(cornerRadius: 1).fill(Color.mrowStrongLine).frame(width: 3)
        MarkdownView(source: text, baseURL: baseURL, size: size, serif: serif)
          .foregroundStyle(Color.mrowSecondary)
      }
    case .table(let header, let rows):
      table(header: header, rows: rows)
    case .rule:
      Rectangle().fill(Color.mrowLine).frame(height: 1).padding(.vertical, 4)
    case .details(let summary, let open, let body):
      DetailsSection(label: inline(summary, size: size, weight: .medium), initiallyOpen: open, size: size) {
        MarkdownView(source: body, baseURL: baseURL, size: size, serif: serif)
      }
    }
  }

  /// Text with its pictures pulled out onto lines of their own; badges stay in the text as links.
  @ViewBuilder private func flow(_ text: String, lineSpacing: CGFloat) -> some View {
    let segments = MarkdownCache.segments(text)
    if segments.count == 1, case .text(let only) = segments[0] {
      inline(only, size: size).lineSpacing(lineSpacing)
    } else {
      Column(spacing: size * 0.6) {
        ForEach(Array(segments.enumerated()), id: \.offset) { _, segment in
          switch segment {
          case .text(let text): inline(text, size: size).lineSpacing(lineSpacing)
          case .image(let image): MarkdownImageView(image: image, baseURL: baseURL)
          }
        }
      }
    }
  }

  @ViewBuilder private func markerView(_ marker: MarkdownBlock.Marker) -> some View {
    switch marker {
    case .bullet:
      Text("•").font(font(size, .bold)).foregroundStyle(Color.mrowTertiary)
    case .number(let number):
      Text("\(number).").font(font(size)).foregroundStyle(Color.mrowSecondary).monospacedDigit()
    case .task(let done):
      Image(systemName: done ? "checkmark.square.fill" : "square")
        .font(.system(size: size * 0.85))
        .foregroundStyle(done ? Color.mrowAccent : Color.mrowTertiary)
    }
  }

  private func table(header: [String], rows: [[String]]) -> some View {
    let all = [header] + rows
    return TableLayout(columns: header.count) {
      ForEach(0..<all.count, id: \.self) { row in
        ForEach(0..<header.count, id: \.self) { column in
          cellView(column < all[row].count ? all[row][column] : "", weight: row == 0 ? .semibold : .regular)
            .padding(.horizontal, 10)
            .padding(.vertical, 7)
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
            .background(row == 0 ? Color.mrowWell : Color.clear)
            .overlay(alignment: .top) {
              if row > 0 { Rectangle().fill(Color.mrowLine).frame(height: 1) }
            }
        }
      }
    }
    .overlay(RoundedRectangle(cornerRadius: 8, style: .continuous).strokeBorder(Color.mrowStrongLine))
    .clipShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
  }

  /// A table cell; screenshots side by side in a table are how GitLab descriptions compare states.
  @ViewBuilder private func cellView(_ text: String, weight: NSFont.Weight = .regular) -> some View {
    let segments = MarkdownCache.segments(text, badges: true)
    if segments.count == 1, case .text(let only) = segments[0] {
      inline(only, size: size * 0.9, weight: weight)
    } else {
      VStack(alignment: .leading, spacing: 6) {
        ForEach(Array(segments.enumerated()), id: \.offset) { _, segment in
          switch segment {
          case .text(let text):
            inline(text, size: size * 0.9, weight: weight)
          case .image(let image) where image.isBadge:
            // Status icons (CodeRabbit's checks): the icon, then what it says.
            HStack(spacing: 5) {
              MarkdownImageView(image: image, baseURL: baseURL, badgeHeight: size * 0.95)
              if !image.alt.isEmpty { inline(image.alt, size: size * 0.9, weight: weight) }
            }
          case .image(let image):
            MarkdownImageView(image: image, baseURL: baseURL, inTable: true)
          }
        }
      }
    }
  }

  private func headingSize(_ level: Int) -> CGFloat {
    switch level {
    case 1: size * 1.5
    case 2: size * 1.3
    case 3: size * 1.12
    default: size
    }
  }

  private func font(_ size: CGFloat, _ weight: NSFont.Weight = .regular) -> Font {
    serif ? .mrowSerif(size, weight) : .mrowUI(size, weight)
  }

  private func inline(_ markdown: String, size: CGFloat, weight: NSFont.Weight = .regular) -> Text {
    Text(MarkdownCache.inline(markdown, size: size, baseURL: baseURL) { attributed(markdown, size: size) })
      .font(font(size, weight))
      .foregroundColor(.mrowText)
  }

  /// Inline markdown with code in Geist Mono and links in the accent colour.
  private func attributed(_ markdown: String, size: CGFloat) -> AttributedString {
    // Images left in the text (badges, or any in a table) become links, without their {width=…}.
    let source = markdown
      .replacingOccurrences(
        of: "\\[!\\[([^\\]]*)\\]\\([^)]+\\)(?:\\{[^}]*\\})?\\]\\(([^)]+)\\)", with: "[🖼 $1]($2)",
        options: .regularExpression)
      .replacingOccurrences(
        of: "!\\[([^\\]]*)\\]\\(([^)]+)\\)(?:\\{[^}]*\\})?", with: "[🖼 $1]($2)", options: .regularExpression)
      // GitLab serves uploads under the project ("/uploads/…" means "<project>/uploads/…"),
      // so make them relative to the project rather than the host.
      .replacingOccurrences(of: "](/uploads/", with: "](uploads/")
    var string =
      (try? AttributedString(
        markdown: source, options: .init(interpretedSyntax: .inlineOnlyPreservingWhitespace),
        baseURL: baseURL))
      ?? AttributedString(markdown)
    for run in string.runs {
      if run.inlinePresentationIntent?.contains(.code) == true {
        string[run.range].font = .mrowMono(size * 0.86)
        string[run.range].backgroundColor = .mrowWell
      }
      if let link = run.link {
        string[run.range].foregroundColor = .mrowAccent
        if link.scheme == nil, let baseURL { string[run.range].link = URL(string: link.relativeString, relativeTo: baseURL) }
      }
    }
    return string
  }
}

/// Parsed blocks, inline text and coloured code, kept so a redraw (any change in the store
/// redraws the open merge request) doesn't parse or lex anything again.
@MainActor private enum MarkdownCache {
  private static var blocks: [String: [MarkdownBlock]] = [:]
  private static var inline: [InlineKey: AttributedString] = [:]
  private static var code: [String: AttributedString] = [:]
  private static var segments: [String: [MarkdownImage.Segment]] = [:]
  private static var cellSegments: [String: [MarkdownImage.Segment]] = [:]

  private struct InlineKey: Hashable {
    let markdown: String
    let size: CGFloat
    let baseURL: URL?
  }

  static func blocks(_ source: String) -> [MarkdownBlock] {
    if let hit = blocks[source] { return hit }
    let parsed = MarkdownBlock.parse(source)
    if blocks.count > 500 { blocks.removeAll() }
    blocks[source] = parsed
    return parsed
  }

  static func inline(_ markdown: String, size: CGFloat, baseURL: URL?, make: () -> AttributedString) -> AttributedString {
    let key = InlineKey(markdown: markdown, size: size, baseURL: baseURL)
    if let hit = inline[key] { return hit }
    let made = make()
    if inline.count > 5000 { inline.removeAll() }
    inline[key] = made
    return made
  }

  static func segments(_ text: String, badges: Bool = false) -> [MarkdownImage.Segment] {
    // Most text has no pictures; don't run the expression over it.
    guard text.has("![") else { return [.text(text)] }
    if let hit = badges ? cellSegments[text] : segments[text] { return hit }
    let split = MarkdownImage.segments(text, badges: badges)
    if segments.count + cellSegments.count > 2000 { (segments, cellSegments) = ([:], [:]) }
    if badges { cellSegments[text] = split } else { segments[text] = split }
    return split
  }

  static func code(_ text: String, language: String?) -> AttributedString {
    let key = (language ?? "") + "\u{0}" + text
    if let hit = code[key] { return hit }
    let styled = DiffStyler.style(code: text, language: SyntaxHighlighter.language(for: language))
    if code.count > 300 { code.removeAll() }
    code[key] = styled
    return styled
  }
}

/// A fenced code block. Long lines wrap, as in the diffs: a scroll view per block is a real
/// NSScrollView each. A fence headed by a file ("```src/x.ts lines 5–9", as Ask Cursor's citations
/// are) gets a header with the file's name and a copy button; others show copy on hover.
private struct CodeBlock: View {
  let info: String?
  let text: String
  let size: CGFloat
  @State private var hovering = false

  var body: some View {
    let file = Self.file(info)
    VStack(alignment: .leading, spacing: 0) {
      if let file {
        HStack(spacing: 6) {
          Image(systemName: "doc.text").font(.system(size: size * 0.72))
          Text((file.path as NSString).lastPathComponent)
            .font(.mrowMono(size * 0.78, .medium))
            .foregroundStyle(Color.mrowSecondary)
            .lineLimit(1)
            .truncationMode(.middle)
            .help(file.path)
          if !file.caption.isEmpty {
            Text(file.caption).font(.mrowUI(size * 0.78)).lineLimit(1).fixedSize()
          }
          Spacer(minLength: 4)
          CopyButton(title: "Copy Code") { text }
        }
        .foregroundStyle(Color.mrowTertiary)
        .padding(.leading, 12)
        .padding(.trailing, 5)
        .frame(height: 30)
        Rectangle().fill(Color.mrowLine).frame(height: 1)
      }
      Text(MarkdownCache.code(text, language: info))
        .font(.mrowMono(size * 0.8))
        .foregroundStyle(Color.mrowText)
        .lineSpacing(3)
        .padding(12)
        .frame(maxWidth: .infinity, alignment: .leading)
    }
    .background(Color.mrowWell, in: RoundedRectangle(cornerRadius: 8, style: .continuous))
    .overlay(RoundedRectangle(cornerRadius: 8, style: .continuous).strokeBorder(Color.mrowLine))
    .overlay(alignment: .topTrailing) {
      if file == nil {
        CopyButton(title: "Copy Code") { text }
          .background(Color.mrowWell, in: RoundedRectangle(cornerRadius: 6, style: .continuous))
          .padding(5)
          .opacity(hovering ? 1 : 0)
      }
    }
    .onHover { hovering = $0 }
  }

  /// "src/x.ts lines 5–9" → the path and what follows it; nil for a plain language ("ts").
  static func file(_ info: String?) -> (path: String, caption: String)? {
    guard let info, !info.isEmpty else { return nil }
    let parts = info.split(separator: " ", maxSplits: 1).map(String.init)
    let path = parts[0]
    let name = (path as NSString).lastPathComponent
    guard path.contains("/") || (name.contains(".") && !name.hasPrefix(".") && name.first?.isLetter == true) else {
      return nil
    }
    return (path, parts.count > 1 ? parts[1].trimmingCharacters(in: .whitespaces) : "")
  }
}

/// A table laid out the way a browser does it: columns take their natural width when it fits,
/// and otherwise narrow columns keep theirs while the wide ones share the rest and wrap.
/// Subviews come row by row.
private struct TableLayout: Layout {
  let columns: Int

  struct Cache {
    var width: CGFloat?
    var widths: [CGFloat] = []
    var heights: [CGFloat] = []
  }

  func makeCache(subviews: Subviews) -> Cache { Cache() }
  func updateCache(_ cache: inout Cache, subviews: Subviews) { cache = Cache() }

  func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout Cache) -> CGSize {
    measure(proposal.width, subviews, &cache)
    return CGSize(width: cache.widths.reduce(0, +), height: cache.heights.reduce(0, +))
  }

  func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout Cache) {
    measure(bounds.width, subviews, &cache)
    var y = bounds.minY
    for row in cache.heights.indices {
      var x = bounds.minX
      for column in 0..<columns {
        let index = row * columns + column
        guard index < subviews.count else { break }
        subviews[index].place(
          at: CGPoint(x: x, y: y), proposal: ProposedViewSize(width: cache.widths[column], height: cache.heights[row]))
        x += cache.widths[column]
      }
      y += cache.heights[row]
    }
  }

  private func measure(_ width: CGFloat?, _ subviews: Subviews, _ cache: inout Cache) {
    guard columns > 0, cache.width != width || cache.widths.isEmpty else { return }
    cache.width = width
    let rows = (subviews.count + columns - 1) / columns
    var ideal = [CGFloat](repeating: 0, count: columns)
    for (index, subview) in subviews.enumerated() {
      ideal[index % columns] = max(ideal[index % columns], subview.sizeThatFits(.unspecified).width)
    }
    var widths = ideal
    if let width, ideal.reduce(0, +) > width {
      // Narrowest first: each takes its natural width or a fair share of what's left.
      var remaining = width
      let narrowestFirst = ideal.indices.sorted { ideal[$0] < ideal[$1] }
      for (done, column) in narrowestFirst.enumerated() {
        let share = remaining / CGFloat(columns - done)
        widths[column] = min(ideal[column], max(share, 60))
        remaining -= widths[column]
      }
    }
    cache.widths = widths
    cache.heights = (0..<rows).map { row in
      (0..<columns).reduce(0) { tallest, column in
        let index = row * columns + column
        guard index < subviews.count else { return tallest }
        return max(tallest, subviews[index].sizeThatFits(ProposedViewSize(width: widths[column], height: nil)).height)
      }
    }
  }
}

/// A `<details>` section: its summary with a chevron, and the body only once it's opened.
private struct DetailsSection<Content: View>: View {
  let label: Text
  let size: CGFloat
  let content: () -> Content
  @State private var expanded: Bool

  init(label: Text, initiallyOpen: Bool, size: CGFloat, @ViewBuilder content: @escaping () -> Content) {
    self.label = label
    self.size = size
    self.content = content
    _expanded = State(initialValue: initiallyOpen)
  }

  var body: some View {
    VStack(alignment: .leading, spacing: size * 0.6) {
      Button {
        expanded.toggle()
      } label: {
        HStack(alignment: .firstTextBaseline, spacing: 7) {
          Image(systemName: "chevron.right")
            .font(.system(size: size * 0.62, weight: .bold))
            .foregroundStyle(Color.mrowTertiary)
            .rotationEffect(.degrees(expanded ? 90 : 0))
            .frame(width: size * 0.7)
          label
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .contentShape(Rectangle())
      }
      .buttonStyle(.plain)
      .pointerStyle(.link)
      if expanded {
        HStack(spacing: size * 0.75) {
          RoundedRectangle(cornerRadius: 1).fill(Color.mrowLine).frame(width: 2)
          content()
        }
        .padding(.leading, size * 0.3)
      }
    }
  }
}
