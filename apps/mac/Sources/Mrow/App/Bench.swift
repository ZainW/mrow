import AppKit
import SwiftUI

/// The performance harness.
///
/// `Mrow --bench-record <folder>` saves the real queue, and the diffs and threads of every open
/// merge request plus the biggest recent finished ones, through read-only `glab` calls.
/// `Mrow --bench <folder> [label]` then times the app against that recording without touching
/// the network: the pure work (decoding, parsing, highlighting, word diffs, markdown, list
/// filtering) and real SwiftUI layout and drawing in an offscreen window (switching merge
/// requests, opening the changes, scrolling them, typing in the filter). Results go to
/// `<folder>/results-<label>.json`; `scripts/bench-compare.py` puts two runs side by side.
@MainActor enum Bench {
  private struct Queue: Codable {
    let me: Person
    let open: [MergeRequest]
    let done: [MergeRequest]
  }

  struct Stat: Codable {
    let name: String
    let unit: String
    let samples: [Double]

    var median: Double { sorted[sorted.count / 2] }
    var p95: Double { sorted[min(sorted.count - 1, Int((Double(sorted.count) * 0.95).rounded(.up)) - 1)] }
    var max: Double { sorted.last ?? 0 }
    private var sorted: [Double] { samples.sorted() }
  }

  // MARK: Recording

  static func record(to folder: String) async {
    let output = URL(fileURLWithPath: folder, isDirectory: true)
    try? FileManager.default.createDirectory(at: output, withIntermediateDirectories: true)
    let store = ReviewStore()
    store.remembersSeen = false
    await store.refresh()
    guard let me = store.me else {
      print("error: \(store.error ?? "no user")")
      exit(1)
    }
    let biggestDone = store.done.sorted { $0.additions + $0.deletions > $1.additions + $1.deletions }.prefix(12)
    let picks = store.open + biggestDone
    do {
      let queue = Queue(me: me, open: store.open, done: store.done)
      try JSONEncoder().encode(queue).write(to: output.appendingPathComponent("queue.json"))
      for item in picks {
        let base = "projects/\(item.projectID)/merge_requests/\(item.iid)"
        try await rawPages("\(base)/diffs").write(to: output.appendingPathComponent("\(key(item)).diffs.json"))
        try await rawPages("\(base)/discussions").write(
          to: output.appendingPathComponent("\(key(item)).discussions.json"))
        print("recorded \(item.fullReference)")
      }
    } catch {
      print("error: \(error.localizedDescription)")
      exit(1)
    }
    print("recorded \(picks.count) merge requests to \(output.path)")
    exit(0)
  }

  private static func rawPages(_ path: String) async throws -> Data {
    var all: [Any] = []
    for page in 1...10 {
      let data = try await Glab.api(["\(path)?per_page=100&page=\(page)"])
      let items = try JSONSerialization.jsonObject(with: data) as? [Any] ?? []
      all += items
      if items.count < 100 { break }
    }
    return try JSONSerialization.data(withJSONObject: all)
  }

  private static func key(_ item: MergeRequest) -> String {
    item.id.split(separator: "/").last.map(String.init) ?? item.iid
  }

  // MARK: Running

