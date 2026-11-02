import SwiftUI

private struct SnapshotKey: EnvironmentKey {
  static let defaultValue = false
}

extension EnvironmentValues {
  /// Set by `--snapshot`: scroll views become plain stacks, since they can't be drawn offscreen.
  var isSnapshot: Bool {
    get { self[SnapshotKey.self] }
    set { self[SnapshotKey.self] = newValue }
  }
}

/// Window-level state the menus reach into.
@MainActor final class WindowState: ObservableObject {
  @Published var sidebarVisible = true
  @Published var tab: DetailTab = .overview
  @Published var showingSettings = false
  /// The detail tab last shown for each merge request, restored when it's shown again.
  var detailTabs: [String: DetailTab] = [:]
  /// Bumped to move focus to the sidebar's filter.
  @Published var focusFilter = 0
  /// The "Ask Cursor" panel beside the card.
  @Published var askVisible = UserDefaults.standard.bool(forKey: "askVisible") {
    didSet { UserDefaults.standard.set(askVisible, forKey: "askVisible") }
  }
  /// Set to move focus to the chat's composer; the panel clears it once it has.
  @Published var askFocusPending = false
  /// Whether the composer has focus, as the panel reports it.
  var askFocused = false
  @Published var modelPickerOpen = false

  /// The header button: opens the chat (into its composer) or closes it.
  func toggleAsk() {
    if askVisible {
      withAnimation(Motion.panel) { askVisible = false }
    } else {
      withAnimation(Motion.panel) { askVisible = true }
      askFocusPending = true
    }
  }

  /// ⌘L, as in Cursor: opens the chat, or moves into it when it's open but elsewhere has focus,
  /// and closes it from inside.
  func focusOrToggleAsk() {
    if askVisible && !askFocused { askFocusPending = true } else { toggleAsk() }
  }
}

/// The few animations the window uses, with Reduce Motion turning slides into fades.
enum Motion {
  static var reduced: Bool { NSWorkspace.shared.accessibilityDisplayShouldReduceMotion }
  static var panel: Animation { .easeOut(duration: reduced ? 0.12 : 0.18) }

  /// A panel sliding in from its edge, or only fading with Reduce Motion on.
  static func slide(_ edge: Edge) -> AnyTransition {
    reduced ? .opacity : .move(edge: edge).combined(with: .opacity)
  }
}

/// Mdow's window: a header across the top, the sidebar on the window's own colour, and the
/// selected merge request on a rounded sheet of paper.
struct RootView: View {
  @EnvironmentObject private var store: ReviewStore
  @EnvironmentObject private var prefs: Preferences
  @EnvironmentObject private var window: WindowState
  @FocusState private var filterFocused: Bool
  @State private var dragStartWidth: Double?
  @State private var askDragStartWidth: Double?
  @Environment(\.isSnapshot) private var isSnapshot
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  var body: some View {
    VStack(spacing: 0) {
      header
      HStack(spacing: 0) {
        if window.sidebarVisible {
          SidebarView(filterFocused: $filterFocused) { window.showingSettings = true }
            .frame(width: prefs.sidebarWidth)
            .transition(Motion.slide(.leading))
        }
        card
          .overlay(alignment: .leading) { if window.sidebarVisible { resizeHandle } }
          .padding(.leading, window.sidebarVisible ? 0 : Theme.gutter)
          .padding(.trailing, Theme.gutter)
          .padding(.bottom, Theme.gutter)
        if window.askVisible, let item = store.selected {
          // The same panel for every merge request, so the composer keeps focus while you move
          // through the list; each chat's transcript is its own.
          AskPanel(item: item, chat: AskStore.shared.chat(for: item)) { window.toggleAsk() }
            .paperCard()
            .frame(width: prefs.askWidth)
            .overlay(alignment: .leading) { askResizeHandle }
            .padding(.trailing, Theme.gutter)
            .padding(.bottom, Theme.gutter)
            .transition(Motion.slide(.trailing))
        }
      }
    }
    .background(Color.mrowChrome)
    .ignoresSafeArea()
    .overlay(alignment: .top) { toast }
    .sheet(isPresented: $window.showingSettings) {
      SettingsView().environmentObject(store).environmentObject(prefs)
    }
    .onChange(of: window.focusFilter) { _, _ in
      if !window.sidebarVisible { window.sidebarVisible = true }
      DispatchQueue.main.async { filterFocused = true }
    }
    .task { await store.refresh() }
    // Each merge request remembers which of its tabs (overview, changes…) was last open.
    .onChange(of: store.selectedID) { old, new in
      if let old { window.detailTabs[old] = window.tab }
      if let new, let remembered = window.detailTabs[new] { window.tab = remembered }
    }
  }

