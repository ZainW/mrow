import SwiftUI

/// One line of a unified diff, with its line numbers on each side.
struct DiffLine: Hashable {
  enum Kind { case context, added, removed, hunk, note }

  let kind: Kind
  let text: String
  let oldNumber: Int?
  let newNumber: Int?

  static func parse(_ diff: String) -> [DiffLine] {
    var lines: [DiffLine] = []
    var old = 0
    var new = 0
    // Split on the newline byte: as Characters, "\r\n" is one grapheme and never equals "\n",
    // so a CRLF file would come out as a single line.
    for raw in diff.utf8.split(separator: 10, omittingEmptySubsequences: false) {
      var bytes = Substring(raw)
      if bytes.utf8.last == 13 { bytes = Substring(bytes.utf8.dropLast()) }
      let line = String(bytes)
      // The marker is the first byte, so a combining mark after it can't hide it.
      let marker = line.utf8.first
      if line.hasPrefix("@@") {
        // @@ -343,8 +343,8 @@ context
        let numbers = line.split(separator: " ")
        if numbers.count > 2 {
          old = Int(numbers[1].dropFirst().split(separator: ",").first ?? "") ?? 0
          new = Int(numbers[2].dropFirst().split(separator: ",").first ?? "") ?? 0
        }
        lines.append(DiffLine(kind: .hunk, text: line, oldNumber: nil, newNumber: nil))
      } else if marker == UInt8(ascii: "+") {
        lines.append(DiffLine(kind: .added, text: Self.afterMarker(line), oldNumber: nil, newNumber: new))
        new += 1
      } else if marker == UInt8(ascii: "-") {
        lines.append(DiffLine(kind: .removed, text: Self.afterMarker(line), oldNumber: old, newNumber: nil))
        old += 1
      } else if marker == UInt8(ascii: "\\") {
        lines.append(DiffLine(kind: .note, text: line, oldNumber: nil, newNumber: nil))
      } else if !line.isEmpty || !lines.isEmpty {
        lines.append(DiffLine(kind: .context, text: Self.afterMarker(line), oldNumber: old, newNumber: new))
        old += 1
        new += 1
      }
    }
    // The diff ends with a newline, which leaves one empty context line behind.
    if let last = lines.last, last.kind == .context, last.text.isEmpty { lines.removeLast() }
    return lines
  }

  /// The line without its one-byte "+", "-" or " " marker.
  private static func afterMarker(_ line: String) -> String {
    line.isEmpty ? "" : String(Substring(line.utf8.dropFirst()))
  }
}

/// Line styles per file, worked out in the background and kept for when the file shows again.
@MainActor final class DiffStyles: ObservableObject {
  static let shared = DiffStyles()

  @Published private(set) var styled: [String: [LineStyle]] = [:]
  private var pending: Set<String> = []
  private var order: [String] = []
  private let limit = 400

  nonisolated static func key(_ file: FileDiff) -> String { "\(file.id)|\(file.digest)" }

  /// Styles these files off the main thread, on every core, in file order. Results land in
  /// batches: each publish redraws the changes, so one per file was most of the wait.
  func request(_ files: [FileDiff]) {
    let wanted = files.filter { styled[Self.key($0)] == nil && !pending.contains(Self.key($0)) }
    guard !wanted.isEmpty else { return }
    for file in wanted { pending.insert(Self.key(file)) }
    Task.detached(priority: .userInitiated) {
      let batch = max(4, ProcessInfo.processInfo.activeProcessorCount)
      for start in stride(from: 0, to: wanted.count, by: batch) {
        let slice = Array(wanted[start..<min(start + batch, wanted.count)])
        let results = Self.style(slice)
        await self.store(results)
      }
    }
  }

  /// Styles them right away, for snapshots.
  func styleNow(_ files: [FileDiff]) {
    store(Self.style(files.filter { styled[Self.key($0)] == nil }))
  }

  /// Forgets everything, for timing a cold start.
  func reset() {
    styled = [:]
    order = []
  }

  private nonisolated static func style(_ files: [FileDiff]) -> [(String, [LineStyle])] {
    var done = [[LineStyle]](repeating: [], count: files.count)
    done.withUnsafeMutableBufferPointer { buffer in
      let out = buffer
      DispatchQueue.concurrentPerform(iterations: files.count) { index in
        out[index] = DiffStyler.styles(files[index].lines, language: DiffStyler.language(for: files[index].newPath))
      }
    }
    return zip(files.map(key), done).map { ($0, $1) }
  }

