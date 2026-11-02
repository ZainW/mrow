import SwiftUI

/// Colours a diff the way diffs.com does: each side of the file is lexed as one text, so a block
/// comment or a template string that spans lines stays coloured, then the tokens are cut back
/// into lines. Removed and added lines that pair up get their changed words marked.
/// How one diff line is coloured: runs of UTF-16 offsets with a token kind, a changed-word mark
/// or both. Small and Sendable, so whole files are styled in the background, while the
/// AttributedString is only built for lines that actually come on screen.
struct LineStyle: Equatable, Sendable {
  struct Run: Equatable, Sendable {
    let start: Int
    let end: Int
    let kind: TokenKind?
    let marked: Bool
  }

  let runs: [Run]
}

enum DiffStyler {
  /// The language for a file path, e.g. "src/app.tsx" → "tsx", "Dockerfile" → "dockerfile".
  static func language(for path: String) -> String? {
    SyntaxHighlighter.language(for: (path as NSString).lastPathComponent)
  }

  /// One style per diff line, in the same order. Pure, so it runs off the main thread.
  static func styles(_ lines: [DiffLine], language: String?) -> [LineStyle] {
    let utf16 = lines.map { Array($0.text.utf16) }
    var spans = [[(NSRange, TokenKind)]](repeating: [], count: lines.count)
    // Hunks are lexed one at a time: a gap between them can hide where a comment or string
    // opened, and carrying the lexer across it would flip the rest of the file.
    var hunkStart = 0
    for index in 0...lines.count where index == lines.count || lines[index].kind == .hunk {
      if index > hunkStart { lex(hunkStart..<index, lines, utf16, language, into: &spans) }
      hunkStart = index + 1
    }
    let marks = wordMarks(lines, utf16: utf16)
    return lines.indices.map { index in
      lineStyle(length: utf16[index].count, units: utf16[index], spans: spans[index], marks: marks[index] ?? [])
    }
  }

  /// The diff line as text to draw, with its changed words marked in the line's colour.
  static func attributed(_ line: DiffLine, style: LineStyle) -> AttributedString {
    attributed(line.text, style: style, markColor: line.kind == .added ? .mrowAddedWord : .mrowRemovedWord)
  }

  /// Lexes one hunk: the new side is context + added lines, the old side context + removed, each
  /// as one text so constructs spanning lines stay coloured; the tokens are then cut into lines.
  private static func lex(
    _ range: Range<Int>, _ lines: [DiffLine], _ utf16: [[UInt16]], _ language: String?,
    into spans: inout [[(NSRange, TokenKind)]]
  ) {
    guard language != nil else { return }
    var newText: [UInt16] = []
    var oldText: [UInt16] = []
    var starts: [(index: Int, new: Bool, at: Int)] = []
    for index in range {
      switch lines[index].kind {
      case .context:
        starts.append((index, true, newText.count))
        newText += utf16[index]
        newText.append(10)
        oldText += utf16[index]
        oldText.append(10)
      case .added:
        starts.append((index, true, newText.count))
        newText += utf16[index]
        newText.append(10)
      case .removed:
        starts.append((index, false, oldText.count))
        oldText += utf16[index]
        oldText.append(10)
      case .hunk, .note:
        break
      }
    }
    let newTokens = tokens(newText, language: language)
    let oldTokens = tokens(oldText, language: language)
    var cursorNew = 0
    var cursorOld = 0
    for start in starts {
      let length = utf16[start.index].count
      spans[start.index] =
        start.new
        ? slice(newTokens, start: start.at, length: length, cursor: &cursorNew)
        : slice(oldTokens, start: start.at, length: length, cursor: &cursorOld)
    }
  }

  /// A whole block of code (a fenced block in a description), coloured.
  static func style(code: String, language: String?) -> AttributedString {
    let units = Array(code.utf16)
    var cursor = 0
    let spans = slice(tokens(units, language: language), start: 0, length: units.count, cursor: &cursor)
    let style = lineStyle(length: units.count, units: units, spans: spans, marks: [])
    return attributed(code, style: style, markColor: .clear)
  }

  // MARK: Tokens

  private static func tokens(_ units: [UInt16], language: String?) -> [SyntaxToken] {
    guard language != nil, !units.isEmpty else { return [] }
    return SyntaxHighlighter.tokens(in: String(decoding: units, as: UTF16.self), language: language)
  }

