import SwiftUI

/// One comment thread: its file and line when it's on the code, the notes, and a reply box.
struct ThreadCard: View {
  @EnvironmentObject private var store: ReviewStore
  var item: MergeRequest
  var thread: Discussion
  var baseURL: URL?
  /// The file and line on top; off when the thread already sits under its line.
  var showsPosition = true

  @State private var replying = false

  var body: some View {
    let notes = thread.notes.filter { !$0.system }
    let resolved = notes.first?.resolvable == true && notes.allSatisfy { $0.resolved == true }
    VStack(alignment: .leading, spacing: 0) {
      if showsPosition, let position = notes.first?.position, let path = position.newPath ?? position.oldPath {
        HStack(spacing: 6) {
          Image(systemName: "doc.text").font(.system(size: 10.5))
          Text(path + ((position.newLine ?? position.oldLine).map { ":\($0)" } ?? ""))
            .font(.mrowMono(11))
            .lineLimit(1)
            .truncationMode(.head)
          Spacer()
          if resolved { Chip(text: "Resolved", symbol: "checkmark", tint: .mrowGreen) }
        }
        .foregroundStyle(Color.mrowSecondary)
        .padding(.horizontal, 14)
        .padding(.vertical, 8)
        .background(Color.mrowWell)
        Rectangle().fill(Color.mrowLine).frame(height: 1)
      }
      ForEach(Array(notes.enumerated()), id: \.element.id) { index, note in
        if index > 0 { Rectangle().fill(Color.mrowLine).frame(height: 1).padding(.leading, 46) }
        noteView(note)
      }
      if !thread.individualNote || notes.first?.position != nil {
        Rectangle().fill(Color.mrowLine).frame(height: 1)
        if replying {
          CommentComposer(placeholder: "Reply…", submitTitle: "Reply", compact: true) {
            replying = false
          } submit: { body in
            let posted = await store.reply(on: item, to: thread, body: body)
            if posted { replying = false }
            return posted
          }
          .padding(10)
        } else {
          Button {
            replying = true
          } label: {
            HStack(spacing: 6) {
              Image(systemName: "arrowshape.turn.up.left").font(.system(size: 10.5, weight: .medium))
              Text("Reply").font(.mrowUI(12, .medium))
              Spacer()
              if !showsPosition && resolved { Text("Resolved").font(.mrowUI(11.5)) }
            }
            .foregroundStyle(Color.mrowSecondary)
            .padding(.horizontal, 14)
            .frame(height: 32)
            .contentShape(Rectangle())
          }
          .buttonStyle(HoverRowStyle())
        }
      }
    }
    .background(Color.mrowPaper)
    .clipShape(RoundedRectangle(cornerRadius: 10, style: .continuous))
    .overlay(RoundedRectangle(cornerRadius: 10, style: .continuous).strokeBorder(Color.mrowStrongLine))
    .opacity(resolved ? 0.7 : 1)
  }

  private func noteView(_ note: Note) -> some View {
    HStack(alignment: .top, spacing: 10) {
      Avatar(person: Person(username: note.author.username, name: note.author.name), size: 22)
      VStack(alignment: .leading, spacing: 6) {
        HStack(spacing: 6) {
          Text(note.author.name).font(.mrowUI(12.5, .semibold)).foregroundStyle(Color.mrowText)
          Text(RelativeTime.long(note.createdAt))
            .font(.mrowUI(12))
            .foregroundStyle(Color.mrowTertiary)
            .help(note.createdAt.formatted(date: .abbreviated, time: .shortened))
        }
        MarkdownView(source: note.body, baseURL: baseURL, size: 13.5, serif: false)
      }
    }
    .padding(14)
  }
}

/// A box to write a comment in, with ⌘↩ to post and Esc to cancel.
struct CommentComposer: View {
  @EnvironmentObject private var store: ReviewStore
  var placeholder = "Leave a comment. Markdown works."
  var submitTitle = "Comment"
  var compact = false
  /// Shows a Cancel button and focuses the box on appear, for boxes opened on demand.
  var cancel: (() -> Void)?
  var submit: (String) async -> Bool

  @State private var draft = ""
  @FocusState private var focused: Bool

  var body: some View {
    VStack(alignment: .trailing, spacing: 8) {
      ZStack(alignment: .topLeading) {
        if draft.isEmpty {
          Text(placeholder)
            .font(.mrowUI(13))
            .foregroundStyle(Color.mrowTertiary)
            .padding(.horizontal, 13)
            .padding(.vertical, 10)
            .allowsHitTesting(false)
        }
        TextEditor(text: $draft)
          .font(.mrowUI(13))
          .foregroundStyle(Color.mrowText)
          .scrollContentBackground(.hidden)
          .focused($focused)
          .padding(.horizontal, 8)
          .padding(.vertical, 8)
          .frame(minHeight: compact ? 56 : 80, maxHeight: 220)
          .fixedSize(horizontal: false, vertical: true)
          .onExitCommand { cancel?() }
      }
      .background(Color.mrowWell, in: RoundedRectangle(cornerRadius: 10, style: .continuous))
      .overlay(
        RoundedRectangle(cornerRadius: 10, style: .continuous)
          .strokeBorder(focused ? Color.mrowAccent.opacity(0.6) : Color.mrowLine))
      HStack(spacing: 8) {
        Text("⌘↩ to post").font(.mrowMono(11)).foregroundStyle(Color.mrowTertiary)
        Spacer()
        if let cancel {
          ActionButton(symbol: nil, title: "Cancel", action: cancel)
        }
        ActionButton(symbol: "text.bubble", title: submitTitle, prominent: true, action: post)
          // Only the focused box answers ⌘↩, so several boxes on screen don't fight over it.
          .keyboardShortcut(focused ? KeyboardShortcut(.return, modifiers: .command) : nil)
          .disabled(draft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || store.busy != nil)
      }
    }
    .onAppear {
      if cancel != nil { DispatchQueue.main.async { focused = true } }
    }
  }

  private func post() {
    let body = draft.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !body.isEmpty else { return }
    Task {
      if await submit(body) { draft = "" }
    }
  }
}