  private func store(_ results: [(String, [LineStyle])]) {
    guard !results.isEmpty else { return }
    var next = styled
    for (key, styles) in results {
      pending.remove(key)
      next[key] = styles
      order.append(key)
    }
    while order.count > limit { next[order.removeFirst()] = nil }
    styled = next
  }
}

/// The merge request's changes: a file list to jump around, then every file's diff. Headers and
/// chunks of lines are rows of one flat lazy stack, so a long diff only builds what's on screen,
/// and the file list can jump to a file that hasn't been built yet.
struct ChangesView: View {
  @EnvironmentObject private var store: ReviewStore
  @ObservedObject private var styles = DiffStyles.shared
  @Environment(\.isSnapshot) private var isSnapshot
  var item: MergeRequest
  var files: [FileDiff]
  var threads: [Discussion]
  var baseURL: URL?
  var scrollTo: (AnyHashable) -> Void

  @State private var collapsed: Set<String> = []
  @State private var expandedLarge: Set<String> = []
  @State private var composing: ComposerTarget?
  /// Files marked viewed start folded, once, when the diff first arrives.
  @State private var foldedViewed = false

  /// The line a new comment is for, with the commits the diff was showing when the box opened,
  /// so a reload in the background can't move the comment to another line or version.
  struct ComposerTarget: Equatable {
    let fileID: String
    let line: DiffLine
    let refs: DiffRefs?

    func isOn(_ line: DiffLine, in file: FileDiff) -> Bool {
      fileID == file.id && self.line.kind == line.kind && self.line.oldNumber == line.oldNumber
        && self.line.newNumber == line.newNumber
    }
  }

  enum RowID: Hashable {
    case header(String)
    case divider(Int), notice(Int), top(Int), footer(Int)
    case chunk(Int, Int)
  }

  private enum Row: Identifiable {
    case header(Int, String), divider(Int), notice(Int, String, expands: Bool), top(Int), footer(Int)
    /// A run of a file's lines, with the threads and comment box that sit under them.
    case chunk(Int, Range<Int>)

    var id: RowID {
      switch self {
      case .header(_, let id): .header(id)
      case .divider(let file): .divider(file)
      case .notice(let file, _, _): .notice(file)
      case .top(let file): .top(file)
      case .footer(let file): .footer(file)
      case .chunk(let file, let lines): .chunk(file, lines.lowerBound)
      }
    }
  }

  /// Files bigger than this start folded, so one lockfile doesn't bury the rest.
  private let largeFile = 400
  /// Lines per lazy row. SwiftUI walks every row of a lazy stack on each scrolled frame, so one
  /// row per line made scrolling cost grow with the diff; chunks keep that walk short.
  private let chunkSize = 24

  var body: some View {
    let placed = PlacedThreads(threads, head: store.diffRefs[item.id]?.headSha)
    let styled = files.map { styles.styled[DiffStyles.key($0)] }
    LazyVStack(alignment: .leading, spacing: 0) {
      fileList.padding(.bottom, 14)
      ForEach(rows(placed)) { row in
        view(for: row, placed: placed, styled: styled)
      }
    }
    .onAppear {
      requestStyles()
      foldViewed()
    }
    .onChange(of: files.map(\.digest)) { _, _ in
      requestStyles()
      foldViewed()
    }
    .onChange(of: expandedLarge) { _, _ in requestStyles() }
  }

  private func foldViewed() {
    guard !foldedViewed, !files.isEmpty else { return }
    foldedViewed = true
    collapsed.formUnion(files.filter { store.viewed($0, in: item) == .yes }.map(\.id))
  }

  private func isLarge(_ file: FileDiff) -> Bool {
    file.lines.count > largeFile || file.generatedFile == true
  }

  private func requestStyles() {
    styles.request(files.filter { !isLarge($0) || expandedLarge.contains($0.id) })
  }

