import AppKit
import SwiftUI

/// `Mrow --snapshot <folder>` loads the real queue, renders the selected merge request's card in
/// light and dark with each tab to PNGs, and quits. It needs no screen access, so the look can
/// be checked from a script.
@MainActor enum Snapshot {
  static func run(window: NSWindow, store: ReviewStore, state: WindowState, to folder: String) async {
    let output = URL(fileURLWithPath: folder, isDirectory: true)
    try? FileManager.default.createDirectory(at: output, withIntermediateDirectories: true)

    // Looking at merge requests here shouldn't mark them read in the app.
    store.remembersSeen = false
    // The window starts its own refresh; wait for that one.
    await store.refresh()
    for _ in 0..<160 where store.lastRefreshed == nil && store.error == nil {
      try? await Task.sleep(for: .milliseconds(250))
    }
    // MROW_FORGE=github shows that account instead of the last one used.
    if let name = ProcessInfo.processInfo.environment["MROW_FORGE"], let forge = Forge(rawValue: name) {
      store.switchForge(forge)
    }
    if let error = store.error { print("error: \(error)") }
    print("open: \(store.count(for: .toReview)) to review, \(store.count(for: .reviewed)) reviewed, \(store.count(for: .done)) done")
    for item in store.items(for: .toReview) {
      print("  \(item.fullReference)  requested \(item.requestedAt)  by \(item.requestedBy?.username ?? "?")  guess=\(item.requestedAtIsGuess)  state=\(item.myState.rawValue)")
    }

    // Which view gets clicks in the header: the title bar must not swallow the buttons.
    if let frame = window.contentView?.superview {
      let height = frame.bounds.height
      for (label, x) in [("sidebar button", 90.0), ("refresh button", 120.0), ("more button", frame.bounds.width - 22)] {
        let hit = frame.hitTest(NSPoint(x: x, y: height - 20))
        var chain: [String] = []
        var view = hit
        while let current = view, chain.count < 8 {
          chain.append("\(String(describing: type(of: current)).prefix(40)) \(current.frame)")
          view = current.superview
        }
        print("hit \(label): \(chain.joined(separator: " < "))")
      }
    }

    // The whole window in a few states: which list, which merge request, which tab.
    let shots: [(String, Bool, QueueMode, Int, DetailTab)] = [
      ("1-light-to-review", false, .toReview, 1, .overview),
      ("2-light-changes", false, .toReview, 1, .changes),
      ("3-dark-to-review", true, .toReview, 0, .overview),
      ("4-dark-discussion", true, .toReview, 1, .discussion),
      ("5-light-reviewed", false, .reviewed, 0, .overview),
    ]
    for (name, dark, mode, index, tab) in shots {
      let items = store.items(for: mode)
      guard !items.isEmpty else { continue }
      let item = items[min(index, items.count - 1)]
      store.mode = mode
      store.selectedID = item.id
      state.tab = tab
      store.loadDetailsIfNeeded(for: item)
      for _ in 0..<120 {
        if store.diffs[item.id]?.value != nil, store.discussions[item.id]?.value != nil { break }
        try? await Task.sleep(for: .milliseconds(250))
      }
      if tab == .changes { DiffStyles.shared.styleNow(store.diffs[item.id]?.value ?? []) }
      render(store: store, state: state, dark: dark, to: output.appendingPathComponent("\(name).png"))
    }

    // Oldest first, one author: the sidebar's sort and author filter.
    let prefs = Preferences.shared
    let wasOldestFirst = prefs.oldestFirst
    prefs.oldestFirst = true
    store.mode = .done
    store.authorFilter = store.authors.first?.person.username
    store.selectedID = store.visibleItems.first?.id
    state.tab = .overview
    render(store: store, state: state, dark: false, to: output.appendingPathComponent("6-light-oldest-by-author.png"))
    prefs.oldestFirst = wasOldestFirst
    store.authorFilter = nil

    // Three merge requests queued in tabs, and a fourth previewed.
    let queue = store.items(for: .toReview) + store.items(for: .reviewed)
    if queue.count >= 4 {
      store.mode = .toReview
      for item in queue.prefix(3) { store.openTab(item, activate: false) }
      store.selectedID = queue[1].id
      state.tab = .overview
      render(store: store, state: state, dark: false, to: output.appendingPathComponent("10-light-tabs.png"))
      store.selectedID = queue[3].id
      render(store: store, state: state, dark: true, to: output.appendingPathComponent("10-dark-tabs-preview.png"))
      for item in queue.prefix(3) { store.closeTab(item.id) }
    }

    // The "Ask Cursor" panel beside the card.
    if let item = queue.first {
      store.selectedID = item.id
      let wasVisible = state.askVisible
      state.askVisible = true
      render(store: store, state: state, dark: false, to: output.appendingPathComponent("11-light-window-ask.png"))
      state.askVisible = wasVisible
    }

    await viewedFiles(store: store, to: output)
    await modelPicker(to: output)
    await inlineThread(store: store, to: output)
    await pictures(store: store, to: output)
    await sections(store: store, to: output)
    NSApp.terminate(nil)
  }

