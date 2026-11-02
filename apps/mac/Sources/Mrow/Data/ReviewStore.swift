import AppKit
import Combine
import UserNotifications

enum Loadable<Value> {
  case idle
  case loading
  case loaded(Value)
  case failed(String)

  var value: Value? {
    if case .loaded(let value) = self { return value }
    return nil
  }
}

/// The three lists in the sidebar.
enum QueueMode: Int, CaseIterable, Identifiable {
  case toReview, reviewed, done

  var id: Int { rawValue }

  var title: String {
    switch self {
    case .toReview: "To review"
    case .reviewed: "Reviewed"
    case .done: "Done"
    }
  }
}

struct QueueSection: Identifiable {
  let title: String
  let items: [MergeRequest]
  var id: String { title }
}

/// One forge's side of the queue: who I am there and what's waiting.
struct Account {
  var me: Person?
  var open: [MergeRequest] = []
  var done: [MergeRequest] = []
  var error: String?
  var lastRefreshed: Date?
}

/// Everything the window shows: the review queues from each forge (GitLab, GitHub), which one
/// the sidebar is on, and what's selected.
@MainActor final class ReviewStore: ObservableObject {
  /// Every enabled forge's queue, refreshed together so the Dock badge, notifications and the
  /// account switcher's counts cover all of them.
  @Published private(set) var accounts: [Forge: Account] = [:]
  /// The account the sidebar shows; switched from the sidebar's account picker.
  @Published private(set) var forge: Forge = Preferences.shared.activeForge
  @Published private(set) var isLoading = false
  @Published var mode: QueueMode = .toReview
  @Published var filter = ""
  /// Only merge requests by this author (a username), when set.
  @Published var authorFilter: String?
  @Published var selectedID: String? {
    didSet {
      guard let selected else { return }
      markSeen(selected)
      // A tab from the other account brings its account along, so the sidebar matches.
      if selected.kind != forge { setForge(selected.kind) }
    }
  }
  /// Merge requests kept open in tabs, in order: a queue to work through. The selected one may
  /// be outside it, as a preview (a single click), the way VS Code previews a file.
  @Published private(set) var tabs: [String] = Preferences.shared.tabs {
    didSet { if remembersSeen { Preferences.shared.tabs = tabs } }
  }
  @Published private(set) var lastSeen: [String: Date] = Preferences.shared.lastSeen
  @Published private(set) var viewedFiles: [String: [String: Int]] = Preferences.shared.viewedFiles
  @Published private(set) var diffs: [String: Loadable<[FileDiff]>] = [:]
  @Published private(set) var discussions: [String: Loadable<[Discussion]>] = [:]
  /// The commits the loaded diffs were taken between; a comment on a line needs them.
  @Published private(set) var diffRefs: [String: DiffRefs] = [:]
  /// The action running on the selected merge request, e.g. "Approving…".
  @Published private(set) var busy: String?
  /// A short message under the header, e.g. "Approved".
  @Published var toast: String?

  /// Snapshots and benchmarks: never ask GitLab, never touch the saved read state.
  var offline = false
  var remembersSeen = true

  private let prefs = Preferences.shared
  private var timer: Timer?
  private var cancellables: Set<AnyCancellable> = []
  private var toastTask: Task<Void, Never>?