  private func rows(_ placed: PlacedThreads) -> [Row] {
    var rows: [Row] = []
    for (f, file) in files.enumerated() {
      rows.append(.header(f, "file:" + file.id))
      guard !collapsed.contains(file.id) else { continue }
      rows.append(.divider(f))
      if file.tooLarge == true || (file.diff.isEmpty && file.collapsed == true) {
        rows.append(.notice(f, "This diff is too large to show here. Open it in \(item.kind.name).", expands: false))
      } else if file.lines.isEmpty {
        let text = file.renamedFile ? "Renamed without changes." : "No changes to show (binary or mode change)."
        rows.append(.notice(f, text, expands: false))
      } else if isLarge(file) && !expandedLarge.contains(file.id) {
        rows.append(.notice(f, "\(file.lines.count) lines. Show the diff", expands: true))
      } else {
        rows.append(.top(f))
        for start in stride(from: 0, to: file.lines.count, by: chunkSize) {
          rows.append(.chunk(f, start..<min(start + chunkSize, file.lines.count)))
        }
      }
      rows.append(.footer(f))
    }
    return rows
  }

  @ViewBuilder private func view(for row: Row, placed: PlacedThreads, styled: [[LineStyle]?]) -> some View {
    switch row {
    case .header(let f, _):
      fileHeader(files[f], isOpen: !collapsed.contains(files[f].id), threads: placed.count(in: files[f]))
    case .divider:
      Rectangle().fill(Color.mrowLine).frame(height: 1).boxSides()
    case .notice(let f, let text, let expands):
      if expands {
        Button {
          expandedLarge.insert(files[f].id)
        } label: {
          notice(text).foregroundStyle(Color.mrowAccent)
        }
        .buttonStyle(.plain)
      } else {
        notice(text)
      }
    case .top:
      Color.clear.frame(height: 4).boxSides()
    case .footer:
      // The box's bottom edge.
      Color.clear
        .frame(height: 10)
        .background(UnevenRoundedRectangle(bottomLeadingRadius: 10, bottomTrailingRadius: 10).fill(Color.mrowPaper))
        .overlay(CapOutline(edge: .bottom).stroke(Color.mrowStrongLine, lineWidth: 1))
        .padding(.bottom, 14)
    case .chunk(let f, let range):
      VStack(alignment: .leading, spacing: 0) {
        ForEach(range, id: \.self) { l in
          lineRows(f, l, placed: placed, styled: styled[f]?[l])
        }
      }
      .boxSides()
    }
  }