  /// The lines around a comment thread on the code, from the first open merge request that has one.
  private static func inlineThread(store: ReviewStore, to output: URL) async {
    for item in store.open {
      store.loadDetailsIfNeeded(for: item)
      for _ in 0..<120 {
        if store.diffs[item.id]?.value != nil, store.discussions[item.id]?.value != nil { break }
        try? await Task.sleep(for: .milliseconds(250))
      }
      let threads = store.discussions[item.id]?.value ?? []
      guard let files = store.diffs[item.id]?.value,
        let thread = threads.first(where: { $0.notes.first?.position?.newLine != nil }),
        let position = thread.notes.first?.position,
        var file = files.first(where: { $0.newPath == position.newPath }),
        let at = file.lines.firstIndex(where: { $0.newNumber == position.newLine && $0.kind != .removed })
      else { continue }
      file.lines = Array(file.lines[max(0, at - 10)...min(file.lines.count - 1, at + 4)])
      DiffStyles.shared.styleNow([file])
      for dark in [false, true] {
        let view = ChangesView(item: item, files: [file], threads: [thread], baseURL: nil, scrollTo: { _ in })
          .padding(28)
          .frame(width: 980)
          .background(Color.mrowPaper)
          .environmentObject(store)
          .environment(\.isSnapshot, true)
          .environment(\.colorScheme, dark ? .dark : .light)
        write(view, dark: dark, to: output.appendingPathComponent("7-\(dark ? "dark" : "light")-inline-comment.png"))
      }
      print("inline thread: \(item.fullReference) \(position.newPath ?? "") line \(position.newLine ?? 0)")
      return
    }
    print("inline thread: none found")
  }

  /// The description with the most pictures (and videos) in it, once its pictures have loaded.
  private static func pictures(store: ReviewStore, to output: URL) async {
    func pictures(_ item: MergeRequest) -> [MarkdownImage] {
      MarkdownBlock.parse(item.description).flatMap { block -> [MarkdownImage] in
        let text: String
        switch block {
        case .paragraph(let body), .listItem(_, _, let body): text = body
        case .table(let header, let rows): text = (header + rows.flatMap { $0 }).joined(separator: "\n")
        default: return []
        }
        return MarkdownImage.segments(text).compactMap { if case .image(let image) = $0 { image } else { nil } }
      }
    }
    let ranked = (store.open + store.done).map { ($0, pictures($0)) }.sorted { $0.1.count > $1.1.count }
    for (item, all) in ranked.prefix(1) {
      let images = all.filter { !$0.isVideo }
      let string = item.webURL.absoluteString
      guard !images.isEmpty, let range = string.range(of: "/-/merge_requests/"),
        let base = URL(string: String(string[..<range.lowerBound]) + "/")
      else { continue }
      for image in images {
        if let source = ImageSource.resolve(image.source, base: base)?.source { _ = await ImageLoader.shared.image(source) }
      }
      for dark in [false, true] {
        let view = MarkdownView(source: item.description, baseURL: base)
          .padding(40)
          .frame(width: 980)
          .background(Color.mrowPaper)
          .environment(\.colorScheme, dark ? .dark : .light)
        await writeWindow(view, dark: dark, to: output.appendingPathComponent("8-\(dark ? "dark" : "light")-images.png"))
      }
      print("pictures: \(item.fullReference), \(images.count) images, \(all.count - images.count) videos")
      return
    }
    print("pictures: none found")
  }

