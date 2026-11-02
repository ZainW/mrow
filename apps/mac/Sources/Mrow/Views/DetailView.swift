import SwiftUI

enum DetailTab: Int, CaseIterable, Identifiable {
  case overview, changes, discussion

  var id: Int { rawValue }

  var title: String {
    switch self {
    case .overview: "Overview"
    case .changes: "Changes"
    case .discussion: "Discussion"
    }
  }
}

/// The selected merge request on Mdow's paper card: what it is, who asked, and the review.
struct DetailView: View {
  @EnvironmentObject private var store: ReviewStore
  var item: MergeRequest
  @Binding var tab: DetailTab
  /// Off only for snapshots, which can't draw a scroll view's content.
  var scrolls = true

  var body: some View {
    Group {
      if scrolls {
        ScrollViewReader { proxy in
          ScrollView {
            page { (id: AnyHashable) in withAnimation(.easeOut(duration: 0.2)) { proxy.scrollTo(id, anchor: .top) } }
          }
          .withoutEdgeEffect()
          .onChange(of: item.id) { _, _ in proxy.scrollTo("top", anchor: .top) }
        }
      } else {
        page { _ in }
      }
    }
    .onAppear { store.loadDetailsIfNeeded(for: item) }
    .onChange(of: item.id) { _, _ in store.loadDetailsIfNeeded(for: item) }
  }

  private func page(scrollTo: @escaping (AnyHashable) -> Void) -> some View {
    VStack(alignment: .leading, spacing: 0) {
      header.id("top")
      tabBar.padding(.top, 26)
      Rectangle().fill(Color.mrowLine).frame(height: 1)
      content(scrollTo: scrollTo).padding(.top, 22)
    }
    .frame(maxWidth: tab == .changes ? 1100 : Theme.readingWidth, alignment: .leading)
    .padding(.horizontal, 44)
    .padding(.top, 34)
    .padding(.bottom, 60)
    .frame(maxWidth: .infinity)
  }

  // MARK: Header

  private var header: some View {
    VStack(alignment: .leading, spacing: 14) {
      HStack(spacing: 8) {
        Text(item.projectPath).font(.mrowMono(11.5)).foregroundStyle(Color.mrowTertiary)
        Text("\(item.kind.referenceMark)\(item.iid)").font(.mrowMono(11.5, .medium)).foregroundStyle(Color.mrowSecondary)
        if item.draft { Chip(text: "Draft") }
        if item.state == "merged" { Chip(text: "Merged", symbol: "arrow.triangle.merge", tint: .mrowBlue) }
        if item.state == "closed" { Chip(text: "Closed", symbol: "xmark", tint: .mrowRed) }
      }

      Text(item.title)
        .font(.mrowSerif(27, .semibold))
        .foregroundStyle(Color.mrowText)
        .lineSpacing(3)
        .textSelection(.enabled)
        .fixedSize(horizontal: false, vertical: true)

      requestLine

      FlowLayout(spacing: 6) {
        Chip(text: "\(item.sourceBranch) → \(item.targetBranch)", symbol: "arrow.branch", mono: true)
          .help("Click to copy the source branch")
          .onTapGesture { copy(item.sourceBranch, "Copied \(item.sourceBranch)") }
        Chip(text: "+\(item.additions)  −\(item.deletions)  ·  \(item.fileCount) files", mono: true)
        if let pipeline = item.pipeline {
          HStack(spacing: 6) {
            PipelineDot(status: pipeline)
            Text("Pipeline \(pipeline.label.lowercased())").font(.mrowUI(11.5, .medium))
          }
          .foregroundStyle(Color.mrowSecondary)
          .padding(.horizontal, 8)
          .frame(height: 22)
          .background(Color.mrowHover, in: RoundedRectangle(cornerRadius: 6, style: .continuous))
        }
        if item.isOpen, let status = item.mergeStatusLabel {
          Chip(text: status, tint: item.mergeStatus == "conflict" ? .mrowRed : .mrowSecondary)
        }
        ForEach(item.labels, id: \.title) { label in
          Chip(
            text: label.title, tint: Color(hex: label.textColor) ?? .mrowText,
            fill: (Color(hex: label.color) ?? .mrowHover).opacity(0.9))
        }
      }

      reviewers
      actions.padding(.top, 4)
    }
  }

