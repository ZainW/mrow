import AppKit
import SwiftUI

/// The chat as Claude lays it out: your question in a bubble with what it carried, then the
/// agent's turn, with its reading and thinking folded into one line between stretches of answer.
enum TranscriptItem: Identifiable {
  case question(ChatEntry, context: ChatEntry?)
  /// Thoughts and tool calls in a row, shown as one collapsible line.
  case activity([ChatEntry])
  case answer(ChatEntry)
  case note(ChatEntry)
  case failure(ChatEntry)

  var id: Int {
    switch self {
    case .question(let entry, _), .answer(let entry), .note(let entry), .failure(let entry): entry.id
    case .activity(let steps): steps[0].id
    }
  }

  static func build(_ entries: [ChatEntry]) -> [TranscriptItem] {
    // What the first question carried sits after it while asking, before it in a reopened chat.
    var contexts: [Int: ChatEntry] = [:]
    for (index, entry) in entries.enumerated() where entry.kind == .context {
      if index > 0, entries[index - 1].kind == .question {
        contexts[entries[index - 1].id] = entry
      } else if index + 1 < entries.count, entries[index + 1].kind == .question {
        contexts[entries[index + 1].id] = entry
      }
    }
    var items: [TranscriptItem] = []
    var steps: [ChatEntry] = []
    func flush() {
      if !steps.isEmpty { items.append(.activity(steps)) }
      steps = []
    }
    for entry in entries {
      switch entry.kind {
      case .thought, .tool, .declined: steps.append(entry)
      case .context: continue
      case .question:
        flush()
        items.append(.question(entry, context: contexts[entry.id]))
      case .answer:
        // The blank lines an answer sometimes sends before a tool call aren't worth a block.
        guard !entry.text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { continue }
        flush()
        items.append(.answer(entry))
      case .note:
        flush()
        items.append(.note(entry))
      case .failure:
        flush()
        items.append(.failure(entry))
      }
    }
    flush()
    return items
  }
}

// MARK: Messages

/// Your question as a card across the transcript, with what the first one shared with Cursor
/// as a slim row along its top.
struct QuestionView: View {
  let entry: ChatEntry
  let context: ChatEntry?
  let reference: String

  var body: some View {
    VStack(alignment: .leading, spacing: 0) {
      if let context {
        HStack(spacing: 6) {
          Image(systemName: "arrow.triangle.pull")
            .font(.system(size: 9.5, weight: .semibold))
            .foregroundStyle(Color.mrowAccent)
          Text(reference).font(.mrowMono(11, .medium)).foregroundStyle(Color.mrowSecondary)
          Text(context.text).font(.mrowUI(11)).foregroundStyle(Color.mrowTertiary).lineLimit(1)
        }
        .padding(.horizontal, 12)
        .frame(height: 26)
        .frame(maxWidth: .infinity, alignment: .leading)
        .help("Shared with the first question: \(context.text)")
        Rectangle().fill(Color.mrowLine).frame(height: 1)
      }
      Text(entry.text)
        .font(.mrowUI(13.5))
        .lineSpacing(2)
        .foregroundStyle(Color.mrowText)
        .textSelection(.enabled)
        .fixedSize(horizontal: false, vertical: true)
        .padding(.horizontal, 12)
        .padding(.vertical, 10)
        .frame(maxWidth: .infinity, alignment: .leading)
    }
    .background(Color.mrowWell.opacity(0.55), in: RoundedRectangle(cornerRadius: 10, style: .continuous))
    .overlay(RoundedRectangle(cornerRadius: 10, style: .continuous).strokeBorder(Color.mrowLine))
  }
}

/// An answer in the reading face, as Claude sets its replies. While it streams, the spark works
/// at its foot; once the turn is done, the turn's copy button takes its place.
struct AnswerView: View {
  let source: String
  let baseURL: URL?
  let live: Bool
  /// The whole turn's answer, on the turn's last stretch of it; nil on the others.
  let copyText: String?
  let latest: Bool
  @State private var hovering = false