  init() {
    prefs.$refreshMinutes.removeDuplicates().sink { [weak self] minutes in
      Task { @MainActor in self?.schedule(minutes: minutes) }
    }.store(in: &cancellables)
    prefs.$doneDays.dropFirst().removeDuplicates().sink { [weak self] _ in
      Task { @MainActor in await self?.refresh() }
    }.store(in: &cancellables)
    // A host, path or account switched on or off in Settings: ask again (after typing stops).
    Publishers.MergeMany(
      prefs.$hostname.dropFirst().removeDuplicates().map { _ in () }.eraseToAnyPublisher(),
      prefs.$githubHost.dropFirst().removeDuplicates().map { _ in () }.eraseToAnyPublisher(),
      prefs.$glabPath.dropFirst().removeDuplicates().map { _ in () }.eraseToAnyPublisher(),
      prefs.$ghPath.dropFirst().removeDuplicates().map { _ in () }.eraseToAnyPublisher(),
      prefs.$gitlabEnabled.dropFirst().removeDuplicates().map { _ in () }.eraseToAnyPublisher(),
      prefs.$githubEnabled.dropFirst().removeDuplicates().map { _ in () }.eraseToAnyPublisher()
    )
    .debounce(for: 1, scheduler: RunLoop.main)
    .sink { [weak self] _ in Task { @MainActor in await self?.refresh() } }
    .store(in: &cancellables)
    NotificationCenter.default.addObserver(
      forName: NSApplication.didBecomeActiveNotification, object: nil, queue: .main
    ) { [weak self] _ in
      Task { @MainActor in
        guard let self, !self.isLoading else { return }
        if let last = self.lastRefreshed, Date().timeIntervalSince(last) < 60 { return }
        await self.refresh()
      }
    }
  }

  // MARK: Accounts

  /// The active account's queue, which is what the lists show.
  var open: [MergeRequest] { accounts[forge]?.open ?? [] }
  var done: [MergeRequest] { accounts[forge]?.done ?? [] }
  var me: Person? { accounts[forge]?.me }
  var lastRefreshed: Date? { accounts[forge]?.lastRefreshed }
  var error: String? {
    if prefs.enabledForges.isEmpty { return "Turn on GitLab or GitHub in Settings." }
    return accounts[forge]?.error
  }

  /// The accounts the switcher offers.
  var forges: [Forge] { prefs.enabledForges }

  /// What's waiting on me in one account, for the switcher.
  func waitingCount(_ forge: Forge) -> Int {
    (accounts[forge]?.open ?? []).filter { $0.myState.needsReview && (prefs.showDrafts || !$0.draft) }.count
  }

  /// Shows another account's queue, keeping the selection if it's from there, otherwise moving
  /// to that account's first tab or first item.
  func switchForge(_ forge: Forge) {
    guard forge != self.forge else { return }
    setForge(forge)
    if let selected, selected.kind == forge { return }
    selectedID = tabItems.first { $0.kind == forge }?.id ?? visibleItems.first?.id
  }

  private func setForge(_ forge: Forge) {
    self.forge = forge
    if remembersSeen { prefs.activeForge = forge }
    authorFilter = nil
  }

  /// Every account's merge requests, for finding tabs and the selection whichever account they're in.
  private var everyItem: [MergeRequest] {
    Forge.allCases.flatMap { (accounts[$0]?.open ?? []) + (accounts[$0]?.done ?? []) }
  }

  var selected: MergeRequest? {
    guard let selectedID else { return nil }
    return everyItem.first { $0.id == selectedID }
  }

  // MARK: Lists

  func items(for mode: QueueMode) -> [MergeRequest] {
    switch mode {
    case .toReview:
      return sorted(open.filter { $0.myState.needsReview && (prefs.showDrafts || !$0.draft) })
    case .reviewed:
      return sorted(open.filter { !$0.myState.needsReview })
    case .done:
      return done.sorted { prefs.oldestFirst ? finishedAt($0) < finishedAt($1) : finishedAt($0) > finishedAt($1) }
    }
  }

  /// The current list, filtered and grouped by when I was asked (or when it finished, for Done).
  var sections: [QueueSection] {
    let query = filter.trimmingCharacters(in: .whitespaces).lowercased()
    let items = items(for: mode).filter {
      (query.isEmpty || matches($0, query)) && (authorFilter == nil || $0.author.username == authorFilter)
    }
    var order: [String] = []
    var groups: [String: [MergeRequest]] = [:]
    for item in items {
      let bucket = Self.bucket(for: mode == .done ? finishedAt(item) : item.requestedAt)
      if groups[bucket] == nil { order.append(bucket) }
      groups[bucket, default: []].append(item)
    }
    return order.map { QueueSection(title: $0, items: groups[$0]!) }
  }