  private var requestLine: some View {
    HStack(spacing: 8) {
      Avatar(person: item.requestedBy ?? item.author, size: 22)
      Group {
        if item.requestedAtIsGuess {
          Text("**\(item.author.name)** opened this ") + Text(RelativeTime.long(item.createdAt))
        } else {
          Text("**\((item.requestedBy ?? item.author).name)** ")
            + Text(item.wasReRequested ? "re-requested your review " : "requested your review ")
            + Text(RelativeTime.long(item.requestedAt))
        }
      }
      .font(.mrowUI(13))
      .foregroundStyle(Color.mrowSecondary)
      .help(item.requestedAt.formatted(date: .complete, time: .shortened))
      if item.requestedBy?.username != item.author.username {
        Text("· by \(item.author.name)").font(.mrowUI(13)).foregroundStyle(Color.mrowTertiary)
      }
      if item.wasReRequested {
        Text("· first asked \(item.firstRequestedAt.formatted(.dateTime.month(.abbreviated).day()))")
          .font(.mrowUI(13)).foregroundStyle(Color.mrowTertiary)
      }
    }
  }

  private var reviewers: some View {
    HStack(spacing: 14) {
      Text("Reviewers").font(.mrowUI(11.5, .medium)).foregroundStyle(Color.mrowTertiary)
      ForEach(item.reviewers, id: \.person.username) { reviewer in
        HStack(spacing: 6) {
          Avatar(person: reviewer.person, size: 18)
          Text(reviewer.person.username == store.me?.username ? "You" : reviewer.person.name)
            .font(.mrowUI(12, .medium))
            .foregroundStyle(Color.mrowText)
          Image(systemName: reviewer.state.symbol)
            .font(.system(size: 11, weight: .semibold))
            .foregroundStyle(stateColor(reviewer.state))
        }
        .help(reviewer.state.label)
      }
    }
  }

  private var iApproved: Bool {
    guard let me = store.me?.username else { return false }
    return item.approvedBy.contains { $0.caseInsensitiveCompare(me) == .orderedSame }
  }

  private var actions: some View {
    HStack(spacing: 8) {
      if item.isOpen {
        if iApproved {
          ActionButton(symbol: "arrow.uturn.backward", title: "Revoke Approval") {
            Task { await store.unapprove(item) }
          }
        } else {
          ActionButton(symbol: "checkmark", title: "Approve", prominent: true) {
            Task { await store.approve(item) }
          }
          .keyboardShortcut("a", modifiers: [.command, .shift])
        }
      }
      ActionButton(symbol: "arrow.up.right", title: "Open in \(item.kind.name)") {
        NSWorkspace.shared.open(item.webURL)
      }
      ActionButton(symbol: "doc.on.doc", title: "Copy Branch") {
        copy(item.sourceBranch, "Copied \(item.sourceBranch)")
      }
      if let busy = store.busy {
        ProgressView().controlSize(.small).padding(.leading, 4)
        Text(busy).font(.mrowUI(12)).foregroundStyle(Color.mrowSecondary)
      }
    }
    .disabled(store.busy != nil)
  }

  // MARK: Tabs

  private var tabBar: some View {
    HStack(spacing: 2) {
      ForEach(DetailTab.allCases) { option in
        ChromeButton(symbol: nil, title: title(for: option), style: .text, isOn: tab == option) {
          tab = option
        }
        .help("\(option.title)  ⌥⌘\(option.rawValue + 1)")
      }
      Spacer()
    }
    .padding(.bottom, 8)
  }

  private func title(for tab: DetailTab) -> String {
    switch tab {
    case .overview: return "Overview"
    case .changes: return item.fileCount > 0 ? "Changes  \(item.fileCount)" : "Changes"
    case .discussion:
      let threads = store.discussions[item.id]?.value?.count ?? 0
      return threads > 0 ? "Discussion  \(threads)" : "Discussion"
    }
  }