  // MARK: Header

  private var header: some View {
    HStack(spacing: 2) {
      ChromeButton(symbol: "sidebar.left", title: "Toggle Sidebar", hint: "⌃⌘S") {
        withAnimation(Motion.panel) { window.sidebarVisible.toggle() }
      }
      ChromeButton(symbol: "arrow.clockwise", title: "Refresh", hint: "⌘R", symbolSize: 13) {
        Task { await store.refresh() }
      }
      // With Reduce Motion on, the dimmed (disabled) button alone says it's refreshing.
      .rotationEffect(.degrees(store.isLoading && !reduceMotion ? 360 : 0))
      .animation(
        store.isLoading && !reduceMotion ? .linear(duration: 0.9).repeatForever(autoreverses: false) : .default,
        value: store.isLoading
      )
      .disabled(store.isLoading)

      // Lined up with the card, as Mdow lines its tabs up with the document.
      let cardStart = window.sidebarVisible ? prefs.sidebarWidth : Theme.gutter
      let buttonsEnd = Theme.trafficLightsWidth + 2 * 28 + 2 * 2
      tabStrip
        .padding(.leading, max(10, cardStart - buttonsEnd))
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.trailing, 12)

      ChromeButton(symbol: "magnifyingglass", title: "Filter", style: .field, hint: "⌘K", symbolSize: 12) {
        window.focusFilter += 1
      }
      .frame(width: 200)
      .padding(.trailing, 6)
      ChromeButton(symbol: "arrow.up.right.square", title: "Open in \(store.selected?.kind.name ?? "Browser")", hint: "⌘O") {
        if let item = store.selected { NSWorkspace.shared.open(item.webURL) }
      }
      .disabled(store.selected == nil)
      ChromeButton(
        symbol: "bubble.left.and.text.bubble.right", title: "Ask Cursor", hint: "⌘L", isOn: window.askVisible,
        symbolSize: 13
      ) {
        window.toggleAsk()
      }
      .disabled(store.selected == nil)
      moreMenu
    }
    .padding(.leading, Theme.trafficLightsWidth)
    .padding(.trailing, Theme.gutter)
    .frame(height: Theme.headerHeight)
    .background { if !isSnapshot { WindowDragArea() } }
  }

  /// The queue of tabs, then the previewed merge request (selected but not kept) in italics.
  private var tabStrip: some View {
    let preview = store.selected.flatMap { store.tabs.contains($0.id) ? nil : $0 }
    return HStack(spacing: 3) {
      ForEach(store.tabItems) { item in
        let chip = TabChip(item: item, active: item.id == store.selectedID, preview: false, unread: store.isUnread(item))
        // Dragging needs a real window; an offscreen snapshot draws it as a placeholder.
        if isSnapshot {
          chip
        } else {
          chip
            .draggable(item.id)
            .dropDestination(for: String.self) { ids, _ in
              guard let id = ids.first else { return false }
              withAnimation(.easeOut(duration: 0.15)) { store.moveTab(id, before: item.id) }
              return true
            }
        }
      }
      if let preview {
        TabChip(item: preview, active: true, preview: true, unread: false)
      }
    }
    .animation(.easeOut(duration: 0.15), value: store.tabs)
  }

  private var moreMenu: some View {
    Menu {
      if let item = store.selected {
        Button("Copy Link") { copy(item.webURL.absoluteString) }
        Button("Copy Reference") { copy(item.fullReference) }
        Button("Copy Branch Name") { copy(item.sourceBranch) }
        Divider()
      }
      Picker("Sort", selection: $prefs.oldestFirst) {
        Text("Newest Request First").tag(false)
        Text("Waiting Longest First").tag(true)
      }
      Toggle("Show Drafts", isOn: $prefs.showDrafts)
      Divider()
      Button("Mark All as Read") { store.markAllSeen() }
      Button("Settings…") { window.showingSettings = true }
    } label: {
      Image(systemName: "ellipsis")
        .font(.system(size: 14, weight: .medium))
        .foregroundStyle(Color.mrowSecondary)
        .frame(width: 28, height: 28)
        .contentShape(Rectangle())
    }
    .menuStyle(.button)
    .buttonStyle(.plain)
    .menuIndicator(.hidden)
    .fixedSize()
    .help("More")
  }

  // MARK: Card

  private var card: some View {
    ZStack {
      if let item = store.selected {
        DetailView(item: item, tab: $window.tab, scrolls: !isSnapshot)
          // A fresh view per merge request: a half-written comment, folded files or an open
          // line comment must never carry over to another one.
          .id(item.id)
          // A snapshot shows the top of the page, cut at the card's height.
          .frame(maxHeight: isSnapshot ? 772 : .infinity, alignment: .top)
          .clipped()
      } else {
        placeholder
      }
    }
    .frame(maxWidth: .infinity, maxHeight: .infinity)
    .paperCard()
  }

  private var placeholder: some View {
    VStack(spacing: 10) {
      Text("Mrow")
        .font(.mrowSerif(30, .semibold))
        .foregroundStyle(Color.mrowText)
      Text(store.isLoading ? "Gathering your reviews…" : "Pick a \(store.forge.noun) to review.")
        .font(.mrowUI(13))
        .foregroundStyle(Color.mrowSecondary)
      if let error = store.error {
        Text(error)
          .font(.mrowUI(12))
          .foregroundStyle(Color.mrowRed)
          .multilineTextAlignment(.center)
          .frame(maxWidth: 420)
          .padding(.top, 6)
      }
    }
  }

  private var resizeHandle: some View {
    Color.clear
      .frame(width: 8)
      .contentShape(Rectangle())
      .offset(x: -4)
      .onHover { inside in
        if inside { NSCursor.resizeLeftRight.push() } else { NSCursor.pop() }
      }
      .gesture(
        DragGesture(minimumDistance: 1, coordinateSpace: .global)
          .onChanged { value in
            let start = dragStartWidth ?? prefs.sidebarWidth
            dragStartWidth = start
            prefs.sidebarWidth = min(520, max(240, start + value.translation.width))
          }
          .onEnded { _ in dragStartWidth = nil }
      )
  }

  /// The chat's left edge, in the gutter beside the card; dragging left widens it.
  private var askResizeHandle: some View {
    Color.clear
      .frame(width: 8)
      .contentShape(Rectangle())
      .offset(x: -6)
      .onHover { inside in
        if inside { NSCursor.resizeLeftRight.push() } else { NSCursor.pop() }
      }
      .gesture(
        DragGesture(minimumDistance: 1, coordinateSpace: .global)
          .onChanged { value in
            let start = askDragStartWidth ?? prefs.askWidth
            askDragStartWidth = start
            prefs.askWidth = min(680, max(320, start - value.translation.width))
          }
          .onEnded { _ in askDragStartWidth = nil }
      )
      .help("Drag to resize")
  }

  @ViewBuilder private var toast: some View {
    if let message = store.toast {
      Text(message)
        .font(.mrowUI(12.5, .medium))
        .foregroundStyle(Color.mrowText)
        .padding(.horizontal, 14)
        .frame(height: 30)
        .background(Color.mrowElevated, in: Capsule())
        .overlay(Capsule().strokeBorder(Color.mrowLine))
        .shadow(color: .black.opacity(0.08), radius: 8, y: 2)
        .padding(.top, Theme.headerHeight + 10)
        .transition(.move(edge: .top).combined(with: .opacity))
        .onTapGesture { store.toast = nil }
    }
  }

  private func copy(_ string: String) {
    NSPasteboard.general.clearContents()
    NSPasteboard.general.setString(string, forType: .string)
    store.show("Copied")
  }
}

