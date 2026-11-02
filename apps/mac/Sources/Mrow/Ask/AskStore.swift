import AppKit
import Combine

/// One entry in a chat.
struct ChatEntry: Identifiable, Equatable {
  enum Kind: Equatable {
    case question
    case answer
    case thought
    /// A tool the agent ran, by ACP's kind ("read", "search"…). `done` is nil while it runs.
    case tool(kind: String, done: Bool?)
    /// What the first question carried: "the description, 41 changed files…".
    case context
    /// A request of the agent's that Mrow turned down.
    case declined
    /// "Stopped", "cut off at the model's limit": a quiet line after an answer.
    case note
    case failure
  }

  let id: Int
  var kind: Kind
  var text: String
  /// For a tool call, the agent's id for it, so later updates find it.
  var toolID: String?
  let started: Date
  /// When it last grew or finished, for "Thought for 12s".
  var finished: Date
}

/// A conversation with Cursor's agent about one merge request.
@MainActor final class Chat: ObservableObject, Identifiable {
  enum Phase: Equatable {
    case idle
    case starting
    case working
  }

  let id: String
  @Published fileprivate(set) var entries: [ChatEntry] = []
  @Published fileprivate(set) var phase: Phase = .idle
  @Published var draft = ""
  /// The clone the agent reads, or nil when there isn't one.
  @Published fileprivate(set) var checkout: URL?
  @Published fileprivate(set) var checkoutLooked = false
  /// When the question being answered was asked, for the working line's timer.
  @Published fileprivate(set) var turnStarted: Date?
  /// Reopening a chat from an earlier launch, while the agent replays it.
  @Published fileprivate(set) var reopening = false

  fileprivate var sessionID: String?
  fileprivate var sharedContext = false
  private var nextID = 0

  init(id: String) { self.id = id }

  @discardableResult
  fileprivate func append(_ kind: ChatEntry.Kind, _ text: String, toolID: String? = nil) -> Int {
    nextID += 1
    let now = Date()
    entries.append(ChatEntry(id: nextID, kind: kind, text: text, toolID: toolID, started: now, finished: now))
    return nextID
  }

  fileprivate func setText(_ text: String, of id: Int) {
    if let index = entries.firstIndex(where: { $0.id == id }) { entries[index].text = text }
  }

  fileprivate func remove(_ id: Int) {
    entries.removeAll { $0.id == id }
  }

  /// Streamed text joins the entry before it when that's the same kind.
  fileprivate func stream(_ kind: ChatEntry.Kind, _ text: String) {
    if let last = entries.indices.last, entries[last].kind == kind, kind == .answer || kind == .thought || kind == .question {
      entries[last].text += text
      entries[last].finished = Date()
    } else {
      append(kind, text)
    }
  }

  fileprivate func updateTool(_ toolID: String, title: String?, done: Bool?) {
    guard let index = entries.lastIndex(where: { $0.toolID == toolID }),
      case .tool(let kind, let wasDone) = entries[index].kind
    else { return }
    if let title, !title.isEmpty { entries[index].text = title }
    if done != nil { entries[index].finished = Date() }
    // A declined call stays marked declined, though the agent reports it finished.
    entries[index].kind = .tool(kind: kind, done: wasDone == false ? false : done ?? wasDone)
  }

  fileprivate func trimContext() {
    for index in entries.indices where entries[index].kind == .question {
      guard let end = entries[index].text.range(of: AskContext.end) else { continue }
      entries[index].text = String(entries[index].text[end.upperBound...]).trimmingCharacters(in: .whitespacesAndNewlines)
      nextID += 1
      let at = entries[index].started
      entries.insert(ChatEntry(id: nextID, kind: .context, text: "This merge request", started: at, finished: at), at: index)
      break
    }
  }

  fileprivate func reset() {
    entries = []
    sessionID = nil
    sharedContext = false
    phase = .idle
    turnStarted = nil
  }
}

/// The chats, the agent's models, and the plumbing between them and `ACPClient`.
@MainActor final class AskStore: ObservableObject {
  static let shared = AskStore()