  @ViewBuilder private func lineRows(_ f: Int, _ l: Int, placed: PlacedThreads, styled: LineStyle?) -> some View {
    let file = files[f]
    let line = file.lines[l]
    let commentable = item.isOpen && !isSnapshot && [.context, .added, .removed].contains(line.kind)
    DiffLineRow(line: line, styled: styled, canComment: commentable) {
      composing = ComposerTarget(fileID: file.id, line: line, refs: store.diffRefs[item.id])
    }
    ForEach(placed.threads(on: line, in: file)) { thread in
      ThreadCard(item: item, thread: thread, baseURL: baseURL, showsPosition: false)
        .frame(maxWidth: 760, alignment: .leading)
        .padding(.horizontal, 14)
        .padding(.vertical, 8)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Color.mrowWell)
    }
    if let target = composing, target.isOn(line, in: file) {
      CommentComposer(
        placeholder: "Comment on line \(target.line.newNumber ?? target.line.oldNumber ?? 0). Markdown works.",
        compact: true
      ) {
        composing = nil
      } submit: { body in
        let posted = await store.comment(on: item, file: file, line: target.line, refs: target.refs, body: body)
        if posted { composing = nil }
        return posted
      }
      .frame(maxWidth: 760)
      .padding(.horizontal, 14)
      .padding(.vertical, 10)
      .frame(maxWidth: .infinity, alignment: .leading)
      .background(Color.mrowWell)
    }
  }

  private var fileList: some View {
    let viewed = files.filter { store.viewed($0, in: item) == .yes }.count
    return VStack(alignment: .leading, spacing: 0) {
      HStack(spacing: 8) {
        Text("\(viewed) of \(files.count) viewed")
          .font(.mrowUI(11.5, .medium))
          .foregroundStyle(viewed == files.count && viewed > 0 ? Color.mrowGreen : Color.mrowSecondary)
        Spacer()
        ProgressBar(fraction: files.isEmpty ? 0 : Double(viewed) / Double(files.count))
          .frame(width: 90, height: 4)
      }
      .padding(.horizontal, 12)
      .frame(height: 26)
      ForEach(files) { file in
        let state = store.viewed(file, in: item)
        Button {
          collapsed.remove(file.id)
          scrollTo(RowID.header("file:" + file.id))
        } label: {
          HStack(spacing: 8) {
            Image(systemName: icon(for: file))
              .font(.system(size: 11, weight: .medium))
              .foregroundStyle(iconColor(for: file))
              .frame(width: 14)
            Text(file.newPath)
              .font(.mrowMono(11.5))
              .foregroundStyle(state == .yes ? Color.mrowTertiary : Color.mrowText)
              .lineLimit(1)
              .truncationMode(.head)
            Spacer()
            counts(for: file).opacity(state == .yes ? 0.6 : 1)
            Image(systemName: state == .yes ? "checkmark.circle.fill" : state == .changed ? "exclamationmark.circle.fill" : "circle")
              .font(.system(size: 11.5))
              .foregroundStyle(state == .yes ? Color.mrowGreen : state == .changed ? Color.mrowAmber : Color.mrowLine)
              .help(state == .yes ? "Viewed" : state == .changed ? "Changed since you viewed it" : "Not viewed yet")
          }
          .padding(.horizontal, 12)
          .frame(height: 26)
          .contentShape(Rectangle())
        }
        .buttonStyle(HoverRowStyle())
      }
    }
    .padding(.vertical, 6)
    .background(Color.mrowWell, in: RoundedRectangle(cornerRadius: 10, style: .continuous))
    .overlay(RoundedRectangle(cornerRadius: 10, style: .continuous).strokeBorder(Color.mrowLine))
  }

  private func fileHeader(_ file: FileDiff, isOpen: Bool, threads: Int) -> some View {
    let state = store.viewed(file, in: item)
    return HStack(spacing: 0) {
      Button {
        if isOpen { collapsed.insert(file.id) } else { collapsed.remove(file.id) }
      } label: {
        HStack(spacing: 8) {
          Image(systemName: "chevron.right")
            .font(.system(size: 10, weight: .semibold))
            .rotationEffect(.degrees(isOpen ? 90 : 0))
            .foregroundStyle(Color.mrowTertiary)
          if file.renamedFile && file.oldPath != file.newPath {
            Text(file.oldPath).font(.mrowMono(12)).foregroundStyle(Color.mrowTertiary).lineLimit(1)
            Image(systemName: "arrow.right").font(.system(size: 9)).foregroundStyle(Color.mrowTertiary)
          }
          Text(file.newPath)
            .font(.mrowMono(12, .medium))
            .foregroundStyle(state == .yes ? Color.mrowSecondary : Color.mrowText)
            .lineLimit(1)
            .truncationMode(.head)
          if file.newFile { Chip(text: "New", tint: .mrowGreen) }
          if file.deletedFile { Chip(text: "Deleted", tint: .mrowRed) }
          if file.generatedFile == true { Chip(text: "Generated") }
          Spacer()
          if threads > 0 {
            HStack(spacing: 4) {
              Image(systemName: "text.bubble").font(.system(size: 10.5, weight: .medium))
              Text("\(threads)").font(.mrowMono(11, .medium))
            }
            .foregroundStyle(Color.mrowSecondary)
            .help("\(threads) comment thread\(threads == 1 ? "" : "s") on this file")
          }
          counts(for: file)
        }
        .padding(.leading, 12)
        .padding(.trailing, 10)
        .frame(height: 36)
        .contentShape(Rectangle())
      }
      .buttonStyle(.plain)
      viewedToggle(file, state: state)
        .padding(.trailing, 7)
    }
    .background(
      UnevenRoundedRectangle(
        topLeadingRadius: 10, bottomLeadingRadius: isOpen ? 0 : 10, bottomTrailingRadius: isOpen ? 0 : 10,
        topTrailingRadius: 10
      )
      .fill(Color.mrowPaper)
    )
    .overlay {
      if isOpen {
        CapOutline(edge: .top).stroke(Color.mrowStrongLine, lineWidth: 1)
      } else {
        RoundedRectangle(cornerRadius: 10).strokeBorder(Color.mrowStrongLine)
      }
    }
    .padding(.bottom, isOpen ? 0 : 14)
    .contextMenu {
      Button("Copy Path") {
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(file.newPath, forType: .string)
      }
    }
  }

  /// GitHub's "Viewed" box: checking it folds the file, and a later push to the file unchecks it.
  private func viewedToggle(_ file: FileDiff, state: ReviewStore.Viewed) -> some View {
    Button {
      let on = state != .yes
      store.setViewed(on, file, in: item)
      if on { collapsed.insert(file.id) } else { collapsed.remove(file.id) }
    } label: {
      HStack(spacing: 5) {
        Image(systemName: state == .yes ? "checkmark.square.fill" : "square")
          .font(.system(size: 12.5, weight: .medium))
          .foregroundStyle(state == .yes ? Color.mrowAccent : Color.mrowTertiary)
        Text(state == .changed ? "Changed since viewed" : "Viewed")
          .font(.mrowUI(11.5, .medium))
          .foregroundStyle(state == .changed ? Color.mrowAmber : Color.mrowSecondary)
      }
      .padding(.horizontal, 7)
      .frame(height: 24)
      .background(
        state == .yes ? Color.mrowAccentSoft : Color.clear, in: RoundedRectangle(cornerRadius: 6, style: .continuous)
      )
      .contentShape(Rectangle())
    }
    .buttonStyle(.plain)
    .help(state == .yes ? "Mark as not viewed" : "Mark as viewed and fold the file")
  }

  private func notice(_ text: String) -> some View {
    Text(text)
      .font(.mrowUI(12))
      .foregroundStyle(Color.mrowSecondary)
      .frame(maxWidth: .infinity)
      .padding(.vertical, 14)
      .boxSides()
  }

  private func counts(for file: FileDiff) -> some View {
    HStack(spacing: 6) {
      if file.added > 0 { Text("+\(file.added)").foregroundStyle(Color.mrowGreen) }
      if file.removed > 0 { Text("−\(file.removed)").foregroundStyle(Color.mrowRed) }
    }
    .font(.mrowMono(11, .medium))
  }

  private func icon(for file: FileDiff) -> String {
    if file.newFile { return "plus.square" }
    if file.deletedFile { return "minus.square" }
    if file.renamedFile { return "arrow.right.square" }
    return "square.and.pencil"
  }

  private func iconColor(for file: FileDiff) -> Color {
    if file.newFile { return .mrowGreen }
    if file.deletedFile { return .mrowRed }
    return .mrowTertiary
  }
}