  /// The tokens that fall inside [start, start + length), relative to `start`. Tokens are sorted,
  /// and lines are asked for in order, so a cursor keeps this linear over the whole file.
  private static func slice(
    _ tokens: [SyntaxToken], start: Int, length: Int, cursor: inout Int
  ) -> [(NSRange, TokenKind)] {
    let end = start + length
    while cursor < tokens.count, NSMaxRange(tokens[cursor].range) <= start { cursor += 1 }
    var spans: [(NSRange, TokenKind)] = []
    var index = cursor
    while index < tokens.count, tokens[index].range.location < end {
      let token = tokens[index]
      let from = max(token.range.location, start)
      let to = min(NSMaxRange(token.range), end)
      if to > from { spans.append((NSRange(location: from - start, length: to - from), token.kind)) }
      index += 1
    }
    return spans
  }

  // MARK: Changed words

  /// For each removed line paired with an added line, the UTF-16 ranges that changed.
  private static func wordMarks(_ lines: [DiffLine], utf16: [[UInt16]]) -> [Int: [NSRange]] {
    var marks: [Int: [NSRange]] = [:]
    var index = 0
    while index < lines.count {
      guard lines[index].kind == .removed else {
        index += 1
        continue
      }
      var removed: [Int] = []
      while index < lines.count, lines[index].kind == .removed {
        removed.append(index)
        index += 1
      }
      var added: [Int] = []
      while index < lines.count, lines[index].kind == .added {
        added.append(index)
        index += 1
      }
      for (old, new) in zip(removed, added) {
        guard let (a, b) = changedRanges(utf16[old], utf16[new]) else { continue }
        if !a.isEmpty { marks[old] = a }
        if !b.isEmpty { marks[new] = b }
      }
    }
    return marks
  }

  /// Word-level diff of two lines. Nil when they share too little for marks to help.
  static func changedRanges(_ a: [UInt16], _ b: [UInt16]) -> ([NSRange], [NSRange])? {
    if a == b || a.isEmpty || b.isEmpty || a.count > 1000 || b.count > 1000 { return nil }
    let wa = words(a)
    let wb = words(b)

    // The common prefix and suffix need no table.
    var head = 0
    while head < wa.count, head < wb.count, wa[head].text == wb[head].text { head += 1 }
    var tail = 0
    while tail < wa.count - head, tail < wb.count - head,
      wa[wa.count - 1 - tail].text == wb[wb.count - 1 - tail].text
    {
      tail += 1
    }
    let midA = Array(wa[head..<(wa.count - tail)])
    let midB = Array(wb[head..<(wb.count - tail)])

    var keepA = [Bool](repeating: false, count: midA.count)
    var keepB = [Bool](repeating: false, count: midB.count)
    if !midA.isEmpty, !midB.isEmpty, midA.count * midB.count <= 40_000 {
      // Longest common subsequence of the middle words.
      let n = midA.count
      let m = midB.count
      var table = [Int](repeating: 0, count: (n + 1) * (m + 1))
      for i in stride(from: n - 1, through: 0, by: -1) {
        for j in stride(from: m - 1, through: 0, by: -1) {
          table[i * (m + 1) + j] =
            midA[i].text == midB[j].text
            ? table[(i + 1) * (m + 1) + j + 1] + 1
            : max(table[(i + 1) * (m + 1) + j], table[i * (m + 1) + j + 1])
        }
      }
      var i = 0
      var j = 0
      while i < n, j < m {
        if midA[i].text == midB[j].text {
          keepA[i] = true
          keepB[j] = true
          i += 1
          j += 1
        } else if table[(i + 1) * (m + 1) + j] >= table[i * (m + 1) + j + 1] {
          i += 1
        } else {
          j += 1
        }
      }
    }

    let rangesA = merge(midA, keep: keepA)
    let rangesB = merge(midB, keep: keepB)
    // When the lines share little, marking nearly everything is noise: the line colour says so.
    let keptA = a.count - rangesA.reduce(0) { $0 + $1.length }
    let keptB = b.count - rangesB.reduce(0) { $0 + $1.length }
    if Double(min(keptA, keptB)) < Double(min(a.count, b.count)) * 0.4 { return nil }
    return (rangesA, rangesB)
  }

