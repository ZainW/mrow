import AppKit
import SwiftUI

/// "Ask Cursor": a chat about the merge request beside it, with Cursor's agent in read-only ask
/// mode, reading the user's own clone. One chat per merge request, kept while switching tabs.
/// Laid out after Claude's: a greeting and the composer in the middle until the first question,
/// then the transcript with the composer at the foot.
struct AskPanel: View {
  let item: MergeRequest
  @ObservedObject var chat: Chat
  var close: () -> Void

  @EnvironmentObject private var store: ReviewStore
  @EnvironmentObject private var window: WindowState
  @ObservedObject private var ask = AskStore.shared
  @FocusState private var composerFocused: Bool
  /// Where each question sits in the transcript, and where the transcript ends, for the room
  /// under the latest turn.
  @State private var questionTops: [Int: CGFloat] = [:]
  @State private var contentEnd: CGFloat = 0
  @State private var viewport: CGFloat = 0
  /// Whether there's more of the chat below what's shown, for the jump-down button.
  @State private var laterBelow = false
  @Environment(\.isSnapshot) private var isSnapshot

  private static let suggestions: [(label: String, symbol: String, prompt: String)] = [
    ("Summarize", "text.alignleft", "Summarize what this changes and why"),
    ("What could break?", "exclamationmark.triangle", "What could break, and where should I look first?"),
    ("Missing tests", "checklist", "Are any tests missing for this change?"),
    ("Trickiest part", "puzzlepiece", "Explain the trickiest part of the diff"),
  ]

  var body: some View {
    VStack(spacing: 0) {
      header
      Rectangle().fill(Color.mrowLine).frame(height: 1)
      if isEmpty {
        Spacer(minLength: 12)
        greeting
      } else {
        transcript
      }
      // One composer in one place in the stack, so it keeps focus and its draft when the first
      // question moves it from the middle to the foot.
      composer
        .padding(.horizontal, 12)
        .padding(.bottom, isEmpty ? 0 : 12)
      if isEmpty {
        chips
        Spacer(minLength: 12)
      }
    }
    .task { await ask.loadModels() }
    .task(id: item.id) { await ask.prepare(chat, for: item) }
    .onAppear { takeFocusIfAsked() }
    .onChange(of: window.askFocusPending) { _, pending in if pending { takeFocusIfAsked() } }
    .onChange(of: composerFocused) { _, focused in window.askFocused = focused }
    .onDisappear { window.askFocused = false }
  }

  private var isEmpty: Bool { chat.entries.isEmpty && chat.phase == .idle }

  private func takeFocusIfAsked() {
    guard window.askFocusPending else { return }
    window.askFocusPending = false
    composerFocused = true
  }

  // MARK: Header

  private var header: some View {
    HStack(spacing: 6) {
      Image(systemName: "bubble.left.and.text.bubble.right")
        .font(.system(size: 12, weight: .medium))
        .foregroundStyle(Color.mrowAccent)
      Text("Ask Cursor")
        .font(.mrowUI(13, .semibold))
        .foregroundStyle(Color.mrowText)
      if !isEmpty {
        Text("\(item.kind.referenceMark)\(item.iid)")
          .font(.mrowMono(11.5))
          .foregroundStyle(Color.mrowTertiary)
          .help(item.title)
      }
      Spacer(minLength: 8)
      ChromeButton(symbol: "square.and.pencil", title: "New Chat", hint: "⇧⌘L", symbolSize: 12.5) {
        ask.newChat(chat, item: item)
        composerFocused = true
      }
      .disabled(chat.entries.isEmpty)
      ChromeButton(symbol: "xmark", title: "Close", hint: "⌘L", symbolSize: 11.5, action: close)
    }
    .padding(.leading, 14)
    .padding(.trailing, 8)
    .frame(height: 40)
  }

  // MARK: Empty

  private var greeting: some View {
    VStack(spacing: 8) {
      HStack(spacing: 8) {
        Spark(animating: false, size: 22)
        Text("Ask about !\(item.iid)")
          .font(.mrowSerif(23, .semibold))
          .foregroundStyle(Color.mrowText)
      }
      Text("Cursor reads the description, the diff, the threads and your clone. It's read-only: nothing is edited or posted.")
        .font(.mrowUI(12.5))
        .lineSpacing(2)
        .foregroundStyle(Color.mrowSecondary)
        .multilineTextAlignment(.center)
        .fixedSize(horizontal: false, vertical: true)
    }
    .padding(.horizontal, 28)
    .padding(.bottom, 18)
    .frame(maxWidth: .infinity)
  }

