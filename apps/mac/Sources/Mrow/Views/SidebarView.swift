import SwiftUI

/// The sidebar: the review queue as a flat list on the window's own background, grouped by when
/// I was asked, with Mdow's mode bar and filter on top and Settings at the bottom.
struct SidebarView: View {
  @EnvironmentObject private var store: ReviewStore
  @EnvironmentObject private var prefs: Preferences
  @FocusState.Binding var filterFocused: Bool
  @FocusState private var listFocused: Bool
  @Environment(\.isSnapshot) private var isSnapshot
  var showSettings: () -> Void

  var body: some View {
    VStack(spacing: 0) {
      AccountSwitcher(showSettings: showSettings)
        .padding(.horizontal, 8)
        .padding(.top, 2)
        .padding(.bottom, 6)
      modeBar
        .padding(.horizontal, 8)
      HStack(spacing: 2) {
        filterBox
        authorMenu
        ChromeButton(
          symbol: prefs.oldestFirst ? "arrow.up.to.line" : "arrow.down.to.line",
          title: prefs.oldestFirst ? "Oldest first. Click for newest first" : "Newest first. Click for oldest first",
          isOn: prefs.oldestFirst, symbolSize: 12
        ) {
          prefs.oldestFirst.toggle()
        }
      }
      .padding(.leading, 10)
      .padding(.trailing, 6)
      .padding(.top, 10)
      if let author = selectedAuthor {
        authorChip(author)
          .padding(.horizontal, 10)
          .padding(.top, 8)
      }
      list
      footer
    }
    .background(Color.mrowChrome)
  }

  private var modeBar: some View {
    HStack(spacing: 2) {
      ForEach(QueueMode.allCases) { mode in
        let count = store.count(for: mode)
        ChromeButton(
          symbol: nil, title: count > 0 && mode != .done ? "\(mode.title)  \(count)" : mode.title,
          style: .text, isOn: store.mode == mode
        ) {
          store.setMode(mode)
        }
        .help("\(mode.title)  ⌘\(mode.rawValue + 1)")
      }
      Spacer(minLength: 0)
    }
  }

  private var filterBox: some View {
    HStack(spacing: 7) {
      Image(systemName: "line.3.horizontal.decrease")
        .font(.system(size: 11, weight: .medium))
        .foregroundStyle(Color.mrowTertiary)
      if isSnapshot {
        Text("Filter \(store.forge.pluralNoun)").font(.mrowUI(12.5)).foregroundStyle(Color.mrowTertiary)
        Spacer()
      } else {
        filterField
      }
    }
    .padding(.horizontal, 9)
    .frame(height: 28)
    .background(Color.mrowHover, in: RoundedRectangle(cornerRadius: 7, style: .continuous))
  }

  private var selectedAuthor: Person? {
    guard let username = store.authorFilter else { return nil }
    return (store.open + store.done).first { $0.author.username == username }?.author
      ?? Person(username: username, name: username)
  }

  private var authorMenu: some View {
    Menu {
      Button {
        store.authorFilter = nil
      } label: {
        if store.authorFilter == nil { Label("Everyone", systemImage: "checkmark") } else { Text("Everyone") }
      }
      Divider()
      ForEach(store.authors, id: \.person.username) { entry in
        Button {
          store.authorFilter = entry.person.username
        } label: {
          let title = "\(entry.person.name)  \(entry.count)"
          if store.authorFilter == entry.person.username {
            Label(title, systemImage: "checkmark")
          } else {
            Text(title)
          }
        }
      }
    } label: {
      Image(systemName: store.authorFilter == nil ? "person" : "person.fill")
        .font(.system(size: 12, weight: .medium))
        .foregroundStyle(store.authorFilter == nil ? Color.mrowSecondary : Color.mrowAccent)
        .frame(width: 28, height: 28)
        .contentShape(Rectangle())
    }
    .menuStyle(.button)
    .buttonStyle(.plain)
    .menuIndicator(.hidden)
    .fixedSize()
    .help("Filter by author")
  }