  private struct Word {
    let text: ArraySlice<UInt16>
    let start: Int
  }

  /// Splits a line into identifier runs, whitespace runs and single punctuation characters.
  private static func words(_ units: [UInt16]) -> [Word] {
    func kind(_ u: UInt16) -> Int {
      if u == 32 || u == 9 { return 1 }
      if (48...57).contains(u) || (65...90).contains(u) || (97...122).contains(u) || u == 95 || u > 127 { return 2 }
      return 0
    }
    var result: [Word] = []
    var index = 0
    while index < units.count {
      let k = kind(units[index])
      var end = index + 1
      if k != 0 { while end < units.count, kind(units[end]) == k { end += 1 } }
      result.append(Word(text: units[index..<end], start: index))
      index = end
    }
    return result
  }

  /// Runs of words not kept by the diff, joined across single spaces so a phrase reads as one.
  private static func merge(_ words: [Word], keep: [Bool]) -> [NSRange] {
    var ranges: [NSRange] = []
    for (index, word) in words.enumerated() where !keep[index] {
      let range = NSRange(location: word.start, length: word.text.count)
      if let last = ranges.last, range.location - NSMaxRange(last) <= 1,
        range.location == NSMaxRange(last) || isSpace(words, before: index)
      {
        ranges[ranges.count - 1] = NSRange(location: last.location, length: NSMaxRange(range) - last.location)
      } else {
        ranges.append(range)
      }
    }
    return ranges
  }

  private static func isSpace(_ words: [Word], before index: Int) -> Bool {
    index > 0 && words[index - 1].text.count == 1 && words[index - 1].text.first == 32
  }

  // MARK: Building text

  private static let colors: [TokenKind: Color] = {
    let kinds: [TokenKind] = [
      .plain, .keyword, .string, .number, .comment, .type, .function, .attribute, .constant, .property, .tag,
      .operator_, .punctuation, .preprocessor, .regex, .heading, .link, .deleted, .inserted,
    ]
    return Dictionary(uniqueKeysWithValues: kinds.map { ($0, Color(nsColor: CodeTheme.color(for: $0))) })
  }()

  /// The runs of a line: each boundary of a token or a changed word starts a new one, and only
  /// runs that are coloured or marked are kept.
  private static func lineStyle(
    length: Int, units: [UInt16], spans: [(NSRange, TokenKind)], marks: [NSRange]
  ) -> LineStyle {
    if spans.isEmpty && marks.isEmpty { return LineStyle(runs: []) }
    var kinds = [TokenKind?](repeating: nil, count: length)
    for (range, kind) in spans where kind != .plain {
      for i in range.location..<min(NSMaxRange(range), length) { kinds[i] = kind }
    }
    var marked = [Bool](repeating: false, count: length)
    for range in marks {
      for i in range.location..<min(NSMaxRange(range), length) { marked[i] = true }
    }
    var runs: [LineStyle.Run] = []
    var start = 0
    while start < length {
      var end = start + 1
      while end < length, kinds[end] == kinds[start], marked[end] == marked[start] { end += 1 }
      // Never split a surrogate pair.
      while end < length, UTF16.isTrailSurrogate(units[end]) { end += 1 }
      if kinds[start] != nil || marked[start] {
        runs.append(LineStyle.Run(start: start, end: end, kind: kinds[start], marked: marked[start]))
      }
      start = end
    }
    return LineStyle(runs: runs)
  }

  /// One string with attributes set on ranges: appending an AttributedString per run cost about
  /// thirty times more than lexing the line did.
  static func attributed(_ text: String, style: LineStyle, markColor: Color) -> AttributedString {
    var result = AttributedString(text)
    guard !style.runs.isEmpty else { return result }
    let utf16 = text.utf16
    var lower = utf16.startIndex
    var offset = 0
    for run in style.runs {
      guard run.end <= utf16.count else { break }
      lower = utf16.index(lower, offsetBy: run.start - offset)
      let upper = utf16.index(lower, offsetBy: run.end - run.start)
      offset = run.end
      defer { lower = upper }
      guard let from = AttributedString.Index(lower, within: result),
        let to = AttributedString.Index(upper, within: result)
      else { continue }
      if let kind = run.kind { result[from..<to].foregroundColor = colors[kind] }
      if run.marked { result[from..<to].backgroundColor = markColor }
    }
    return result
  }
}