  var visibleItems: [MergeRequest] { sections.flatMap(\.items) }

  func count(for mode: QueueMode) -> Int { items(for: mode).count }

  /// Everyone with something in the current list, most merge requests first.
  var authors: [(person: Person, count: Int)] {
    var counts: [String: (person: Person, count: Int)] = [:]
    for item in items(for: mode) {
      counts[item.author.username, default: (item.author, 0)].count += 1
    }
    return counts.values.sorted { $0.count != $1.count ? $0.count > $1.count : $0.person.name < $1.person.name }
  }

  func isUnread(_ item: MergeRequest) -> Bool {
    guard item.isOpen else { return false }
    guard let seen = lastSeen[item.id] else { return true }
    return item.updatedAt > seen
  }

  func finishedAt(_ item: MergeRequest) -> Date { item.mergedAt ?? item.closedAt ?? item.updatedAt }

  private func sorted(_ items: [MergeRequest]) -> [MergeRequest] {
    items.sorted { prefs.oldestFirst ? $0.requestedAt < $1.requestedAt : $0.requestedAt > $1.requestedAt }
  }

  private func matches(_ item: MergeRequest, _ query: String) -> Bool {
    let haystack = [
      item.title, item.fullReference, item.author.name, item.author.username, item.sourceBranch,
      item.requestedBy?.name ?? "",
    ] + item.labels.map(\.title)
    return query.split(separator: " ").allSatisfy { word in
      haystack.contains { $0.lowercased().contains(word) }
    }
  }

  private static func bucket(for date: Date) -> String {
    let calendar = Calendar.current
    if calendar.isDateInToday(date) { return "Today" }
    if calendar.isDateInYesterday(date) { return "Yesterday" }
    let days = calendar.dateComponents([.day], from: date, to: Date()).day ?? 0
    if days < 7 { return "This week" }
    if days < 30 { return "This month" }
    return "Older"
  }

  // MARK: Selection

  func select(offset: Int) {
    let items = visibleItems
    guard !items.isEmpty else { return }
    guard let current = selectedID, let index = items.firstIndex(where: { $0.id == current }) else {
      selectedID = items.first?.id
      return
    }
    selectedID = items[max(0, min(items.count - 1, index + offset))].id
  }

  // MARK: Tabs

  var tabItems: [MergeRequest] {
    let items = everyItem
    return tabs.compactMap { id in items.first { $0.id == id } }
  }

  /// Keeps a merge request in a tab, at the end of the queue; `activate` also shows it.
  func openTab(_ item: MergeRequest, activate: Bool = true) {
    if !tabs.contains(item.id) { tabs.append(item.id) }
    if activate { selectedID = item.id }
  }

  /// Closes a tab, or the preview, moving on to the next merge request in the queue.
  func closeTab(_ id: String) {
    let index = tabs.firstIndex(of: id)
    if let index { tabs.remove(at: index) }
    guard selectedID == id else { return }
    if tabs.isEmpty {
      selectedID = nil
    } else {
      selectedID = tabs[min(index ?? tabs.count, tabs.count - 1)]
    }
  }

  func closeOtherTabs(than id: String) {
    tabs = tabs.contains(id) ? [id] : []
    selectedID = id
  }

  func moveTab(_ id: String, before target: String) {
    guard id != target, let from = tabs.firstIndex(of: id) else { return }
    tabs.remove(at: from)
    tabs.insert(id, at: tabs.firstIndex(of: target) ?? tabs.count)
  }

  /// The next or previous tab; from a preview, the queue's first or last.
  func selectTab(offset: Int) {
    guard !tabs.isEmpty else { return }
    guard let current = selectedID, let index = tabs.firstIndex(of: current) else {
      selectedID = offset > 0 ? tabs.first : tabs.last
      return
    }
    selectedID = tabs[(index + offset + tabs.count) % tabs.count]
  }