  private func authorChip(_ author: Person) -> some View {
    HStack(spacing: 7) {
      Avatar(person: author, size: 16)
      Text("By \(author.name)")
        .font(.mrowUI(12, .medium))
        .foregroundStyle(Color.mrowText)
        .lineLimit(1)
      Spacer(minLength: 4)
      Button {
        store.authorFilter = nil
      } label: {
        Image(systemName: "xmark").font(.system(size: 9.5, weight: .bold)).foregroundStyle(Color.mrowSecondary)
          .frame(width: 18, height: 18)
          .contentShape(Rectangle())
      }
      .buttonStyle(.plain)
      .help("Show everyone's")
    }
    .padding(.leading, 7)
    .padding(.trailing, 4)
    .frame(height: 26)
    .background(Color.mrowAccentSoft, in: RoundedRectangle(cornerRadius: 7, style: .continuous))
  }

  @ViewBuilder private var filterField: some View {
    TextField(
      "", text: $store.filter,
      prompt: Text("Filter \(store.forge.pluralNoun)").foregroundStyle(Color.mrowTertiary)
    )
    .textFieldStyle(.plain)
    .font(.mrowUI(12.5))
    .foregroundStyle(Color.mrowText)
    .focused($filterFocused)
    .onSubmit { listFocused = true }
    .onKeyPress(.escape) {
      store.filter = ""
      listFocused = true
      return .handled
    }
    .onKeyPress(.downArrow) {
      listFocused = true
      store.select(offset: 1)
      return .handled
    }
    if !store.filter.isEmpty {
      Button {
        store.filter = ""
      } label: {
        Image(systemName: "xmark.circle.fill").foregroundStyle(Color.mrowTertiary)
      }
      .buttonStyle(.plain)
    }
  }

  private var rows: some View {
    LazyVStack(alignment: .leading, spacing: 1, pinnedViews: []) {
      ForEach(store.sections) { section in
        Text(section.title)
          .font(.mrowUI(11.5, .medium))
          .foregroundStyle(Color.mrowTertiary)
          .padding(.horizontal, 18)
          .padding(.top, 14)
          .padding(.bottom, 4)
        ForEach(section.items) { item in
          SidebarRow(item: item, selected: item.id == store.selectedID, unread: store.isUnread(item))
            .id(item.id)
            .onTapGesture {
              // ⌘-click queues it in a tab without leaving what's on screen, as in a browser.
              if NSEvent.modifierFlags.contains(.command) {
                store.openTab(item, activate: false)
              } else {
                store.selectedID = item.id
              }
              listFocused = true
            }
            // Simultaneous, so a single click still selects at once rather than waiting.
            .simultaneousGesture(TapGesture(count: 2).onEnded { store.openTab(item) })
            .contextMenu { contextMenu(for: item) }
        }
      }
    }
    .padding(.bottom, 8)
  }

  @ViewBuilder private var list: some View {
    if isSnapshot {
      // Laid over a flexible box, so a long list is cut at the window instead of growing it.
      Color.clear.overlay(alignment: .top) { rows }.clipped()
    } else {
      scrollingList
    }
  }

  private var scrollingList: some View {
    ScrollViewReader { proxy in
      ScrollView { rows }
      .scrollIndicators(.automatic)
      .withoutEdgeEffect()
      .overlay { emptyState }
      .focusable()
      .focusEffectDisabled()
      .focused($listFocused)
      .onKeyPress(keys: [.upArrow, .downArrow, "j", "k"]) { press in
        store.select(offset: press.key == .upArrow || press.key == "k" ? -1 : 1)
        return .handled
      }
      .onKeyPress(.return) {
        if let item = store.selected { NSWorkspace.shared.open(item.webURL) }
        return .handled
      }
      .onChange(of: store.selectedID) { _, id in
        guard let id else { return }
        withAnimation(.easeOut(duration: 0.12)) { proxy.scrollTo(id) }
      }
    }
  }