  /// Cursor's models, kept between launches so the picker has them before any chat starts.
  @Published private(set) var models: [AgentModel] = []
  @Published private(set) var loadingModels = false
  @Published private(set) var modelsError: String?
  /// The chosen model by base name ("claude-sonnet-5-5"): the last one picked, else Sonnet 5.5.
  @Published private(set) var model: String
  /// The last few picked, newest first.
  @Published private(set) var recentModels: [String]
  /// Each model's effort setting as Cursor offers it, learned from any session on that model.
  @Published private(set) var efforts: [String: EffortOptions]
  /// The effort picked for each model; a model without one uses what Cursor last used.
  @Published private(set) var chosenEfforts: [String: String]

  static let defaultModel = "claude-sonnet-5-5"
  private var modelsFetched: Date?
  /// A session in the empty folder for listing models and asking a model's effort options.
  private var catalogSession: String?

  /// Off for `--ask`, so a test chat isn't reopened in the app.
  var remembersSessions = true

  private var chats: [String: Chat] = [:]
  private var bySession: [String: Chat] = [:]
  /// Streamed chunks waiting to be shown; flushed together a few times a second.
  private var pending: [(Chat, ChatEntry.Kind, String)] = []
  private var flushScheduled = false
  private let client = ACPClient.shared

  /// Chats kept across launches, by merge request, so a question can be followed up tomorrow.
  private var savedSessions: [String: String] {
    get { remembersSessions ? UserDefaults.standard.dictionary(forKey: "askSessions") as? [String: String] ?? [:] : [:] }
    set { if remembersSessions { UserDefaults.standard.set(newValue, forKey: "askSessions") } }
  }

  init() {
    let defaults = UserDefaults.standard
    // Earlier versions saved a full id, or "" for Cursor's default.
    let saved = AgentModel.base(defaults.string(forKey: "askModel") ?? "")
    model = saved.isEmpty ? Self.defaultModel : saved
    recentModels = defaults.stringArray(forKey: "askRecentModels") ?? []
    // A list from before the parameterized one has variant ids ("…[effort=high]"); fetch it again.
    if let data = defaults.data(forKey: "askModels"), let cached = try? JSONDecoder().decode([AgentModel].self, from: data),
      !cached.contains(where: { $0.id.contains("[") })
    {
      models = cached
      modelsFetched = defaults.object(forKey: "askModelsFetched") as? Date
    }
    efforts = defaults.data(forKey: "askEffortOptions").flatMap { try? JSONDecoder().decode([String: EffortOptions].self, from: $0) } ?? [:]
    chosenEfforts = defaults.dictionary(forKey: "askEfforts") as? [String: String] ?? [Self.defaultModel: "medium"]
    client.onUpdate = { [weak self] session, update in self?.received(session: session, update: update) }
  }

  func chat(for item: MergeRequest) -> Chat {
    if let chat = chats[item.id] { return chat }
    let chat = Chat(id: item.id)
    chats[item.id] = chat
    return chat
  }

  /// Finds the clone (off the main thread: it reads a few folders) and reopens a saved chat.
  func prepare(_ chat: Chat, for item: MergeRequest) async {
    guard !chat.checkoutLooked else { return }
    let project = item.projectPath
    chat.checkout = await Task.detached { Checkouts.find(project: project) }.value
    chat.checkoutLooked = true
    if let saved = savedSessions[item.id], chat.sessionID == nil, chat.entries.isEmpty {
      await resume(chat, session: saved)
    }
  }

  func choose(_ folder: URL, for chat: Chat, item: MergeRequest) {
    Checkouts.remember(folder, for: item.projectPath)
    chat.checkout = folder
    newChat(chat, item: item)
  }

  func newChat(_ chat: Chat, item: MergeRequest) {
    if let session = chat.sessionID {
      client.notify("session/cancel", ["sessionId": session])
      bySession[session] = nil
    }
    chat.reset()
    savedSessions[item.id] = nil
  }

  func stop(_ chat: Chat) {
    guard let session = chat.sessionID, chat.phase == .working else { return }
    client.notify("session/cancel", ["sessionId": session])
  }