  static func run(fixtures folder: String, label: String) async {
    let input = URL(fileURLWithPath: folder, isDirectory: true)
    guard let data = try? Data(contentsOf: input.appendingPathComponent("queue.json")),
      let queue = try? JSONDecoder().decode(Queue.self, from: data)
    else {
      print("error: no recording in \(folder); run --bench-record first")
      exit(1)
    }
    var rawDiffs: [String: Data] = [:]
    var threads: [String: [Discussion]] = [:]
    for item in queue.open + queue.done {
      if let diff = try? Data(contentsOf: input.appendingPathComponent("\(key(item)).diffs.json")) {
        rawDiffs[item.id] = diff
      }
      if let raw = try? Data(contentsOf: input.appendingPathComponent("\(key(item)).discussions.json")) {
        threads[item.id] = (try? JSONCoding.decoder.decode([Discussion].self, from: raw)) ?? []
      }
    }
    var stats: [Stat] = []
    // MROW_BENCH_ONLY=discussion (say) runs one part, for profiling it alone.
    let only = ProcessInfo.processInfo.environment["MROW_BENCH_ONLY"]
    func wants(_ part: String) -> Bool { only == nil || only == part }

    // Pure work.
    var diffs: [String: [FileDiff]] = [:]
    stats.append(
      measure("decode + parse every diff", runs: 7) {
        diffs = rawDiffs.mapValues { raw in
          ((try? JSONCoding.decoder.decode([FileDiff].self, from: raw)) ?? []).map { $0.prepared() }
        }
      })
    let files = diffs.values.flatMap { $0 }
    let lineCount = files.reduce(0) { $0 + $1.lines.count }
    stats.append(
      measure("highlight every diff (\(files.count) files, \(lineCount) lines)", runs: 7) {
        for file in files { _ = DiffStyler.styles(file.lines, language: DiffStyler.language(for: file.newPath)) }
      })
    stats.append(
      measure("  … of which lexing alone", runs: 7) {
        for file in files {
          _ = SyntaxHighlighter.tokens(in: file.lines.map(\.text).joined(separator: "\n"), language: DiffStyler.language(for: file.newPath))
        }
      })
    let big = syntheticFile(lines: 6000)
    stats.append(
      measure("highlight one 6,000-line TypeScript diff", runs: 7) {
        _ = DiffStyler.styles(big.lines, language: "typescript")
      })
    let pairs = syntheticPairs(count: 2000)
    stats.append(
      measure("word diff 2,000 long line pairs", runs: 7) {
        for (a, b) in pairs { _ = DiffStyler.changedRanges(a, b) }
      })
    let texts = (queue.open + queue.done).map(\.description) + threads.values.flatMap { $0.flatMap(\.notes).map(\.body) }
    stats.append(
      measure("parse markdown (\(texts.count) descriptions and notes)", runs: 7) {
        for text in texts { _ = MarkdownBlock.parse(text) }
      })

    // The window.
    let store = ReviewStore()
    store.load(me: queue.me, open: queue.open, done: queue.done, diffs: diffs, discussions: threads)
    let state = WindowState()
    let host = NSHostingView(
      rootView: RootView()
        .environmentObject(store)
        .environmentObject(Preferences.shared)
        .environmentObject(state))
    let window = NSWindow(
      contentRect: NSRect(x: 0, y: 0, width: 1280, height: 820), styleMask: [.titled, .fullSizeContentView],
      backing: .buffered, defer: false)
    window.contentView = host
    window.appearance = NSAppearance(named: .aqua)
    let rep = host.bitmapImageRepForCachingDisplay(in: host.bounds)!
    // What the app's main thread does for a frame: apply the change, lay out, draw the layers
    // that need it and commit. Compositing is the window server's job and isn't counted
    // (`cacheDisplay`, which re-renders every layer on the CPU, is only for the check image).
    func draw() {
      host.layoutSubtreeIfNeeded()
      host.displayIfNeeded()
      CATransaction.flush()
    }
    func frame(_ change: () -> Void) async -> Double {
      let start = DispatchTime.now().uptimeNanoseconds
      change()
      await Task.yield()
      draw()
      return Double(DispatchTime.now().uptimeNanoseconds - start) / 1e6
    }
    draw()

    stats.append(
      measure("sidebar sections, filtered (×200)", runs: 7) {
        store.filter = "e"
        for _ in 0..<200 { _ = store.sections }
        store.filter = ""
      })

    // Switching merge requests on the overview.
    var switches: [Double] = []
    for item in (queue.open + queue.done).prefix(wants("switch") ? 24 : 0) {
      switches.append(await frame { state.tab = .overview; store.selectedID = item.id })
    }
    stats.append(Stat(name: "switch merge request (overview), per frame", unit: "ms", samples: switches))

    // The biggest recorded merge request's changes: first frame, then until highlighted.
    let biggest = queue.open.max { (diffs[$0.id]?.count ?? 0) < (diffs[$1.id]?.count ?? 0) }
      .flatMap { item in (diffs[item.id]?.isEmpty == false) ? item : nil }
      ?? (queue.open + queue.done).max { (diffs[$0.id]?.count ?? 0) < (diffs[$1.id]?.count ?? 0) }!
    let biggestFiles = diffs[biggest.id] ?? []
    var opens: [Double] = []
    var ready: [Double] = []
    for _ in 0..<(wants("changes") ? 5 : 0) {
      _ = await frame { state.tab = .overview }
      DiffStyles.shared.reset()
      let start = DispatchTime.now().uptimeNanoseconds
      opens.append(await frame { store.selectedID = biggest.id; state.tab = .changes })
      while !biggestFiles.allSatisfy({ $0.lines.count > 400 || DiffStyles.shared.styled[DiffStyles.key($0)] != nil }) {
        try? await Task.sleep(for: .milliseconds(1))
      }
      draw()
      ready.append(Double(DispatchTime.now().uptimeNanoseconds - start) / 1e6)
    }
    let shownLines = biggestFiles.filter { $0.lines.count <= 400 }.reduce(0) { $0 + $1.lines.count }
    stats.append(
      Stat(
        name: "open changes of \(biggest.shortReference) (\(biggestFiles.count) files, \(shownLines) lines shown), first frame",
        unit: "ms", samples: opens))
    stats.append(Stat(name: "  … until every shown line is highlighted", unit: "ms", samples: ready))

    // Scrolling those changes, 240pt a frame, top to bottom.
    var scrolls: [Double] = []
    if let scroll = scrollViews(in: host).max(by: {
      ($0.documentView?.frame.height ?? 0) < ($1.documentView?.frame.height ?? 0)
    }) {
      let clip = scroll.contentView
      var y: CGFloat = 0
      while scrolls.count < 400, wants("changes") {
        let limit = (scroll.documentView?.frame.height ?? 0) - clip.bounds.height
        if y >= limit { break }
        y = min(y + 240, limit)
        scrolls.append(
          await frame {
            clip.scroll(to: NSPoint(x: 0, y: y))
            scroll.reflectScrolledClipView(clip)
          })
      }
      print("scrolled \(Int(y))pt of \(Int(scroll.documentView?.frame.height ?? 0))pt in \(scrolls.count) frames")
      host.cacheDisplay(in: host.bounds, to: rep)
      try? rep.representation(using: .png, properties: [:])?.write(
        to: input.appendingPathComponent("check-\(label).png"))
    }
    stats.append(Stat(name: "scroll the changes, per frame", unit: "ms", samples: scrolls))

    // The discussion with the most notes.
    if let chatty = (queue.open + queue.done).max(by: {
      (threads[$0.id]?.count ?? 0) < (threads[$1.id]?.count ?? 0)
    }) {
      var samples: [Double] = []
      for _ in 0..<(wants("discussion") ? 5 : 0) {
        _ = await frame { state.tab = .overview; store.selectedID = queue.open.first?.id }
        samples.append(await frame { store.selectedID = chatty.id; state.tab = .discussion })
      }
      stats.append(
        Stat(
          name: "open discussion of \(chatty.shortReference) (\(threads[chatty.id]?.count ?? 0) threads), first frame",
          unit: "ms", samples: samples))
    }

    // Typing in the filter.
    var typing: [Double] = []
    state.tab = .overview
    for _ in 0..<(wants("typing") ? 3 : 0) {
      for count in 1...5 { typing.append(await frame { store.filter = String("shray".prefix(count)) }) }
      typing.append(await frame { store.filter = "" })
    }
    stats.append(Stat(name: "type in the sidebar filter, per keystroke", unit: "ms", samples: typing))
    stats.append(Stat(name: "memory after the run", unit: "MB", samples: [residentMegabytes()]))

    stats = stats.filter { !$0.samples.isEmpty }
    report(stats, label: label, to: input.appendingPathComponent("results-\(label).json"))
    exit(0)
  }