  @ViewBuilder private var emptyState: some View {
    if store.sections.isEmpty {
      VStack(spacing: 10) {
        if store.isLoading && store.lastRefreshed == nil {
          ProgressView().controlSize(.small)
          Text("Asking \(store.forge.name)…")
        } else if let error = store.error, store.lastRefreshed == nil {
          Image(systemName: "exclamationmark.triangle").font(.system(size: 18))
          Text(error).multilineTextAlignment(.center)
          ChromeButton(symbol: "arrow.clockwise", title: "Try Again", style: .field) {
            Task { await store.refresh() }
          }
        } else if !store.filter.isEmpty || store.authorFilter != nil {
          Text(store.filter.isEmpty ? "Nothing from this author here." : "Nothing matches “\(store.filter)”")
        } else {
          Image(systemName: store.mode == .toReview ? "checkmark.seal" : "tray")
            .font(.system(size: 20, weight: .light))
          Text(emptyMessage)
        }
      }
      .font(.mrowUI(12.5))
      .foregroundStyle(Color.mrowSecondary)
      .padding(24)
    }
  }

  private var emptyMessage: String {
    switch store.mode {
    case .toReview: "No reviews waiting on you."
    case .reviewed: "Nothing you've reviewed is still open."
    case .done: "Nothing merged or closed in the last \(prefs.doneDays) days."
    }
  }

  private var footer: some View {
    HStack(spacing: 4) {
      ChromeButton(symbol: "gearshape", title: "Settings", style: .text, hint: "⌘,", symbolSize: 13) {
        showSettings()
      }
      Spacer()
      if let last = store.lastRefreshed {
        TimelineView(.periodic(from: .now, by: 30)) { _ in
          Text(store.error != nil ? "Offline" : "Updated \(RelativeTime.short(last))")
            .font(.mrowUI(11))
            .foregroundStyle(store.error != nil ? Color.mrowRed : Color.mrowTertiary)
            .help(store.error ?? last.formatted(date: .omitted, time: .shortened))
        }
      }
      ChromeButton(symbol: "circle.lefthalf.filled", title: "Switch Appearance", symbolSize: 13) {
        let dark = NSApp.effectiveAppearance.bestMatch(from: [.aqua, .darkAqua]) == .darkAqua
        prefs.appearance = dark ? .light : .dark
      }
    }
    .padding(8)
  }

  @ViewBuilder private func contextMenu(for item: MergeRequest) -> some View {
    if store.tabs.contains(item.id) {
      Button("Close Tab") { store.closeTab(item.id) }
    } else {
      Button("Open in New Tab") { store.openTab(item, activate: false) }
    }
    Button("Open in \(item.kind.name)") { NSWorkspace.shared.open(item.webURL) }
    Divider()
    Button(store.authorFilter == item.author.username ? "Show Everyone's" : "Show Only from \(item.author.name)") {
      store.authorFilter = store.authorFilter == item.author.username ? nil : item.author.username
    }
    Divider()
    Button("Copy Link") { copy(item.webURL.absoluteString) }
    Button("Copy Reference") { copy(item.fullReference) }
    Button("Copy Branch Name") { copy(item.sourceBranch) }
  }

  private func copy(_ string: String) {
    NSPasteboard.general.clearContents()
    NSPasteboard.general.setString(string, forType: .string)
  }
}

/// A merge request in the sidebar: a rounded pill when selected, a fainter one under the pointer.
private struct SidebarRow: View {
  var item: MergeRequest
  var selected: Bool
  var unread: Bool

  @State private var hovering = false