  private static func render(store: ReviewStore, state: WindowState, dark: Bool, to url: URL) {
    let view = RootView()
      .environmentObject(store)
      .environmentObject(Preferences.shared)
      .environmentObject(state)
      .environment(\.isSnapshot, true)
      .environment(\.colorScheme, dark ? .dark : .light)
      .frame(width: 1280, height: 820)
    write(view, dark: dark, to: url)
  }

  /// A bot comment with `<details>` sections, as it opens (closed) and with every section open.
  private static func sections(store: ReviewStore, to output: URL) async {
    for item in store.open {
      store.loadDetailsIfNeeded(for: item)
      for _ in 0..<120 where store.discussions[item.id]?.value == nil {
        try? await Task.sleep(for: .milliseconds(250))
      }
      let notes = (store.discussions[item.id]?.value ?? []).flatMap(\.notes)
      guard let note = notes.first(where: { $0.body.contains("<details") }) else { continue }
      let string = item.webURL.absoluteString
      let base = string.range(of: "/-/merge_requests/").flatMap { URL(string: String(string[..<$0.lowerBound]) + "/") }
      for case .image(let image) in MarkdownImage.segments(note.body, badges: true) {
        if let source = ImageSource.resolve(image.source, base: base)?.source { _ = await ImageLoader.shared.image(source) }
      }
      for (name, body) in [("closed", note.body), ("open", note.body.replacingOccurrences(of: "<details>", with: "<details open>"))] {
        let view = MarkdownView(source: body, baseURL: base, size: 13.5, serif: false)
          .padding(28)
          .frame(width: 820)
          .background(Color.mrowPaper)
        await writeWindow(view, dark: false, to: output.appendingPathComponent("9-light-details-\(name).png"))
      }
      print("sections: \(item.fullReference) by \(note.author.username)")
      return
    }
    print("sections: none found")
  }

  /// The changes of a merge request with some files marked viewed (folded), one changed since.
  private static func viewedFiles(store: ReviewStore, to output: URL) async {
    guard let item = store.open.first(where: { (store.diffs[$0.id]?.value?.count ?? 0) >= 5 }),
      let files = store.diffs[item.id]?.value.map({ Array($0.prefix(6)) })
    else { return print("viewed: no merge request with five files loaded") }
    store.setViewed(true, files[0], in: item)
    store.setViewed(true, files[2], in: item)
    var stale = files[1]
    stale.digest &+= 1
    store.setViewed(true, stale, in: item)
    DiffStyles.shared.styleNow(files)
    let view = ChangesView(item: item, files: files, threads: [], baseURL: nil) { _ in }
      .environmentObject(store)
      .padding(28)
      .frame(width: 900, height: 1100, alignment: .top)
      .clipped()
      .background(Color.mrowPaper)
    await writeWindow(view, dark: false, to: output.appendingPathComponent("12-light-viewed.png"))
    await writeWindow(view, dark: true, to: output.appendingPathComponent("12-dark-viewed.png"))
    for file in files.prefix(3) { store.setViewed(false, file, in: item) }
  }

  private static func modelPicker(to output: URL) async {
    await AskStore.shared.loadModels()
    for _ in 0..<120 where AskStore.shared.models.isEmpty { try? await Task.sleep(for: .milliseconds(250)) }
    await AskStore.shared.describe(AskStore.shared.model)
    let view = ModelPicker(chat: nil) {}
      .background(Color.mrowElevated)
      .padding(20)
      .background(Color.mrowChrome)
    await writeWindow(view, dark: false, to: output.appendingPathComponent("13-light-models.png"))
    await writeWindow(view, dark: true, to: output.appendingPathComponent("13-dark-models.png"))
    ACPClient.shared.stop()
  }