  /// Asks a question. The first one in a chat carries the merge request with it.
  func ask(_ question: String, in chat: Chat, item: MergeRequest, store: ReviewStore) async {
    let question = question.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !question.isEmpty, chat.phase == .idle else { return }
    chat.draft = ""
    chat.append(.question, question)
    // What the first question carries shows with it at once, its counts filled in once known,
    // so the question doesn't shift when they arrive.
    let contextID = chat.sharedContext ? nil : chat.append(.context, "Description, diff and threads")
    chat.turnStarted = Date()
    chat.phase = .starting
    do {
      if chat.sessionID == nil { try await startSession(chat, item: item) }
      guard let session = chat.sessionID else { return }
      var prompt: [[String: Any]] = []
      if !chat.sharedContext {
        let (files, threads) = await details(for: item, store: store)
        let context = AskContext.build(item, files: files, threads: threads, checkout: chat.checkout)
        prompt.append(["type": "text", "text": context.text])
        chat.sharedContext = true
        var shared = "Description, \(context.files) changed file\(context.files == 1 ? "" : "s")"
        if context.filesLeftOut > 0 { shared += " (\(context.filesLeftOut) left out for size)" }
        shared += ", \(context.threads) thread\(context.threads == 1 ? "" : "s")"
        if let contextID { chat.setText(shared, of: contextID) }
      }
      prompt.append(["type": "text", "text": question])
      chat.phase = .working
      let result = try await client.request("session/prompt", ["sessionId": session, "prompt": prompt])
      flush()
      switch result.string("stopReason") {
      case "cancelled": chat.append(.note, "Stopped")
      case "refusal": chat.append(.failure, "The model declined to answer.")
      case "max_tokens", "max_turn_requests": chat.append(.note, "The answer was cut off at the model's limit.")
      default: break
      }
    } catch {
      flush()
      // Nothing was shared if it failed before the prompt went.
      if let contextID, !chat.sharedContext { chat.remove(contextID) }
      chat.append(.failure, error.localizedDescription)
      // A chat that never started, or whose agent stopped, starts afresh with the next question.
      if chat.phase == .starting || error.localizedDescription.hasPrefix("Cursor's agent stopped") {
        if let session = chat.sessionID { bySession[session] = nil }
        chat.sessionID = nil
        chat.sharedContext = false
      }
    }
    chat.phase = .idle
    chat.turnStarted = nil
  }

  private func startSession(_ chat: Chat, item: MergeRequest) async throws {
    let folder = chat.checkout ?? Self.emptyFolder
    let reply = try await client.request("session/new", ["cwd": folder.path, "mcpServers": []])
    guard let session = reply.string("sessionId") else { throw ACPClient.Failure(message: "The agent didn't start a chat.") }
    chat.sessionID = session
    bySession[session] = chat
    savedSessions[item.id] = session
    keepModels(from: reply)
    try await configure(session, options: reply.objects("configOptions"))
  }

  /// Read-only "ask" mode, and the chosen model and effort.
  private func configure(_ session: String, options: [JSONObject]) async throws {
    _ = try await client.request("session/set_mode", ["sessionId": session, "modeId": "ask"])
    await applyModel(to: session, options: options)
  }

  /// Puts a session on the chosen model, then that model's chosen effort. A failure leaves the
  /// session on Cursor's choice rather than failing the question.
  private func applyModel(to session: String, options: [JSONObject]) async {
    var options = options
    let model = model
    let current = options.first { $0.string("id") == "model" }?.string("currentValue")
    if current != model, models.contains(where: { $0.id == model }) {
      guard let reply = try? await setOption(session, "model", model) else { return }
      options = reply
    }
    absorb(options)
    guard let chosen = chosenEfforts[model], let effort = efforts[model], effort.current != chosen,
      effort.option(chosen) != nil
    else { return }
    if let reply = try? await setOption(session, effort.configID, chosen) { absorb(reply) }
  }

  private func setOption(_ session: String, _ id: String, _ value: String) async throws -> [JSONObject] {
    try await client.request("session/set_config_option", ["sessionId": session, "configId": id, "value": value])
      .objects("configOptions")
  }