  private var chips: some View {
    CenteredFlow(spacing: 6) {
      ForEach(Self.suggestions, id: \.label) { suggestion in
        SuggestionChip(label: suggestion.label, symbol: suggestion.symbol, prompt: suggestion.prompt) {
          send(suggestion.prompt)
        }
      }
    }
    .padding(.horizontal, 16)
    .padding(.top, 12)
  }

  // MARK: Transcript

  private var transcript: some View {
    let items = TranscriptItem.build(chat.entries)
    let rows = Array(items.enumerated())
    let latestQuestion = items.last { if case .question = $0 { true } else { false } }?.id
    // Room under the latest turn, so its question can sit at the top while the answer comes in
    // below it, as Claude does: nothing you've read moves as the answer grows.
    let turnHeight = latestQuestion.flatMap { questionTops[$0] }.map { contentEnd - $0 }
    let room = max(0, viewport - Self.transcriptPadding.top - Self.transcriptPadding.bottom - (turnHeight ?? 0))
    return ScrollViewReader { proxy in
      ScrollView {
        VStack(alignment: .leading, spacing: 16) {
          ForEach(rows, id: \.element.id) { index, item in
            row(item, at: index, in: items)
              .id(item.id)
              .onGeometryChange(for: CGFloat.self) { $0.frame(in: .named(Self.transcriptSpace)).minY } action: { top in
                if case .question = item { questionTops[item.id] = top }
              }
          }
          if showsWorking(items) {
            WorkingLine(
              label: chat.reopening ? "Opening the earlier chat"
                : chat.phase == .starting ? "Starting Cursor" : WorkingLine.verb(for: chat.turnStarted),
              since: chat.reopening ? nil : chat.turnStarted, stoppable: chat.phase == .working)
          }
          Color.clear.frame(height: 0).id(Self.endID)
            .onGeometryChange(for: CGFloat.self) { $0.frame(in: .named(Self.transcriptSpace)).minY } action: { end in
              contentEnd = end
            }
        }
        .padding(.bottom, room)
        .coordinateSpace(.named(Self.transcriptSpace))
        .padding(.horizontal, 16)
        .padding(.top, Self.transcriptPadding.top)
        .padding(.bottom, Self.transcriptPadding.bottom)
      }
      // A fresh scroll position (and folded steps) for each merge request's chat.
      .id(chat.id)
      .defaultScrollAnchor(.bottom, for: .initialOffset)
      .scrollDisabled(isSnapshot)
      .onScrollGeometryChange(for: CGSize.self) { geometry in
        CGSize(width: geometry.contentOffset.y, height: geometry.containerSize.height)
      } action: { _, scroll in
        viewport = scroll.height
        // More of the chat below what's shown, not counting the room under the latest turn.
        laterBelow = contentEnd + Self.transcriptPadding.top > scroll.width + scroll.height + 24
      }
      .onChange(of: latestQuestion) { old, question in
        // Asking something brings it to the top; a chat you open shows its end.
        guard let question, !chat.reopening, old != nil || chat.phase != .idle else { return }
        Task { @MainActor in
          withAnimation(Motion.reduced ? nil : .easeOut(duration: 0.25)) { proxy.scrollTo(question, anchor: .top) }
        }
      }
      .onChange(of: chat.id) { _, _ in questionTops = [:] }
      .overlay(alignment: .bottom) {
        if laterBelow {
          Button {
            withAnimation(Motion.reduced ? nil : .easeOut(duration: 0.2)) { proxy.scrollTo(Self.endID, anchor: .bottom) }
          } label: {
            Image(systemName: "arrow.down")
              .font(.system(size: 12, weight: .semibold))
              .foregroundStyle(Color.mrowSecondary)
              .frame(width: 30, height: 30)
              .background(Color.mrowElevated, in: Circle())
              .overlay(Circle().strokeBorder(Color.mrowLine))
              .shadow(color: .black.opacity(0.1), radius: 6, y: 2)
              .contentShape(Circle())
          }
          .buttonStyle(.plain)
          .help(chat.phase == .idle ? "Scroll to the latest" : "Scroll to the answer so far")
          .padding(.bottom, 8)
          .transition(.opacity)
        }
      }
      .animation(.easeOut(duration: 0.15), value: laterBelow)
    }
  }

  private static let transcriptSpace = "transcript"
  private static let transcriptPadding = (top: CGFloat(16), bottom: CGFloat(8))
  private static let endID = "end"