  func setMode(_ mode: QueueMode) {
    self.mode = mode
    if let selectedID, visibleItems.contains(where: { $0.id == selectedID }) { return }
    selectedID = visibleItems.first?.id
  }

  private func markSeen(_ item: MergeRequest) {
    lastSeen[item.id] = max(Date(), item.updatedAt)
    // Forget merge requests that have left every list, so the store doesn't grow forever.
    let live = Set(everyItem.map(\.id))
    if !live.isEmpty { lastSeen = lastSeen.filter { live.contains($0.key) } }
    if remembersSeen { prefs.lastSeen = lastSeen }
  }

  // MARK: Viewed files

  enum Viewed {
    case no, yes
    /// Marked viewed, but the file has changed since.
    case changed
  }

  func viewed(_ file: FileDiff, in item: MergeRequest) -> Viewed {
    guard let digest = viewedFiles[item.id]?[file.id] else { return .no }
    return digest == file.digest ? .yes : .changed
  }

  func setViewed(_ on: Bool, _ file: FileDiff, in item: MergeRequest) {
    viewedFiles[item.id, default: [:]][file.id] = on ? file.digest : nil
    if viewedFiles[item.id]?.isEmpty == true { viewedFiles[item.id] = nil }
    // Forget merge requests that have left every list.
    let live = Set(everyItem.map(\.id))
    if !live.isEmpty { viewedFiles = viewedFiles.filter { live.contains($0.key) } }
    if remembersSeen { prefs.viewedFiles = viewedFiles }
  }

  func markAllSeen() {
    for item in open { lastSeen[item.id] = max(Date(), item.updatedAt) }
    prefs.lastSeen = lastSeen
  }

  // MARK: Loading

  private var refreshTask: Task<Void, Never>?
  private var refreshAgain = false

  /// Refreshes the queue. A call made while one is running waits for another full pass, so a
  /// caller (say, after approving) always sees data fetched after it asked.
  func refresh() async {
    guard !offline else { return }
    if let running = refreshTask {
      refreshAgain = true
      await running.value
      return
    }
    let task = Task {
      repeat {
        refreshAgain = false
        await refreshOnce()
      } while refreshAgain
    }
    refreshTask = task
    await task.value
    refreshTask = nil
  }

  private func refreshOnce() async {
    isLoading = true
    defer { isLoading = false }
    let forges = prefs.enabledForges
    accounts = accounts.filter { forges.contains($0.key) }
    if !forges.contains(forge), let first = forges.first { setForge(first) }
    let since = Calendar.current.date(byAdding: .day, value: -prefs.doneDays, to: Date())!
    let results = await withTaskGroup(of: (Forge, Result<QueueResult, Error>).self) { group in
      for forge in forges {
        group.addTask {
          do { return (forge, .success(try await forge.provider.queue(doneSince: since))) } catch {
            return (forge, .failure(error))
          }
        }
      }
      var results: [(Forge, Result<QueueResult, Error>)] = []
      for await result in group { results.append(result) }
      return results
    }
    // An account that fails keeps what it had, marked offline.
    for (forge, result) in results {
      var account = accounts[forge] ?? Account()
      switch result {
      case .success(let queue):
        account.me = queue.me
        account.open = queue.open
        account.done = queue.done
        account.error = nil
        account.lastRefreshed = Date()
      case .failure(let error):
        account.error = error.localizedDescription
      }
      accounts[forge] = account
    }
    guard results.contains(where: { if case .success = $0.1 { true } else { false } }) else { return }

    // A merge request that's left every list (merged long ago) closes its tab, unless an account
    // that never loaded might still have it.
    let everyAccountLoaded = forges.allSatisfy { accounts[$0]?.lastRefreshed != nil }
    let known = Set(everyItem.map(\.id))
    if everyAccountLoaded, tabs.contains(where: { !known.contains($0) }) { tabs.removeAll { !known.contains($0) } }
    if selectedID == nil || selected == nil {
      selectedID = tabItems.first { $0.kind == forge }?.id ?? visibleItems.first?.id
    }
    // What's on screen has been seen, including the update my own approval or comment made.
    if let selected { markSeen(selected) }
    // Changes and threads may have moved on; fetch them again when next shown.
    diffs = diffs.filter { $0.key == selectedID }
    discussions = discussions.filter { $0.key == selectedID }
    if let selected { reloadDetails(for: selected) }
    updateBadge()
    notifyAboutNewRequests()
  }