  /// Notes a model's effort options from a session's settings.
  private func absorb(_ options: [JSONObject]) {
    guard let (model, effort) = EffortOptions.from(options), efforts[model] != effort else { return }
    efforts[model] = effort
    UserDefaults.standard.set(try? JSONEncoder().encode(efforts), forKey: "askEffortOptions")
  }

  // MARK: Models

  /// The chosen model as Cursor lists it now, or nil before the list has loaded.
  var chosenModel: AgentModel? { models.first { $0.id == model } }

  /// The effort a model will run at: the one picked for it, else the one Cursor last used.
  func effort(for model: String) -> EffortOptions.Option? {
    guard let options = efforts[model] else { return nil }
    return options.option(chosenEfforts[model]) ?? options.option(options.current)
  }

  /// Picks the model for this chat (from its next answer) and for new ones.
  func choose(model chosen: AgentModel, for chat: Chat?) {
    model = chosen.id
    recentModels = Array(([chosen.id] + recentModels.filter { $0 != chosen.id }).prefix(4))
    UserDefaults.standard.set(model, forKey: "askModel")
    UserDefaults.standard.set(recentModels, forKey: "askRecentModels")
    if let session = chat?.sessionID {
      Task { await applyModel(to: session, options: []) }
    } else {
      Task { await describe(chosen.id) }
    }
  }

  /// Picks the chosen model's effort, for this chat and the model's next ones.
  func choose(effort value: String, for chat: Chat?) {
    chosenEfforts[model] = value
    UserDefaults.standard.set(chosenEfforts, forKey: "askEfforts")
    guard let session = chat?.sessionID, let effort = efforts[model] else { return }
    Task { if let reply = try? await setOption(session, effort.configID, value) { absorb(reply) } }
  }

  /// Learns a model's effort options (Cursor only says for the model a session is on) by moving
  /// the catalog session to it.
  func describe(_ model: String) async {
    guard efforts[model] == nil, models.contains(where: { $0.id == model }) else { return }
    for _ in 0..<2 {
      do {
        if catalogSession == nil {
          let reply = try await client.request("session/new", ["cwd": Self.emptyFolder.path, "mcpServers": []])
          catalogSession = reply.string("sessionId")
          keepModels(from: reply)
          absorb(reply.objects("configOptions"))
        }
        guard efforts[model] == nil, let session = catalogSession else { return }
        absorb(try await setOption(session, "model", model))
        return
      } catch {
        // The agent may have restarted since; its sessions went with it.
        catalogSession = nil
      }
    }
  }

  /// Fetches the list when there's none or it's a day old. Cursor only lists models for a chat, so
  /// this opens one in the empty folder (not in a clone, so it stays out of your projects'
  /// histories); every real chat refreshes the list too.
  func loadModels(force: Bool = false) async {
    let stale = modelsFetched.map { Date().timeIntervalSince($0) > 86_400 } ?? true
    guard !loadingModels, force || models.isEmpty || stale else { return }
    loadingModels = true
    defer { loadingModels = false }
    do {
      let reply = try await client.request("session/new", ["cwd": Self.emptyFolder.path, "mcpServers": []])
      catalogSession = reply.string("sessionId")
      keepModels(from: reply)
      absorb(reply.objects("configOptions"))
      modelsError = nil
    } catch {
      modelsError = error.localizedDescription
    }
  }

  private func keepModels(from reply: JSONObject) {
    let available = reply.object("models")?.objects("availableModels") ?? []
    let fresh = available.compactMap { model in
      model.string("modelId").map { AgentModel(id: $0, name: model.string("name") ?? $0) }
    }
    guard !fresh.isEmpty else { return }
    if fresh != models { models = fresh }
    modelsFetched = Date()
    UserDefaults.standard.set(try? JSONEncoder().encode(fresh), forKey: "askModels")
    UserDefaults.standard.set(modelsFetched, forKey: "askModelsFetched")
  }