  var body: some View {
    VStack(alignment: .leading, spacing: 6) {
      MarkdownView(source: source, baseURL: baseURL, size: 14, serif: true)
      if live {
        Spark(size: 14).padding(.top, 2)
      } else if let copyText {
        // Copy keeps the markdown, ready to paste into a GitLab comment.
        CopyButton(title: "Copy Answer") { copyText }
          .padding(.leading, -5)
          .opacity(hovering || latest ? 1 : 0)
      }
    }
    .onHover { hovering = $0 }
  }
}

/// "Stopped", "Cut off at the model's limit".
struct NoteView: View {
  let text: String

  var body: some View {
    Label {
      Text(text)
    } icon: {
      Image(systemName: text == "Stopped" ? "stop.circle" : "exclamationmark.circle")
    }
    .font(.mrowUI(12))
    .foregroundStyle(Color.mrowTertiary)
  }
}

/// A failure on a soft red card, with a way to ask again when it ended the chat.
struct FailureCard: View {
  let text: String
  var retry: (() -> Void)?

  var body: some View {
    VStack(alignment: .leading, spacing: 9) {
      HStack(alignment: .firstTextBaseline, spacing: 8) {
        Image(systemName: "exclamationmark.triangle.fill")
          .font(.system(size: 11.5))
          .foregroundStyle(Color.mrowRed)
        Text(text)
          .font(.mrowUI(12.5))
          .foregroundStyle(Color.mrowText)
          .textSelection(.enabled)
          .fixedSize(horizontal: false, vertical: true)
      }
      if let retry {
        Button(action: retry) {
          Label("Try again", systemImage: "arrow.clockwise")
            .font(.mrowUI(12, .medium))
            .foregroundStyle(Color.mrowText)
            .padding(.horizontal, 10)
            .frame(height: 26)
            .background(Color.mrowElevated, in: RoundedRectangle(cornerRadius: 7, style: .continuous))
            .overlay(RoundedRectangle(cornerRadius: 7, style: .continuous).strokeBorder(Color.mrowLine))
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .padding(.leading, 20)
      }
    }
    .padding(11)
    .frame(maxWidth: .infinity, alignment: .leading)
    .background(Color.mrowRed.opacity(0.07), in: RoundedRectangle(cornerRadius: 10, style: .continuous))
    .overlay(RoundedRectangle(cornerRadius: 10, style: .continuous).strokeBorder(Color.mrowRed.opacity(0.22)))
  }
}

// MARK: Activity

/// The agent's reading and thinking between answers, folded to one line as Claude does: live,
/// it says what's happening now ("Reading x.ts"); done, what happened ("Thought, read 2 files and
/// searched the code"). Opened, it lists each step down a thin rule.
struct ActivityView: View {
  let steps: [ChatEntry]
  let live: Bool
  /// When the turn began, for the live line's timer.
  var since: Date?
  @State private var open = false

  var body: some View {
    VStack(alignment: .leading, spacing: 0) {
      Button {
        withAnimation(Motion.reduced ? nil : .easeOut(duration: 0.15)) { open.toggle() }
      } label: {
        HStack(spacing: 6) {
          if live { Spark(size: 12) }
          Text(live ? liveLabel : summary)
            .font(.mrowUI(12.5))
            .foregroundStyle(Color.mrowSecondary)
            .lineLimit(1)
            .truncationMode(.middle)
            .modifier(Shimmer(active: live))
          if live, let since { Elapsed(since: since) }
          Image(systemName: "chevron.right")
            .font(.system(size: 8.5, weight: .bold))
            .foregroundStyle(Color.mrowTertiary)
            .rotationEffect(.degrees(open ? 90 : 0))
          Spacer(minLength: 0)
        }
        .contentShape(Rectangle())
      }
      .buttonStyle(.plain)
      .help(open ? "Hide the steps" : "Show the steps")
      if open {
        VStack(alignment: .leading, spacing: 0) {
          ForEach(Array(steps.enumerated()), id: \.element.id) { index, step in
            StepRow(step: step, live: live && index == steps.count - 1, last: index == steps.count - 1)
          }
        }
        .padding(.top, 10)
        .transition(.opacity)
      }
    }
  }