/// Comment threads on the code, filed by the line they're on. Threads left on an older version
/// of the diff stay in the Discussion tab, since their line numbers no longer point here.
private struct PlacedThreads {
  private var byLine: [String: [Discussion]] = [:]
  private var byPath: [String: Int] = [:]

  init(_ threads: [Discussion], head: String?) {
    for thread in threads {
      guard let position = thread.notes.first(where: { !$0.system })?.position else { continue }
      if let head, let at = position.headSha, at != head { continue }
      let key: String
      if let line = position.newLine, let path = position.newPath {
        key = "n|\(path)|\(line)"
      } else if let line = position.oldLine, let path = position.oldPath {
        key = "o|\(path)|\(line)"
      } else {
        continue
      }
      byLine[key, default: []].append(thread)
      byPath[position.newPath ?? position.oldPath ?? "", default: 0] += 1
    }
  }

  /// A comment sits on the new side of added and unchanged lines, and the old side of removed ones.
  func threads(on line: DiffLine, in file: FileDiff) -> [Discussion] {
    guard !byLine.isEmpty else { return [] }
    switch line.kind {
    case .added, .context: return line.newNumber.flatMap { byLine["n|\(file.newPath)|\($0)"] } ?? []
    case .removed: return line.oldNumber.flatMap { byLine["o|\(file.oldPath)|\($0)"] } ?? []
    case .hunk, .note: return []
    }
  }

  func count(in file: FileDiff) -> Int { byPath[file.newPath] ?? 0 }
}

private struct DiffLineRow: View {
  var line: DiffLine
  var styled: LineStyle?
  var canComment: Bool
  var comment: () -> Void

  @State private var hovering = false