  @ViewBuilder private func row(_ item: TranscriptItem, at index: Int, in items: [TranscriptItem]) -> some View {
    let last = index == items.count - 1
    let working = chat.phase != .idle
    switch item {
    case .question(let entry, let context):
      QuestionView(entry: entry, context: context, reference: "\(self.item.kind.referenceMark)\(self.item.iid)")
    case .activity(let steps):
      ActivityView(steps: steps, live: working && last, since: chat.turnStarted)
    case .answer(let entry):
      AnswerView(
        source: Citations.expand(entry.text, checkout: chat.checkout), baseURL: projectURL, live: working && last,
        copyText: endsTurn(at: index, in: items) ? turnAnswer(endingAt: index, in: items) : nil,
        latest: last && !working)
    case .note(let entry):
      NoteView(text: entry.text)
    case .failure(let entry):
      FailureCard(text: entry.text, retry: last && !working ? retry : nil)
    }
  }

  /// The working line until there's something live of the turn's own to watch: a step, or words.
  private func showsWorking(_ items: [TranscriptItem]) -> Bool {
    guard chat.phase != .idle else { return false }
    switch items.last {
    case .activity?, .answer?: return false
    default: return true
    }
  }

  /// Whether this answer is the turn's last: nothing but its steps comes before the next question.
  private func endsTurn(at index: Int, in items: [TranscriptItem]) -> Bool {
    for later in items[(index + 1)...] {
      switch later {
      case .answer: return false
      case .question: return true
      default: continue
      }
    }
    return chat.phase == .idle
  }

  /// Every stretch of answer since the turn's question, as one markdown text.
  private func turnAnswer(endingAt index: Int, in items: [TranscriptItem]) -> String {
    var parts: [String] = []
    for earlier in items[...index].reversed() {
      if case .question = earlier { break }
      if case .answer(let entry) = earlier { parts.insert(Citations.expand(entry.text, checkout: chat.checkout), at: 0) }
    }
    return parts.map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }.joined(separator: "\n\n")
  }

  /// Asks the last question again, after a failure.
  private func retry() {
    if let question = chat.entries.last(where: { $0.kind == .question })?.text { send(question) }
  }

  // MARK: Composer

  private var composer: some View {
    VStack(alignment: .leading, spacing: 0) {
      TextField(placeholder, text: $chat.draft, axis: .vertical)
        .textFieldStyle(.plain)
        .font(.mrowUI(13.5))
        .lineLimit(1...10)
        .focused($composerFocused)
        .onSubmit { send(chat.draft) }
        .onKeyPress(.escape) {
          guard chat.phase == .working else { return .ignored }
          ask.stop(chat)
          return .handled
        }
        .padding(.horizontal, 14)
        .padding(.top, 12)
        .padding(.bottom, 8)
        // The space around the words focuses the field too, rather than being a dead zone; behind
        // the field, so clicks in the text still place the caret.
        .background {
          Color.clear.contentShape(Rectangle()).onTapGesture { composerFocused = true }
        }
      HStack(spacing: 2) {
        cloneButton
        Spacer(minLength: 6)
        ModelButton(chat: chat, open: $window.modelPickerOpen)
        sendButton.padding(.leading, 4)
      }
      .padding(.horizontal, 8)
      .padding(.bottom, 8)
    }
    .background(Color.mrowElevated, in: RoundedRectangle(cornerRadius: 16, style: .continuous))
    .overlay(
      RoundedRectangle(cornerRadius: 16, style: .continuous)
        .strokeBorder(composerFocused ? Color.mrowAccent.opacity(0.45) : Color.mrowStrongLine)
    )
    .shadow(color: .black.opacity(0.05), radius: 8, y: 2)
  }

  private var placeholder: String {
    chat.entries.contains { $0.kind == .question } ? "Reply…" : "How can I help with !\(item.iid)?"
  }

  private var sendButton: some View {
    let working = chat.phase == .working
    let active = sendEnabled || working
    return Button {
      if working { ask.stop(chat) } else { send(chat.draft) }
    } label: {
      Image(systemName: working ? "stop.fill" : "arrow.up")
        .font(.system(size: working ? 10 : 12.5, weight: .bold))
        .foregroundStyle(Color.white)
        .frame(width: 28, height: 28)
        .background(Color.mrowAccent.opacity(active ? 1 : 0.35), in: RoundedRectangle(cornerRadius: 8, style: .continuous))
        .contentShape(Rectangle())
    }
    .buttonStyle(.plain)
    .disabled(!active)
    .help(working ? "Stop  esc" : "Send  ↩    New line  ⌥↩")
    .accessibilityLabel(working ? "Stop" : "Send")
  }

  private var sendEnabled: Bool {
    chat.phase == .idle && !chat.draft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
  }

  /// Which clone the agent reads, as a chip in the composer: click it to pick another.
  private var cloneButton: some View {
    ComposerChip(
      symbol: chat.checkout == nil ? "folder.badge.questionmark" : "folder",
      title: cloneTitle,
      tint: chat.checkout == nil && chat.checkoutLooked ? .mrowAmber : .mrowSecondary
    ) {
      chooseFolder()
    }
    .help(cloneHelp)
  }

  private var cloneTitle: String {
    if let checkout = chat.checkout { return checkout.lastPathComponent }
    return chat.checkoutLooked ? "No clone" : "Looking…"
  }

  private var cloneHelp: String {
    let read = "Read-only: Cursor can read files, but can't edit them, run commands or post to GitLab or GitHub."
    guard let checkout = chat.checkout else {
      return "No clone of \(item.projectPath) found, so Cursor sees only the merge request. Click to choose one.\n\(read)"
    }
    return "Cursor reads \((checkout.path as NSString).abbreviatingWithTildeInPath) as it is, whatever branch it's on; "
      + "the merge request's own changes come from the diff.\nClick to choose another clone"
      + (chat.entries.isEmpty ? "." : " (starts a new chat).") + "\n\(read)"
  }

  private func send(_ text: String) {
    let question = text
    Task { await ask.ask(question, in: chat, item: item, store: store) }
  }

  private func chooseFolder() {
    let panel = NSOpenPanel()
    panel.canChooseDirectories = true
    panel.canChooseFiles = false
    panel.prompt = "Use This Clone"
    panel.message =
      "Choose your local clone of \(item.projectPath)"
      + (chat.entries.isEmpty ? "" : ". This starts a new chat.")
    panel.directoryURL = chat.checkout ?? FileManager.default.homeDirectoryForCurrentUser
    if panel.runModal() == .OK, let url = panel.url { ask.choose(url, for: chat, item: item) }
  }

  private var projectURL: URL? {
    let string = item.webURL.absoluteString
    guard let range = string.range(of: "/-/merge_requests/") else { return nil }
    return URL(string: String(string[..<range.lowerBound]) + "/")
  }
}