  private var liveLabel: String {
    guard let last = steps.last else { return "Thinking" }
    switch last.kind {
    case .tool(let kind, nil):
      let (_, rest) = ToolTitle.split(last.text)
      switch kind {
      case "read": return rest.isEmpty ? "Reading" : "Reading \(rest)"
      case "search": return rest.isEmpty ? "Searching the code" : "Searching \(rest)"
      case "fetch": return "Fetching \(rest)"
      default: return last.text
      }
    default: return "Thinking"
    }
  }

  private var summary: String {
    var reads: [String] = []
    var searches = 0
    var fetches = 0
    var others = 0
    var declined = 0
    var thinking: TimeInterval = 0
    var thought = false
    for step in steps {
      switch step.kind {
      case .thought:
        thought = true
        thinking += step.finished.timeIntervalSince(step.started)
      case .tool(let kind, _):
        switch kind {
        case "read": reads.append(String(ToolTitle.split(step.text).rest.split(separator: " ").first ?? ""))
        case "search": searches += 1
        case "fetch": fetches += 1
        default: others += 1
        }
      case .declined: declined += 1
      default: break
      }
    }
    var parts: [String] = []
    if thought { parts.append(thinking >= 1 ? "thought for \(Self.duration(thinking))" : "thought") }
    if reads.count == 1, let name = reads.first, !name.isEmpty {
      parts.append("read \(name)")
    } else if !reads.isEmpty {
      parts.append("read \(reads.count) files")
    }
    if searches > 0 { parts.append(searches == 1 ? "searched the code" : "searched \(searches) times") }
    if fetches > 0 { parts.append("fetched \(fetches) page\(fetches == 1 ? "" : "s")") }
    if others > 0 { parts.append("used \(others) tool\(others == 1 ? "" : "s")") }
    if declined > 0 { parts.append("declined \(declined) request\(declined == 1 ? "" : "s")") }
    let joined = ListFormatter.localizedString(byJoining: parts)
    return joined.prefix(1).uppercased() + joined.dropFirst()
  }

  static func duration(_ seconds: TimeInterval) -> String {
    let whole = Int(seconds.rounded())
    return whole < 60 ? "\(whole)s" : "\(whole / 60)m \(whole % 60)s"
  }
}

/// One step: its icon on a rule that runs down to the next, and what it was.
private struct StepRow: View {
  let step: ChatEntry
  let live: Bool
  let last: Bool
  @State private var showThought = false

  var body: some View {
    HStack(alignment: .top, spacing: 9) {
      Image(systemName: symbol)
        .font(.system(size: 10.5, weight: .medium))
        .foregroundStyle(failed ? Color.mrowRed : Color.mrowTertiary)
        .frame(width: 16, height: 18)
      content
        .padding(.bottom, last ? 0 : 10)
      Spacer(minLength: 0)
    }
    .background(alignment: .topLeading) {
      if !last {
        Rectangle().fill(Color.mrowLine).frame(width: 1).padding(.top, 20).padding(.leading, 7.5)
      }
    }
  }