  // MARK: Helpers

  private static func measure(_ name: String, runs: Int, _ body: () -> Void) -> Stat {
    body()  // Warm up.
    var samples: [Double] = []
    for _ in 0..<runs {
      let start = DispatchTime.now().uptimeNanoseconds
      body()
      samples.append(Double(DispatchTime.now().uptimeNanoseconds - start) / 1e6)
    }
    return Stat(name: name, unit: "ms", samples: samples)
  }

  private static func scrollViews(in view: NSView) -> [NSScrollView] {
    let own = (view as? NSScrollView).map { [$0] } ?? []
    return own + view.subviews.flatMap { scrollViews(in: $0) }
  }

  private static func residentMegabytes() -> Double {
    var info = mach_task_basic_info()
    var count = mach_msg_type_number_t(MemoryLayout<mach_task_basic_info>.size / MemoryLayout<natural_t>.size)
    let result = withUnsafeMutablePointer(to: &info) {
      $0.withMemoryRebound(to: integer_t.self, capacity: Int(count)) {
        task_info(mach_task_self_, task_flavor_t(MACH_TASK_BASIC_INFO), $0, &count)
      }
    }
    return result == KERN_SUCCESS ? Double(info.resident_size) / 1_048_576 : 0
  }

  private static func report(_ stats: [Stat], label: String, to url: URL) {
    print("\n\(label)")
    for stat in stats {
      let line =
        stat.unit == "MB"
        ? String(format: "%8.1f MB", stat.median)
        : String(format: "%9.2f ms median  %9.2f p95  %9.2f max  (n=%d)", stat.median, stat.p95, stat.max, stat.samples.count)
      print("  \(stat.name.padding(toLength: 78, withPad: " ", startingAt: 0)) \(line)")
    }
    let encoder = JSONEncoder()
    encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
    try? encoder.encode(stats).write(to: url)
    print("wrote \(url.path)")
  }