  // MARK: Details

  func loadDetailsIfNeeded(for item: MergeRequest) {
    guard !offline else { return }
    if diffs[item.id] == nil { loadDiffs(item) }
    if discussions[item.id] == nil { loadDiscussions(item) }
  }

  private func reloadDetails(for item: MergeRequest) {
    loadDiffs(item)
    loadDiscussions(item)
  }

  /// Bumped per request, so a slow older response can't overwrite a newer one.
  private var requests: [String: Int] = [:]

  private func startRequest(_ key: String) -> Int {
    requests[key, default: 0] += 1
    return requests[key]!
  }

  private func isLatest(_ key: String, _ number: Int) -> Bool { requests[key] == number }

  private func loadDiffs(_ item: MergeRequest) {
    guard !offline else { return }
    if diffs[item.id]?.value == nil { diffs[item.id] = .loading }
    let number = startRequest("diffs " + item.id)
    Task {
      do {
        let (files, refs) = try await item.kind.provider.diffs(item)
        guard isLatest("diffs " + item.id, number) else { return }
        diffRefs[item.id] = refs
        diffs[item.id] = .loaded(files)
      } catch {
        guard isLatest("diffs " + item.id, number) else { return }
        diffs[item.id] = .failed(error.localizedDescription)
      }
    }
  }

  private func loadDiscussions(_ item: MergeRequest) {
    guard !offline else { return }
    if discussions[item.id]?.value == nil { discussions[item.id] = .loading }
    let number = startRequest("threads " + item.id)
    Task {
      do {
        let threads = try await item.kind.provider.discussions(item)
        guard isLatest("threads " + item.id, number) else { return }
        discussions[item.id] = .loaded(threads.filter { !$0.notes.allSatisfy(\.system) })
      } catch {
        guard isLatest("threads " + item.id, number) else { return }
        discussions[item.id] = .failed(error.localizedDescription)
      }
    }
  }

  // MARK: Actions

  func approve(_ item: MergeRequest) async {
    await perform("Approving…", done: "Approved \(item.shortReference)") {
      try await item.kind.provider.approve(item)
    }
  }

  func unapprove(_ item: MergeRequest) async {
    await perform("Revoking approval…", done: "Approval revoked") {
      try await item.kind.provider.unapprove(item)
    }
  }

  /// Posts a comment on the merge request. Returns whether it went through.
  func comment(on item: MergeRequest, body: String) async -> Bool {
    await perform("Posting comment…", done: "Comment posted") {
      try await item.kind.provider.comment(on: item, body: body)
    }
  }

  /// Starts a thread on one line of a file's diff, against the commits that diff was taken
  /// between (`refs`, kept from when the box was opened). Returns whether it went through.
  func comment(on item: MergeRequest, file: FileDiff, line: DiffLine, refs: DiffRefs?, body: String) async -> Bool {
    guard let refs else {
      show("The changes moved while loading. Refresh, then comment on the line again.")
      return false
    }
    return await perform("Posting comment…", done: "Comment posted", refreshing: false) {
      try await item.kind.provider.comment(on: item, file: file, line: line, refs: refs, body: body)
      loadDiscussions(item)
    }
  }

