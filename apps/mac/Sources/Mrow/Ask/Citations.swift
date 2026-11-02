import Foundation

/// Cursor cites code as an empty fence headed "```200:231:src/x.ts", which its own app fills in.
/// Mrow fills it from the clone, as the clone is now.
@MainActor enum Citations {
  private static let pattern = try! NSRegularExpression(
    pattern: "^```(\\d+):(\\d+):([^\\n`]+)\\n([\\s\\S]*?)^```[ \\t]*$", options: [.anchorsMatchLines])
  private static var files: [String: [Substring]] = [:]

  static func expand(_ text: String, checkout: URL?) -> String {
    guard text.has("```") else { return text }
    let source = text as NSString
    var result = ""
    var last = 0
    for match in pattern.matches(in: text, range: NSRange(location: 0, length: source.length)) {
      result += source.substring(with: NSRange(location: last, length: match.range.location - last))
      last = match.range.location + match.range.length
      let start = Int(source.substring(with: match.range(at: 1))) ?? 1
      let end = Int(source.substring(with: match.range(at: 2))) ?? start
      var path = source.substring(with: match.range(at: 3)).trimmingCharacters(in: .whitespaces)
      var code = source.substring(with: match.range(at: 4))
      if let checkout, path.hasPrefix(checkout.path + "/") { path.removeFirst(checkout.path.count + 1) }
      if code.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
        code = lines(path, start, end, checkout: checkout).map { $0 + "\n" } ?? ""
      }
      code = dedent(code)
      // The code block's header names the file and lines (and its path says the language);
      // without the code, a line in the text says where it was.
      let lines = "line\(start == end ? " \(start)" : "s \(start)–\(end)")"
      if code.isEmpty {
        result += "`\((path as NSString).lastPathComponent)` · \(lines)\n"
      } else {
        result += "```\(path) \(lines)\n\(code)```"
      }
    }
    result += source.substring(from: last)
    return result
  }

  /// Lines cut from the middle of a file keep its indentation; the block needn't.
  static func dedent(_ code: String) -> String {
    let lines = code.split(separator: "\n", omittingEmptySubsequences: false)
    let indents = lines.filter { !$0.allSatisfy(\.isWhitespace) }.map { $0.prefix { $0 == " " || $0 == "\t" }.count }
    guard let common = indents.min(), common > 0 else { return code }
    return lines.map { $0.allSatisfy(\.isWhitespace) ? "" : String($0.dropFirst(common)) }.joined(separator: "\n")
  }

  private static func lines(_ path: String, _ start: Int, _ end: Int, checkout: URL?) -> String? {
    guard start >= 1, end >= start, end - start < 400 else { return nil }
    let url = path.hasPrefix("/") ? URL(fileURLWithPath: path) : checkout?.appendingPathComponent(path)
    guard let url else { return nil }
    if files[url.path] == nil {
      guard let size = (try? url.resourceValues(forKeys: [.fileSizeKey]))?.fileSize, size < 2_000_000,
        let text = try? String(contentsOf: url, encoding: .utf8)
      else { return nil }
      files[url.path] = text.split(separator: "\n", omittingEmptySubsequences: false)
    }
    guard let all = files[url.path], start <= all.count else { return nil }
    return all[(start - 1)..<min(end, all.count)].joined(separator: "\n")
  }
}