  /// A long TypeScript diff: mostly context, with edits, comments and template strings spread through.
  private static func syntheticFile(lines: Int) -> FileDiff {
    var diff = "@@ -1,\(lines) +1,\(lines) @@\n"
    for i in 0..<lines {
      switch i % 10 {
      case 0: diff += " /** Handles item \(i). Spans\n"
      case 1: diff += "  * two lines. */\n"
      case 3: diff += "-export const value\(i) = compute(\"old\", \(i), { retries: 3 });\n"
      case 4: diff += "+export const value\(i) = compute(\"new\", \(i * 2), { retries: 5, backoff: true });\n"
      case 6: diff += " const message = `item ${value\(i)} is ${\"ready\"}`;\n"
      default: diff += " function handler\(i)(input: Record<string, number>): Promise<void> { return run(input); }\n"
      }
    }
    let json = """
      [{"old_path":"big.ts","new_path":"big.ts","diff":\(String(data: try! JSONEncoder().encode(diff), encoding: .utf8)!),
      "new_file":false,"renamed_file":false,"deleted_file":false}]
      """
    return try! JSONCoding.decoder.decode([FileDiff].self, from: Data(json.utf8))[0].prepared()
  }

  private static func syntheticPairs(count: Int) -> [([UInt16], [UInt16])] {
    (0..<count).map { i in
      let words = (0..<45).map { "word\(($0 * 7 + i) % 13)" }
      var changed = words
      changed[i % 45] = "changed"
      changed[(i * 3) % 45] = "edited"
      return (Array(words.joined(separator: " ").utf16), Array(changed.joined(separator: " ").utf16))
    }
  }
}