/// A merge request's tab: Mdow's active-tab look when selected, plain text otherwise. A preview
/// (selected, not kept) is in italics; double-clicking it keeps it.
private struct TabChip: View {
  @EnvironmentObject private var store: ReviewStore
  let item: MergeRequest
  let active: Bool
  let preview: Bool
  let unread: Bool
  @State private var hovering = false

  var body: some View {
    HStack(spacing: 6) {
      ZStack {
        Image(systemName: "arrow.triangle.pull")
          .font(.system(size: 11, weight: .medium))
          .foregroundStyle(active ? Color.mrowSecondary : Color.mrowTertiary)
          .opacity(hovering ? 0 : 1)
        if hovering {
          Image(systemName: "xmark")
            .font(.system(size: 9.5, weight: .bold))
            .foregroundStyle(Color.mrowSecondary)
            .frame(width: 16, height: 16)
            .background(Color.mrowWell, in: RoundedRectangle(cornerRadius: 4, style: .continuous))
            .contentShape(Rectangle())
            .onTapGesture { store.closeTab(item.id) }
            .help(preview ? "Close Preview" : "Close Tab (⌘W)")
        }
      }
      .frame(width: 16)
      // The number, then as much of the title as fits: refs alone all look alike.
      (Text("\(item.kind.referenceMark)\(item.iid)").font(.mrowMono(11.5, .medium))
        .foregroundColor(active ? .mrowText : .mrowSecondary)
        + Text("  " + item.title).font(.mrowUI(12.5))
        .foregroundColor(active ? .mrowSecondary : .mrowTertiary))
        .lineLimit(1)
        .frame(maxWidth: 200, alignment: .leading)
      if unread && !active {
        Circle().fill(Color.mrowAccent).frame(width: 5, height: 5)
      }
    }
    .padding(.leading, 6)
    .padding(.trailing, 10)
    .frame(height: 28)
    .background {
      if active {
        RoundedRectangle(cornerRadius: 8, style: .continuous).fill(Color.mrowElevated)
        RoundedRectangle(cornerRadius: 8, style: .continuous)
          .strokeBorder(preview ? Color.mrowStrongLine : Color.mrowLine, style: StrokeStyle(lineWidth: 1, dash: preview ? [3, 2.5] : []))
      } else if hovering {
        RoundedRectangle(cornerRadius: 8, style: .continuous).fill(Color.mrowWell)
      }
    }
    .contentShape(Rectangle())
    .onHover { hovering = $0 }
    .onTapGesture { store.selectedID = item.id }
    .simultaneousGesture(TapGesture(count: 2).onEnded { if preview { store.openTab(item) } })
    .help(preview ? "\(item.fullReference)\nDouble-click to keep it open (⌘T)" : item.fullReference)
    .contextMenu {
      if preview {
        Button("Keep Open in Tab") { store.openTab(item) }
      }
      Button(preview ? "Close Preview" : "Close Tab") { store.closeTab(item.id) }
      if !preview {
        Button("Close Other Tabs") { store.closeOtherTabs(than: item.id) }
      }
      Divider()
      Button("Open in \(item.kind.name)") { NSWorkspace.shared.open(item.webURL) }
    }
  }
}

/// The four bits of a rectangle outside its rounded corners, filled even-odd.
private struct CardCorners: Shape {
  var radius: CGFloat

  func path(in rect: CGRect) -> Path {
    var path = Path(rect)
    path.addRoundedRect(in: rect, cornerSize: CGSize(width: radius, height: radius), style: .circular)
    return path
  }
}

extension View {
  /// A rounded sheet of paper on the window's colour.
  func paperCard() -> some View {
    background(Color.mrowPaper)
      // Corners are rounded by painting the window's colour outside them rather than by
      // clipping: a mask renders the whole card offscreen on every frame (Mdow's CardView).
      .overlay {
        CardCorners(radius: Theme.cardRadius)
          .fill(Color.mrowChrome, style: FillStyle(eoFill: true))
          .allowsHitTesting(false)
      }
      .overlay(RoundedRectangle(cornerRadius: Theme.cardRadius, style: .circular).strokeBorder(Color.mrowLine))
  }
}