  @ViewBuilder private func content(scrollTo: @escaping (AnyHashable) -> Void) -> some View {
    switch tab {
    case .overview:
      if item.description.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
        Text("No description.").font(.mrowSerif(15.5)).italic().foregroundStyle(Color.mrowTertiary)
      } else {
        MarkdownView(source: item.description, baseURL: projectURL)
      }
    case .changes:
      loadable(store.diffs[item.id], what: "changes") { files in
        if files.isEmpty {
          Text("No changes.").font(.mrowUI(13)).foregroundStyle(Color.mrowTertiary)
        } else {
          ChangesView(
            item: item, files: files, threads: store.discussions[item.id]?.value ?? [], baseURL: projectURL,
            scrollTo: scrollTo)
        }
      }
    case .discussion:
      loadable(store.discussions[item.id], what: "discussion") { threads in
        DiscussionView(item: item, threads: threads, baseURL: projectURL)
      }
    }
  }

  @ViewBuilder private func loadable<T, Content: View>(
    _ state: Loadable<T>?, what: String, @ViewBuilder content: (T) -> Content
  ) -> some View {
    switch state {
    case .loaded(let value):
      content(value)
    case .failed(let message):
      Label("Couldn't load the \(what): \(message)", systemImage: "exclamationmark.triangle")
        .font(.mrowUI(12.5)).foregroundStyle(Color.mrowRed)
    case .idle, .loading, nil:
      HStack(spacing: 8) {
        ProgressView().controlSize(.small)
        Text("Loading \(what)…").font(.mrowUI(12.5)).foregroundStyle(Color.mrowSecondary)
      }
    }
  }

  /// "https://gitlab.com/group/project/" or "https://github.com/owner/repo/", for relative links
  /// and uploads.
  private var projectURL: URL? {
    let string = item.webURL.absoluteString
    guard let range = string.range(of: "/-/merge_requests/") ?? string.range(of: "/pull/") else { return nil }
    return URL(string: String(string[..<range.lowerBound]) + "/")
  }

  private func copy(_ string: String, _ message: String) {
    NSPasteboard.general.clearContents()
    NSPasteboard.general.setString(string, forType: .string)
    store.show(message)
  }
}

/// Comment threads, oldest first, with a box to add one.
struct DiscussionView: View {
  @EnvironmentObject private var store: ReviewStore
  var item: MergeRequest
  var threads: [Discussion]
  var baseURL: URL?

  var body: some View {
    // Lazy: a busy merge request has dozens of long threads, and only those on screen get built.
    LazyVStack(alignment: .leading, spacing: 16) {
      if threads.isEmpty {
        Text("No comments yet.").font(.mrowUI(13)).foregroundStyle(Color.mrowTertiary)
      }
      ForEach(threads) { thread in
        ThreadCard(item: item, thread: thread, baseURL: baseURL)
      }
      CommentComposer { body in
        await store.comment(on: item, body: body)
      }
      .padding(.top, 8)
    }
  }
}

/// Lays chips out in rows, wrapping when the line is full.
struct FlowLayout: Layout {
  var spacing: CGFloat = 6

  func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
    let rows = arrange(width: proposal.width ?? .infinity, subviews: subviews)
    let height = rows.map(\.height).reduce(0, +) + spacing * CGFloat(max(0, rows.count - 1))
    let width = rows.map(\.width).max() ?? 0
    return CGSize(width: proposal.width ?? width, height: height)
  }

  func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
    var y = bounds.minY
    for row in arrange(width: bounds.width, subviews: subviews) {
      var x = bounds.minX
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

  private func arrange(width: CGFloat, subviews: Subviews) -> [Row] {
    var rows: [Row] = [Row()]
    for (index, subview) in subviews.enumerated() {
      let size = subview.sizeThatFits(.unspecified)
      if !rows[rows.count - 1].indices.isEmpty, rows[rows.count - 1].width + spacing + size.width > width {
        rows.append(Row())
      }
      var row = rows[rows.count - 1]
      row.width += (row.indices.isEmpty ? 0 : spacing) + size.width
      row.height = max(row.height, size.height)
      row.indices.append(index)
      rows[rows.count - 1] = row
    }
    return rows
  }
}