  var body: some View {
    HStack(alignment: .top, spacing: 0) {
      // Both line numbers and the sign as one Text: the font is monospaced, so padding lines
      // the columns up, and it's a third of the views per line that separate frames were.
      (Text(numbers).font(.mrowMono(11)).foregroundColor(.mrowTertiary)
        + Text(sign).foregroundColor(signColor))
        .lineLimit(1)
        .frame(width: 96, alignment: .leading)
        .overlay(alignment: .leading) { if hovering { commentButton } }
      // Rows in a lazy stack are offered all the height they need, so the text wraps without
      // `.fixedSize`, which measured every line twice.
      text
        .frame(maxWidth: .infinity, alignment: .leading)
        // Per line: set on the whole stack instead, SwiftUI treats it as one selectable region
        // and scrolling got three times slower.
        .textSelection(.enabled)
    }
    .font(.mrowMono(12))
    .padding(.vertical, line.kind == .hunk ? 5 : 1)
    .padding(.trailing, 12)
    .background(background)
    .onHover { inside in if canComment { hovering = inside } }
  }

  private var commentButton: some View {
    Button(action: comment) {
      Image(systemName: "plus")
        .font(.system(size: 10, weight: .bold))
        .foregroundStyle(Color.mrowPaper)
        .frame(width: 18, height: 18)
        .background(Color.mrowAccent, in: RoundedRectangle(cornerRadius: 5, style: .continuous))
    }
    .buttonStyle(.plain)
    .padding(.leading, 6)
    .help("Comment on this line")
  }

  private var text: Text {
    if line.kind == .hunk || line.kind == .note {
      return Text(line.text).foregroundColor(.mrowTertiary)
    }
    if line.text.isEmpty { return Text(" ") }
    guard let styled else { return Text(line.text).foregroundColor(.mrowText) }
    return Text(DiffStyler.attributed(line, style: styled)).foregroundColor(.mrowText)
  }

  /// "   12    13 " — each number right-aligned in five columns.
  private var numbers: String {
    func column(_ value: Int?) -> String {
      let digits = value.map(String.init) ?? ""
      return String(repeating: " ", count: max(0, 5 - digits.count)) + digits
    }
    return column(line.oldNumber) + " " + column(line.newNumber) + " "
  }

  private var sign: String {
    switch line.kind {
    case .added: "+"
    case .removed: "−"
    default: ""
    }
  }

  private var signColor: Color {
    line.kind == .added ? .mrowGreen : .mrowRed
  }

  private var background: Color {
    switch line.kind {
    case .added: .mrowAddedLine
    case .removed: .mrowRemovedLine
    case .hunk: .mrowWell
    default: .clear
    }
  }
}

/// The two sides of a box, for rows stacked inside one.
private struct SideLines: Shape {
  func path(in rect: CGRect) -> Path {
    var path = Path()
    path.move(to: CGPoint(x: rect.minX + 0.5, y: rect.minY))
    path.addLine(to: CGPoint(x: rect.minX + 0.5, y: rect.maxY))
    path.move(to: CGPoint(x: rect.maxX - 0.5, y: rect.minY))
    path.addLine(to: CGPoint(x: rect.maxX - 0.5, y: rect.maxY))
    return path
  }
}

/// A box's top or bottom: both sides and the rounded edge between them, open at the other end.
private struct CapOutline: Shape {
  enum Edge { case top, bottom }
  var edge: Edge
  var radius: CGFloat = 10

  func path(in rect: CGRect) -> Path {
    let inner = rect.insetBy(dx: 0.5, dy: 0.5)
    let (near, far) = edge == .top ? (inner.minY, rect.maxY) : (inner.maxY, rect.minY)
    var path = Path()
    path.move(to: CGPoint(x: inner.minX, y: far))
    path.addArc(
      tangent1End: CGPoint(x: inner.minX, y: near), tangent2End: CGPoint(x: inner.maxX, y: near), radius: radius)
    path.addArc(
      tangent1End: CGPoint(x: inner.maxX, y: near), tangent2End: CGPoint(x: inner.maxX, y: far), radius: radius)
    path.addLine(to: CGPoint(x: inner.maxX, y: far))
    return path
  }
}

private extension View {
  func boxSides() -> some View {
    overlay(SideLines().stroke(Color.mrowStrongLine, lineWidth: 1))
  }
}

/// A full-width row that picks up the hover colour.
struct HoverRowStyle: ButtonStyle {
  func makeBody(configuration: Configuration) -> some View {
    HoverRow(configuration: configuration)
  }

  private struct HoverRow: View {
    let configuration: Configuration
    @State private var hovering = false

    var body: some View {
      configuration.label
        .background(configuration.isPressed ? Color.mrowSelected : hovering ? Color.mrowHover : .clear)
        .onHover { hovering = $0 }
    }
  }
}