  @ViewBuilder private var content: some View {
    switch step.kind {
    case .thought:
      VStack(alignment: .leading, spacing: 4) {
        let seconds = step.finished.timeIntervalSince(step.started)
        Text(live ? "Thinking…" : seconds >= 1 ? "Thought for \(ActivityView.duration(seconds))" : "Thought")
          .font(.mrowUI(12, .medium))
          .foregroundStyle(Color.mrowSecondary)
          .frame(height: 18)
        Text(step.text.trimmingCharacters(in: .whitespacesAndNewlines))
          .font(.mrowUI(12))
          .lineSpacing(2)
          .foregroundStyle(Color.mrowTertiary)
          .textSelection(.enabled)
          .lineLimit(showThought ? nil : 4)
          .fixedSize(horizontal: false, vertical: true)
        if !showThought && step.text.count > 280 {
          Button("Show all") { showThought = true }
            .buttonStyle(.plain)
            .font(.mrowUI(11.5, .medium))
            .foregroundStyle(Color.mrowAccent)
        }
      }
    case .tool(_, let done):
      let (verb, rest) = ToolTitle.split(step.text)
      HStack(alignment: .firstTextBaseline, spacing: 5) {
        (Text(verb).font(.mrowUI(12, .medium)).foregroundColor(.mrowSecondary)
          + Text(rest.isEmpty ? "" : "  " + rest).font(.mrowMono(11)).foregroundColor(.mrowTertiary))
          .lineLimit(2)
          .truncationMode(.middle)
        Spacer(minLength: 4)
        if done == nil { ProgressView().controlSize(.mini) }
      }
      .frame(minHeight: 18)
      .help(step.text)
    case .declined:
      Text("\(step.text): Mrow is read-only")
        .font(.mrowUI(12))
        .foregroundStyle(Color.mrowSecondary)
        .frame(minHeight: 18)
    default:
      EmptyView()
    }
  }

  private var failed: Bool {
    if case .tool(_, false?) = step.kind { return true }
    return step.kind == .declined
  }

  private var symbol: String {
    switch step.kind {
    case .thought: return "lightbulb"
    case .declined: return "hand.raised"
    case .tool(let kind, _):
      switch kind {
      case "read": return "doc.text"
      case "search": return "magnifyingglass"
      case "fetch": return "globe"
      case "execute": return "terminal"
      case "edit", "delete", "move": return "pencil"
      default: return "wrench.adjustable"
      }
    default: return "circle"
    }
  }
}

/// "Read src/a/b/x.ts (419 - 463)" → ("Read", "x.ts (419 - 463)"): the verb, then the rest with
/// paths cut to their file names (the whole title is the tooltip).
enum ToolTitle {
  static func split(_ title: String) -> (verb: String, rest: String) {
    let words = title.split(separator: " ", maxSplits: 1).map(String.init)
    guard let first = words.first, first.allSatisfy(\.isLetter) else { return ("", shorten(title)) }
    let verb = first.prefix(1).uppercased() + first.dropFirst()
    return (verb, words.count > 1 ? shorten(words[1]) : "")
  }

  private static func shorten(_ text: String) -> String {
    text.split(separator: " ", omittingEmptySubsequences: false).map { word in
      word.contains("/") && !word.hasPrefix("\"") ? (String(word) as NSString).lastPathComponent : String(word)
    }.joined(separator: " ")
  }
}

// MARK: Working

/// Claude Code's working glyph: an asterisk that blooms and folds while the agent works, still
/// with Reduce Motion on.
struct Spark: View {
  var animating = true
  var size: CGFloat = 13
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  private static let frames = ["·", "✢", "✳", "✶", "✻", "✽", "✻", "✶", "✳", "✢"]

  var body: some View {
    if animating && !reduceMotion {
      TimelineView(.periodic(from: .now, by: 0.12)) { context in
        let index = Int(context.date.timeIntervalSinceReferenceDate / 0.12) % Self.frames.count
        glyph(Self.frames[index])
      }
    } else {
      glyph("✻")
    }
  }