/// A quiet button in the composer's bottom row: an icon and a few words, filled on hover.
struct ComposerChip: View {
  let symbol: String?
  let title: String
  var trailing: String?
  var tint: Color = .mrowSecondary
  var isOn = false
  let action: () -> Void
  @State private var hovering = false

  var body: some View {
    Button(action: action) {
      HStack(spacing: 5) {
        if let symbol { Image(systemName: symbol).font(.system(size: 11, weight: .medium)) }
        Text(title).font(.mrowUI(12, .medium)).lineLimit(1)
        if let trailing {
          Text(trailing).font(.mrowUI(12)).foregroundStyle(Color.mrowTertiary).lineLimit(1)
        }
        Image(systemName: "chevron.down")
          .font(.system(size: 7.5, weight: .bold))
          .foregroundStyle(Color.mrowTertiary)
      }
      .foregroundStyle(tint)
      .padding(.horizontal, 8)
      .frame(height: 28)
      .background(hovering || isOn ? Color.mrowHover : .clear, in: RoundedRectangle(cornerRadius: 8, style: .continuous))
      .contentShape(Rectangle())
    }
    .buttonStyle(.plain)
    .onHover { hovering = $0 }
  }
}

/// A suggested first question as a pill under the composer.
private struct SuggestionChip: View {
  let label: String
  let symbol: String
  let prompt: String
  let action: () -> Void
  @State private var hovering = false

  var body: some View {
    Button(action: action) {
      HStack(spacing: 6) {
        Image(systemName: symbol)
          .font(.system(size: 10.5, weight: .medium))
          .foregroundStyle(hovering ? Color.mrowAccent : Color.mrowTertiary)
        Text(label).font(.mrowUI(12)).foregroundStyle(Color.mrowSecondary)
      }
      .padding(.horizontal, 11)
      .frame(height: 30)
      // The border is a capsule behind a capsule: a stroked one picks up stray end caps.
      .background {
        Capsule().fill(Color.mrowStrongLine)
          .overlay(Capsule().fill(hovering ? Color.mrowHover : Color.mrowPaper).padding(1))
      }
      .contentShape(Capsule())
      .help(prompt)
    }
    .buttonStyle(.plain)
    .focusEffectDisabled()
    .onHover { hovering = $0 }
  }
}