  /// Reopens a chat from an earlier launch; the agent replays it as updates.
  private func resume(_ chat: Chat, session: String) async {
    chat.phase = .starting
    chat.reopening = true
    defer { chat.reopening = false }
    chat.sessionID = session
    bySession[session] = chat
    do {
      let folder = chat.checkout ?? Self.emptyFolder
      let reply = try await client.request("session/load", ["sessionId": session, "cwd": folder.path, "mcpServers": []])
      try await configure(session, options: reply.objects("configOptions"))
      flush()
      // The first question of a saved chat carried the merge request; show only the question.
      chat.sharedContext = true
      chat.trimContext()
    } catch {
      bySession[session] = nil
      chat.sessionID = nil
      chat.entries = []
    }
    chat.phase = .idle
  }

  /// The diff and threads, waiting for them if the merge request was never opened.
  private func details(for item: MergeRequest, store: ReviewStore) async -> ([FileDiff], [Discussion]) {
    store.loadDetailsIfNeeded(for: item)
    for _ in 0..<80 {
      if store.diffs[item.id]?.value != nil, store.discussions[item.id]?.value != nil { break }
      try? await Task.sleep(for: .milliseconds(250))
    }
    return (store.diffs[item.id]?.value ?? [], store.discussions[item.id]?.value ?? [])
  }

  // MARK: Updates

  private func received(session: String, update: JSONObject) {
    guard let chat = bySession[session] else { return }
    let kind = update.string("sessionUpdate")
    let text = update.object("content")?.string("text") ?? ""
    switch kind {
    case "agent_message_chunk": queue(chat, .answer, text)
    case "agent_thought_chunk": queue(chat, .thought, text)
    case "user_message_chunk": queue(chat, .question, text)
    case "tool_call":
      flush()
      chat.append(
        .tool(kind: update.string("kind") ?? "other", done: Self.done(update.string("status"))),
        Self.title(update, checkout: chat.checkout), toolID: update.string("toolCallId"))
    case "tool_call_update":
      flush()
      let title = update.string("title").map { _ in Self.title(update, checkout: chat.checkout) }
      chat.updateTool(update.string("toolCallId") ?? "", title: title, done: Self.done(update.string("status")))
    case "config_option_update":
      absorb(update.objects("configOptions"))
    case "mrow_declined":
      // Sent by ACPClient after it declined a permission request.
      flush()
      let call = update.object("toolCall") ?? JSONObject(value: [:])
      chat.updateTool(call.string("toolCallId") ?? "", title: nil, done: false)
      chat.append(.declined, call.string("kind") == "execute" ? "Declined running a command" : "Declined a change")
    default:
      break
    }
  }

  private func queue(_ chat: Chat, _ kind: ChatEntry.Kind, _ text: String) {
    pending.append((chat, kind, text))
    guard !flushScheduled else { return }
    flushScheduled = true
    // A few redraws a second rather than one per token.
    DispatchQueue.main.asyncAfter(deadline: .now() + 0.05) { [weak self] in
      MainActor.assumeIsolated { self?.flush() }
    }
  }

  private func flush() {
    flushScheduled = false
    let chunks = pending
    pending = []
    for (chat, kind, text) in chunks { chat.stream(kind, text) }
  }

  private static func done(_ status: String?) -> Bool? {
    switch status {
    case "completed": true
    case "failed": false
    default: nil
    }
  }

  /// "Read src/x.ts" rather than the absolute path into the clone.
  static func title(_ update: JSONObject, checkout: URL?) -> String {
    var title = update.string("title") ?? "Working"
    guard let checkout else { return title }
    // The agent names files absolutely or from the home folder.
    let fromHome = (checkout.path as NSString).abbreviatingWithTildeInPath.dropFirst(2)
    for prefix in [checkout.path + "/", fromHome + "/"] where !fromHome.isEmpty {
      title = title.replacingOccurrences(of: prefix, with: "")
    }
    return title
  }

  /// Where a chat runs when there's no clone: an empty folder, so the agent has nothing to read
  /// but the merge request itself.
  private static let emptyFolder: URL = {
    let folder = FileManager.default.urls(for: .cachesDirectory, in: .userDomainMask)[0]
      .appendingPathComponent("Mrow/ask", isDirectory: true)
    try? FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
    return folder
  }()
}