  private func glyph(_ character: String) -> some View {
    Text(character)
      .font(.system(size: size, weight: .semibold))
      .foregroundStyle(Color.mrowAccent)
      // Every frame in the same box, so the text beside it doesn't jiggle.
      .frame(width: size + 2, height: size + 2)
      .accessibilityHidden(true)
  }
}

/// A sheen that sweeps across text while something is under way.
struct Shimmer: ViewModifier {
  var active: Bool
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  func body(content: Content) -> some View {
    if active && !reduceMotion {
      content.overlay {
        TimelineView(.animation(minimumInterval: 1 / 30)) { context in
          let phase = context.date.timeIntervalSinceReferenceDate.truncatingRemainder(dividingBy: 1.8) / 1.8
          GeometryReader { geometry in
            let width = max(36, geometry.size.width * 0.45)
            LinearGradient(
              colors: [.clear, Color.mrowText.opacity(0.7), .clear], startPoint: .leading, endPoint: .trailing
            )
            .frame(width: width)
            .offset(x: -width + (geometry.size.width + width) * phase)
          }
        }
        .mask(content)
        .allowsHitTesting(false)
      }
    } else {
      content
    }
  }
}

/// "✻ Pondering… 12s · esc to stop", as Claude Code shows it until the first word or step.
struct WorkingLine: View {
  let label: String
  let since: Date?
  var stoppable = true

  var body: some View {
    HStack(spacing: 7) {
      Spark()
      Text(label + "…")
        .font(.mrowUI(12.5, .medium))
        .foregroundStyle(Color.mrowSecondary)
        .modifier(Shimmer(active: true))
      if let since { Elapsed(since: since, stoppable: stoppable) }
    }
    .accessibilityElement(children: .combine)
  }

  /// One of Claude Code's verbs, the same for the whole of a turn.
  static func verb(for turn: Date?) -> String {
    let verbs = ["Thinking", "Pondering", "Considering", "Mulling", "Reviewing", "Cogitating", "Deliberating", "Puzzling"]
    guard let turn else { return verbs[0] }
    return verbs[Int(turn.timeIntervalSinceReferenceDate) % verbs.count]
  }
}

/// "12s · esc to stop", counting up from the turn's start.
struct Elapsed: View {
  let since: Date
  var stoppable = true

  var body: some View {
    TimelineView(.periodic(from: since, by: 1)) { context in
      let seconds = max(0, Int(context.date.timeIntervalSince(since)))
      Text(seconds < 1 ? "" : ActivityView.duration(TimeInterval(seconds)) + (stoppable ? " · esc to stop" : ""))
        .font(.mrowUI(11.5))
        .monospacedDigit()
        .foregroundStyle(Color.mrowTertiary)
        .lineLimit(1)
        .fixedSize()
    }
  }
}

// MARK: Layout

/// Chips in rows, wrapping, each row centred.
struct CenteredFlow: Layout {
  var spacing: CGFloat = 6

  func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
    let rows = rows(width: proposal.width ?? .infinity, subviews: subviews)
    let height = rows.map(\.height).reduce(0, +) + spacing * CGFloat(max(0, rows.count - 1))
    return CGSize(width: proposal.width ?? rows.map(\.width).max() ?? 0, height: height)
  }

  func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
    var y = bounds.minY
    for row in rows(width: bounds.width, subviews: subviews) {
      var x = bounds.minX + (bounds.width - row.width) / 2
      for index in row.indices {
        let size = subviews[index].sizeThatFits(.unspecified)
        subviews[index].place(at: CGPoint(x: x, y: y), proposal: ProposedViewSize(size))
        x += size.width + spacing
      }
      y += row.height + spacing
    }
  }

  private struct Row {
    var indices: [Int] = []
    var width: CGFloat = 0
    var height: CGFloat = 0
  }

  private func rows(width: CGFloat, subviews: Subviews) -> [Row] {
    var rows: [Row] = []
    var row = Row()
    for index in subviews.indices {
      let size = subviews[index].sizeThatFits(.unspecified)
      if !row.indices.isEmpty, row.width + spacing + size.width > width {
        rows.append(row)
        row = Row()
      }
      row.width += (row.indices.isEmpty ? 0 : spacing) + size.width
      row.height = max(row.height, size.height)
      row.indices.append(index)
    }
    if !row.indices.isEmpty { rows.append(row) }
    return rows
  }
}