  /// `Mrow --ask <folder> [project] [question]`: asks Cursor a real question about the first merge
  /// request to review (in `project` if given), prints the chat, and renders the panel before
  /// and after. The chat isn't kept for the app.
  static func ask(store: ReviewStore, to folder: String, project: String?, question: String) async {
    let output = URL(fileURLWithPath: folder, isDirectory: true)
    try? FileManager.default.createDirectory(at: output, withIntermediateDirectories: true)
    store.remembersSeen = false
    AskStore.shared.remembersSessions = false
    await store.refresh()
    for _ in 0..<160 where store.lastRefreshed == nil && store.error == nil {
      try? await Task.sleep(for: .milliseconds(250))
    }
    let queue = store.items(for: .toReview) + store.open
    guard let item = queue.first(where: { project == nil || $0.projectPath.hasSuffix(project!) }) else {
      print("ask: no merge request found")
      exit(1)
    }
    print("ask: \(item.fullReference) \(item.title)")
    let chat = AskStore.shared.chat(for: item)
    await AskStore.shared.prepare(chat, for: item)
    print("checkout: \(chat.checkout?.path ?? "none")")
    let state = WindowState()
    func panel(height: CGFloat = 820) -> some View {
      AskPanel(item: item, chat: chat) {}
        .environmentObject(store)
        .environmentObject(state)
        .paperCard()
        .frame(width: 400, height: height)
        .padding(14)
        .background(Color.mrowChrome)
    }
    await writeWindow(panel(), dark: false, to: output.appendingPathComponent("11-light-ask-empty.png"))
    // No question: only the empty state, without asking Cursor anything.
    if question.isEmpty { exit(0) }

    let started = Date()
    // The panel mid-answer: starting, reading, then streaming.
    let peek = Task {
      for (index, delay) in [1.5, 5.0, 4.0].enumerated() {
        try? await Task.sleep(for: .seconds(delay))
        await writeWindow(panel(), dark: index == 1, to: output.appendingPathComponent("11-ask-working-\(index + 1).png"))
      }
    }
    await AskStore.shared.ask(question, in: chat, item: item, store: store)
    await peek.value
    print(String(format: "answered in %.1fs; permission requests declined: %d", Date().timeIntervalSince(started), ACPClient.shared.declined))
    let ask = AskStore.shared
    print("model: \(ask.model), effort as Cursor reports it: \(ask.efforts[ask.model]?.current ?? "?") (chosen: \(ask.chosenEfforts[ask.model] ?? "none"))")
    for entry in chat.entries {
      switch entry.kind {
      case .tool(_, let done): print("[tool \(done.map { $0 ? "done" : "failed" } ?? "running")] \(entry.text)")
      case .thought: print("[thought] \(entry.text.count) characters")
      default: print("[\(entry.kind)] \(entry.text)")
      }
    }
    await writeWindow(panel(), dark: false, to: output.appendingPathComponent("11-light-ask.png"))
    await writeWindow(panel(), dark: true, to: output.appendingPathComponent("11-dark-ask.png"))
    // Tall enough to show the chat from its first question.
    await writeWindow(panel(height: 2400), dark: false, to: output.appendingPathComponent("11-light-ask-whole.png"))
    ACPClient.shared.stop()
    exit(0)
  }

  /// Through a real (offscreen) window rather than ImageRenderer, which leaves scroll views (the
  /// tables) blank.
  private static func writeWindow<V: View>(_ view: V, dark: Bool, to url: URL) async {
    let host = NSHostingView(rootView: view)
    let width = host.fittingSize.width
    let window = NSWindow(
      contentRect: NSRect(x: -4000, y: 0, width: width, height: 800), styleMask: [.borderless], backing: .buffered,
      defer: false)
    window.appearance = NSAppearance(named: dark ? .darkAqua : .aqua)
    window.contentView = host
    window.orderBack(nil)
    try? await Task.sleep(for: .milliseconds(300))
    window.setContentSize(NSSize(width: width, height: host.fittingSize.height))
    host.layoutSubtreeIfNeeded()
    try? await Task.sleep(for: .milliseconds(300))
    defer { window.orderOut(nil) }
    guard let rep = host.bitmapImageRepForCachingDisplay(in: host.bounds) else { return }
    host.cacheDisplay(in: host.bounds, to: rep)
    try? rep.representation(using: .png, properties: [:])?.write(to: url)
    print("wrote \(url.lastPathComponent)")
  }

  private static func write<V: View>(_ view: V, dark: Bool, to url: URL) {
    let renderer = ImageRenderer(content: view)
    renderer.scale = 2
    var image: NSImage?
    NSAppearance(named: dark ? .darkAqua : .aqua)!.performAsCurrentDrawingAppearance {
      image = renderer.nsImage
    }
    guard let image, let tiff = image.tiffRepresentation, let rep = NSBitmapImageRep(data: tiff),
      let png = rep.representation(using: .png, properties: [:])
    else { return }
    try? png.write(to: url)
    print("wrote \(url.lastPathComponent)")
  }
}