  /// Replies to a thread. Returns whether it went through.
  func reply(on item: MergeRequest, to thread: Discussion, body: String) async -> Bool {
    await perform("Posting reply…", done: "Reply posted", refreshing: false) {
      try await item.kind.provider.reply(on: item, to: thread, body: body)
      loadDiscussions(item)
    }
  }

  @discardableResult
  private func perform(
    _ label: String, done message: String, refreshing: Bool = true, _ work: () async throws -> Void
  ) async -> Bool {
    guard !offline else { return false }
    busy = label
    defer { busy = nil }
    do {
      try await work()
      show(message)
      if refreshing { await refresh() }
      return true
    } catch {
      show(error.localizedDescription)
      return false
    }
  }

  func show(_ message: String) {
    toast = message
    toastTask?.cancel()
    toastTask = Task {
      try? await Task.sleep(for: .seconds(3))
      if !Task.isCancelled { toast = nil }
    }
  }

  // MARK: Background

  private func schedule(minutes: Int) {
    timer?.invalidate()
    guard minutes > 0 else { return }
    timer = Timer.scheduledTimer(withTimeInterval: TimeInterval(minutes * 60), repeats: true) { [weak self] _ in
      Task { @MainActor in await self?.refresh() }
    }
  }

  private func updateBadge() {
    let waiting = accounts.values.flatMap(\.open).filter { $0.myState.needsReview && !$0.draft }.count
    NSApp.dockTile.badgeLabel = waiting > 0 ? "\(waiting)" : nil
  }

  /// A notification for each review request that wasn't in the queue last time, or that was
  /// asked again since.
  private func notifyAboutNewRequests() {
    let waiting = accounts.values.flatMap(\.open).filter { $0.myState.needsReview }
    // Stored as "id@seconds", so a re-request (a later time) notifies again.
    var known: [String: Int] = [:]
    for entry in prefs.knownRequests {
      guard let at = entry.lastIndex(of: "@"), let seconds = Int(entry[entry.index(after: at)...]) else { continue }
      known[String(entry[..<at])] = seconds
    }
    let initialized = UserDefaults.standard.bool(forKey: "knownRequestsInitialized")
    defer {
      prefs.knownRequests = Set(waiting.map { "\($0.id)@\(Int($0.requestedAt.timeIntervalSince1970))" })
      UserDefaults.standard.set(true, forKey: "knownRequestsInitialized")
    }
    // The first run knows nothing yet; an empty queue later is still a known state.
    guard initialized, prefs.notify, Bundle.main.bundleIdentifier != nil else { return }
    let fresh = waiting.filter { item in
      guard let seconds = known[item.id] else { return true }
      // A failed note lookup can swap the request time for the creation time; that's no new ask.
      return !item.requestedAtIsGuess && Int(item.requestedAt.timeIntervalSince1970) > seconds + 60
    }
    guard !fresh.isEmpty else { return }
    let center = UNUserNotificationCenter.current()
    for item in fresh.prefix(5) {
      let content = UNMutableNotificationContent()
      content.title = "\(item.requestedBy?.name ?? item.author.name) requested your review"
      content.subtitle = item.shortReference
      content.body = item.title
      content.userInfo = ["id": item.id]
      center.add(UNNotificationRequest(identifier: item.id, content: content, trigger: nil))
    }
  }
}

extension ReviewStore {
  /// Loads a recorded (GitLab) queue and its details instead of asking a forge (`--bench`).
  func load(
    me: Person, open: [MergeRequest], done: [MergeRequest], diffs: [String: [FileDiff]],
    discussions: [String: [Discussion]]
  ) {
    offline = true
    remembersSeen = false
    forge = .gitlab
    accounts = [.gitlab: Account(me: me, open: open, done: done, lastRefreshed: Date())]
    self.diffs = diffs.mapValues { .loaded($0) }
    self.discussions = discussions.mapValues { .loaded($0) }
  }
}