/// `Mrow --selftest`: checks for the edge cases the review turned up. Exits non-zero on failure.
@MainActor enum SelfTest {
  static func run() {
    var failures = 0
    func check(_ condition: Bool, _ name: String) {
      print("\(condition ? "ok  " : "FAIL") \(name)")
      if !condition { failures += 1 }
    }

    let crlf = DiffLine.parse("@@ -1,2 +1,2 @@\r\n-a\r\n+b\r\n c\r\n")
    check(crlf.map(\.kind) == [.hunk, .removed, .added, .context], "CRLF diff splits into lines")
    check(crlf.map(\.text) == ["@@ -1,2 +1,2 @@", "a", "b", "c"], "CRLF lines lose the \\r and marker")
    check(crlf[3].oldNumber == 2 && crlf[3].newNumber == 2, "CRLF line numbers")
    let combining = DiffLine.parse("@@ -1 +1 @@\n+\u{301}x\n")
    check(combining.last?.kind == .added && combining.last?.text == "\u{301}x", "marker before a combining mark")

    // A comment opened in one hunk (closed somewhere off screen) mustn't swallow the next hunk.
    let hunks = DiffLine.parse("@@ -1,2 +1,2 @@\n let a = 1\n+/* start\n@@ -10,1 +10,1 @@\n+let b = 2\n")
    let styled = DiffStyler.styles(hunks, language: "typescript")
    check(styled[4].runs.first?.kind == .keyword, "hunks are lexed apart")

    let note = { (body: String) in [ActivityNote(body: body, createdAt: Date(), author: nil)] }
    check(ReviewStore.requestNotes(note("requested review from @alex.smith"), me: "alex").isEmpty,
      "@alex doesn't match @alex.smith")
    check(!ReviewStore.requestNotes(note("requested review from @alex."), me: "alex").isEmpty, "@alex. matches")
    check(!ReviewStore.requestNotes(note("requested review from @bo and @alex"), me: "alex").isEmpty, "@alex in a list")

    // GitHub: a fictional pull request, re-requested from me after I'd approved it.
    let pullJSON = """
      {"id":"PR_1","number":7,"title":"t","url":"https://github.com/acme/web/pull/7",
       "createdAt":"2026-01-01T00:00:00Z","updatedAt":"2026-01-03T00:00:00Z","mergedAt":null,"closedAt":null,
       "state":"OPEN","isDraft":false,"headRefName":"f","baseRefName":"main","mergeStateStatus":"BEHIND",
       "totalCommentsCount":2,"body":"b","additions":1,"deletions":2,"changedFiles":1,
       "repository":{"nameWithOwner":"acme/web","name":"web"},"author":{"login":"ann","name":"Ann"},
       "reviewRequests":{"nodes":[{"requestedReviewer":{"login":"me","name":"Me"}},
                                  {"requestedReviewer":{"slug":"core","name":"Core"}}]},
       "latestReviews":{"nodes":[{"state":"APPROVED","author":{"login":"me"}},
                                 {"state":"CHANGES_REQUESTED","author":{"login":"bob"}}]},
       "labels":{"nodes":[{"name":"bug","color":"d73a4a"}]},
       "commits":{"nodes":[{"commit":{"statusCheckRollup":{"state":"FAILURE"}}}]},
       "timelineItems":{"nodes":[{},
         {"createdAt":"2026-01-01T01:00:00Z","actor":{"login":"ann"},"requestedReviewer":{"login":"me"}},
         {"createdAt":"2026-01-02T01:00:00Z","actor":{"login":"ann"},"requestedReviewer":{"slug":"core"}},
         {"createdAt":"2026-01-03T01:00:00Z","actor":{"login":"ann"},"requestedReviewer":{"login":"ME"}}]}}
      """
    if let node = try? JSONCoding.decoder.decode(GitHubPullNode.self, from: Data(pullJSON.utf8)) {
      let pull = GitHubProvider.build(node, me: "me", pending: false)
      check(pull.myState == .unreviewed, "github: a re-request puts it back to review")
      check(pull.shortReference == "web#7" && pull.kind == .github, "github: reference uses #")
      check(pull.requestedAt == JSONCoding.parse("2026-01-03T01:00:00Z") && pull.wasReRequested,
        "github: asked at the latest request naming me")
      check(pull.reviewers.map(\.person.username) == ["me", "core", "bob"], "github: requests, teams, then reviews")
      check(pull.pipeline == .failed && pull.mergeStatus == "need_rebase", "github: checks and merge state")
      check(pull.labels.first?.color == "#d73a4a" && pull.labels.first?.textColor == "#ffffff", "github: label colours")
    } else {
      check(false, "github: pull request decodes")
    }
    check(GitHubProvider.textColor(onHex: "fbca04") == "#1f2328", "github: dark text on a light label")

    let pictures = MarkdownImage.segments(
      "Before:\n![image.png](/uploads/abc/image.png){width=900 height=117} after ![ok](https://x.dev/b.svg) done")
    check(pictures.count == 3, "a screenshot splits its paragraph")
    if case .image(let image) = pictures[1] {
      check(image.width == 900 && image.height == 117 && image.source == "/uploads/abc/image.png", "pasted size is read")
    } else {
      check(false, "pasted size is read")
    }
    if case .text(let rest) = pictures[2] { check(rest.contains("b.svg"), "badges stay in the text") }
    check(MarkdownImage.segments("plain text").count == 1, "text without pictures is one piece")
    let html = MarkdownImage.markdownForHTML(
      #"<a href="https://c.dev/x"><img alt="Open" width="114" height="28" src="https://c.dev/o.png"></a>"#)
    check(html == "[![Open](https://c.dev/o.png){width=114 height=28}](https://c.dev/x)", "<a><img></a> becomes a linked image")
    let percent = MarkdownImage.segments(#"![wide](/uploads/s/w.png){width="100%"}"#)
    if case .image(let image) = percent.first { check(image.width == nil, "percent widths are ignored") }
    let base = URL(string: "https://gitlab.com/group/sub/project")
    check(ImageSource.resolve("/uploads/s3cr3t/my%20shot.png", base: base)?.source
      == .upload(project: "group/sub/project", secret: "s3cr3t", file: "my shot.png"), "relative upload")
    check(ImageSource.resolve("https://gitlab.com/group/sub/project/-/uploads/s3cr3t/a.png", base: base)?.source
      == .upload(project: "group/sub/project", secret: "s3cr3t", file: "a.png"), "absolute upload")
    check(ImageSource.resolve("/-/project/42/uploads/s3cr3t/a.png", base: base)?.source
      == .upload(project: "42", secret: "s3cr3t", file: "a.png"), "upload by project id")
    check(ImageSource.resolve("https://example.com/a.png", base: base)?.source
      == .web(URL(string: "https://example.com/a.png")!), "other hosts are plain fetches")

    let nested = MarkdownBlock.parse(
      "Intro\n\n<details>\n<summary>🟡 Other (1)</summary><blockquote>\n\n<details>\n<summary>inner</summary>\n\nbody\n\n</details>\n\n</blockquote></details>\n\nAfter")
    check(nested.count == 3, "a details section sits between its paragraphs")
    if case .details(let summary, let open, let body) = nested[1] {
      check(summary == "🟡 Other (1)" && !open, "summary is read and closed by default")
      if case .details(let inner, _, _)? = MarkdownBlock.parse(body).first {
        check(inner == "inner", "nested sections match their own closing tag")
      } else {
        check(false, "nested sections match their own closing tag")
      }
    } else {
      check(false, "summary is read and closed by default")
    }
    check(MarkdownBlock.parse("<details open><summary>x</summary>y</details>").first
      == .details(summary: "x", open: true, body: "y"), "<details open> starts open")
    check(MarkdownBlock.parse("```html\n<b>bold</b>\n```").first == .code(language: "html", text: "<b>bold</b>"),
      "tags in code are left alone")
    check(MarkdownBlock.parse("<ul><li>one <code>x</code></li><li>two</li></ul>")
      == [.listItem(indent: 0, marker: .bullet, text: "one `x`"), .listItem(indent: 0, marker: .bullet, text: "two")],
      "<ul><li> becomes a list")

    let a = Array("let total = price * count;".utf16)
    let b = Array("let total = price * quantity;".utf16)
    let marks = DiffStyler.changedRanges(a, b)
    check(marks?.0.count == 1 && marks?.1.count == 1, "one changed word on each side")
    check(DiffStyler.changedRanges(Array("abc".utf16), Array("xyz completely different".utf16)) == nil,
      "no marks when lines share nothing")
    let emoji = DiffLine.parse("@@ -1 +1 @@\n+😀 \"é\"\n")
    let emojiStyle = DiffStyler.styles(emoji, language: "swift")
    check(String(DiffStyler.attributed(emoji[1], style: emojiStyle[1]).characters) == "😀 \"é\"", "emoji survive")

    // Cursor's code citations are filled from the clone; ones it wrote out are kept.
    let clone = FileManager.default.temporaryDirectory.appendingPathComponent("mrow-selftest-\(getpid())")
    try? FileManager.default.createDirectory(at: clone.appendingPathComponent("src"), withIntermediateDirectories: true)
    try? "one\ntwo\nthree\nfour\n".write(to: clone.appendingPathComponent("src/a.ts"), atomically: true, encoding: .utf8)
    defer { try? FileManager.default.removeItem(at: clone) }
    check(Citations.expand("See:\n```2:3:src/a.ts\n```\nso.", checkout: clone)
      == "See:\n```src/a.ts lines 2–3\ntwo\nthree\n```\nso.", "citation filled from the clone")
    check(Citations.expand("```1:1:\(clone.path)/src/a.ts\nmine\n```", checkout: clone)
      == "```src/a.ts line 1\nmine\n```", "written-out citation kept, path shortened")
    check(Citations.expand("```4:9:src/gone.ts\n```", checkout: clone) == "`gone.ts` · lines 4–9\n", "missing file leaves the reference")
    check(Citations.expand("```ts\nlet a = 1\n```", checkout: clone) == "```ts\nlet a = 1\n```", "plain fences untouched")
    let catalog = ["auto-smart", "claude-opus-4-8", "claude-opus-5-5", "claude-opus-5", "gpt-5.5", "claude-sonnet-5-5",
      "claude-haiku-4-5", "gemini-3.1-pro", "composer-2.5"].map { AgentModel(id: $0, name: "") }
    check(AgentModel.featured(in: catalog).map(\.id) == ["auto-smart", "claude-opus-5-5", "claude-sonnet-5-5", "claude-haiku-4-5", "composer-2.5"],
      "featured models: the newest of each family, in order")
    check(AgentModel(id: "gemini-3.1-pro", name: "").family == "gemini-pro", "a version in the middle of an id")
    check(Citations.dedent("    a\n      b\n\n    c\n") == "a\n  b\n\nc\n", "cited code loses its common indent")
    let home = FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent("code/app")
    check(AskStore.title(JSONObject(value: ["title": "Read code/app/src/x.ts (1 - 9)"]), checkout: home) == "Read src/x.ts (1 - 9)",
      "tool titles drop the home-relative clone path")
    check(AskStore.title(JSONObject(value: ["title": "Read \(home.path)/src/x.ts"]), checkout: home) == "Read src/x.ts",
      "tool titles drop the absolute clone path")

    // Cursor's models and their effort settings, named for the picker.
    let sonnet = AgentModel(id: "claude-sonnet-5-5", name: "Claude Sonnet 5.5")
    check(sonnet.title == "Claude Sonnet 5.5" && sonnet.shortTitle == "Sonnet 5.5" && sonnet.maker == "Anthropic", "model title")
    check(AgentModel(id: "gpt-5.3-codex", name: "").title == "GPT-5.3 Codex" && AgentModel.pretty("gemini-3.8-flash") == "Gemini 3.8 Flash"
      && AgentModel.pretty("claude-opus-4-8") == "Claude Opus 4.8" && AgentModel.pretty("glm-5p3") == "GLM 5p3", "model names")
    check(AgentModel.base("claude-sonnet-5-5[context=300k,reasoning_effort=high]") == "claude-sonnet-5-5", "old variant ids")
    let options = [
      JSONObject(value: ["id": "model", "category": "model", "currentValue": "gpt-5.5"]),
      JSONObject(value: [
        "id": "reasoning", "category": "thought_level", "currentValue": "medium",
        "options": [["value": "none", "name": "None"], ["value": "medium", "name": "Medium"], ["value": "extra-high", "name": "Extra High"]],
      ]),
      JSONObject(value: ["id": "fast", "category": "model_config", "currentValue": "false"]),
    ]
    let effort = EffortOptions.from(options)
    check(effort?.model == "gpt-5.5" && effort?.effort.configID == "reasoning" && effort?.effort.current == "medium"
      && effort?.effort.values.map(\.name) == ["None", "Medium", "Extra High"], "effort setting read from config options")
    check(EffortOptions.from([options[0]])?.effort.values.isEmpty == true, "model without an effort setting")

    // Viewed files are matched by a digest that must survive a relaunch.
    check(FileDiff.fnv1a("+a\n") == FileDiff.fnv1a("+a\n") && FileDiff.fnv1a("+a\n") != FileDiff.fnv1a("+b\n"), "digest tells diffs apart")
    check(FileDiff.fnv1a("") == Int(bitPattern: 0xcbf2_9ce4_8422_2325), "digest is FNV-1a, not hashValue")

    print(failures == 0 ? "all passed" : "\(failures) failed")
    exit(failures == 0 ? 0 : 1)
  }
}