  var body: some View {
    HStack(alignment: .top, spacing: 8) {
      Circle()
        .fill(unread ? Color.mrowAccent : .clear)
        .frame(width: 6, height: 6)
        .padding(.top, 6)
      VStack(alignment: .leading, spacing: 4) {
        HStack(alignment: .firstTextBaseline, spacing: 6) {
          Text(item.title)
            .font(.mrowUI(13, unread ? .semibold : .regular))
            .foregroundStyle(item.draft ? Color.mrowSecondary : Color.mrowText)
            .lineLimit(2)
            .frame(maxWidth: .infinity, alignment: .leading)
          Text(RelativeTime.short(item.isOpen ? item.requestedAt : (item.mergedAt ?? item.updatedAt)))
            .font(.mrowUI(11, .medium))
            .foregroundStyle(isStale ? Color.mrowAccent : Color.mrowTertiary)
            .monospacedDigit()
            .help(timeHelp)
        }
        HStack(spacing: 6) {
          Text(item.shortReference)
            .font(.mrowMono(11))
            .foregroundStyle(Color.mrowTertiary)
            .lineLimit(1)
          Text("·").foregroundStyle(Color.mrowTertiary)
          Text(item.author.name)
            .font(.mrowUI(11.5))
            .foregroundStyle(Color.mrowSecondary)
            .lineLimit(1)
          Spacer(minLength: 4)
          badges
        }
      }
    }
    .padding(.leading, 10)
    .padding(.trailing, 12)
    .padding(.vertical, 8)
    .background(
      RoundedRectangle(cornerRadius: 7, style: .continuous)
        .fill(selected ? Color.mrowSelected : hovering ? Color.mrowHover : .clear)
    )
    .padding(.horizontal, 8)
    .contentShape(Rectangle())
    .onHover { hovering = $0 }
  }

  /// Waiting on me for more than two working days.
  private var isStale: Bool {
    item.isOpen && item.myState.needsReview && !item.draft
      && Date().timeIntervalSince(item.requestedAt) > 2 * 86_400
  }

  private var timeHelp: String {
    guard item.isOpen else {
      return "\(item.state.capitalized) \(RelativeTime.long(item.mergedAt ?? item.closedAt ?? item.updatedAt))"
    }
    let asked = item.requestedAtIsGuess ? "Opened" : item.wasReRequested ? "Review re-requested" : "Review requested"
    return "\(asked) \(item.requestedAt.formatted(date: .abbreviated, time: .shortened))"
  }

  @ViewBuilder private var badges: some View {
    HStack(spacing: 5) {
      if item.draft {
        Text("Draft")
          .font(.mrowUI(10.5, .medium))
          .foregroundStyle(Color.mrowSecondary)
          .padding(.horizontal, 5)
          .frame(height: 16)
          .background(Color.mrowHover, in: RoundedRectangle(cornerRadius: 4))
      }
      if item.wasReRequested && item.myState.needsReview {
        Image(systemName: "arrow.clockwise").help("Review re-requested")
      }
      if item.state == "merged" {
        Image(systemName: "arrow.triangle.merge").foregroundStyle(Color.mrowBlue).help("Merged")
      } else if item.state == "closed" {
        Image(systemName: "xmark.circle").help("Closed")
      } else if item.myState != .unreviewed {
        Image(systemName: item.myState.symbol)
          .foregroundStyle(stateColor(item.myState))
          .help(item.myState.label)
      }
      if let pipeline = item.pipeline, item.isOpen {
        PipelineDot(status: pipeline)
      }
    }
    .font(.system(size: 10.5, weight: .semibold))
    .foregroundStyle(Color.mrowTertiary)
  }
}

struct PipelineDot: View {
  var status: PipelineStatus

  var body: some View {
    Circle()
      .fill(color)
      .frame(width: 7, height: 7)
      .help("Pipeline: \(status.label)")
  }

  private var color: Color {
    switch status {
    case .success: .mrowGreen
    case .failed: .mrowRed
    case .running, .pending, .preparing, .created, .waitingForResource, .scheduled, .canceling,
      .waitingForCallback:
      .mrowAmber
    case .canceled, .skipped, .manual: .mrowTertiary
    }
  }
}

func stateColor(_ state: ReviewState) -> Color {
  switch state {
  case .approved: .mrowGreen
  case .requestedChanges: .mrowRed
  case .unapproved: .mrowAmber
  case .reviewed, .reviewStarted: .mrowBlue
  case .unreviewed, .unknown: .mrowTertiary
  }
}
