import Foundation
import os

/// A JSON value from the agent, passed between threads as-is (it's only read once it arrives).
struct JSONObject: @unchecked Sendable {
  let value: [String: Any]

  subscript(key: String) -> Any? { value[key] }
  func string(_ key: String) -> String? { value[key] as? String }
  func object(_ key: String) -> JSONObject? { (value[key] as? [String: Any]).map(JSONObject.init) }
  func objects(_ key: String) -> [JSONObject] { (value[key] as? [[String: Any]] ?? []).map(JSONObject.init) }
}

/// Cursor's agent, run as `agent acp` and spoken to over the Agent Client Protocol: JSON-RPC, one
/// message per line on its stdin and stdout. One process serves every chat.
///
/// Mrow offers it no file or terminal access of its own, and declines anything it asks permission
/// for, on top of the chats running in Cursor's read-only ask mode.
final class ACPClient: @unchecked Sendable {
  struct Failure: LocalizedError {
    let message: String
    var errorDescription: String? { message }
  }

  static let shared = ACPClient()

  /// Session updates (streamed text, tool calls), delivered on the main thread.
  var onUpdate: (@MainActor (_ sessionID: String, _ update: JSONObject) -> Void)?

  private let lock = OSAllocatedUnfairLock(uncheckedState: State())

  private struct State {
    var process: Process?
    var input: FileHandle?
    var buffer = Data()
    var nextID = 0
    var waiting: [Int: CheckedContinuation<JSONObject, Error>] = [:]
    var starting: Task<Void, Error>?
    var errors = ""
    var declined = 0
  }

  /// How many permission requests have been declined since the agent started (for `--ask`).
  var declined: Int { lock.withLockUnchecked { $0.declined } }

  /// The agent's own description of what it can do, from `initialize`.
  private(set) var capabilities = JSONObject(value: [:])

  // MARK: Requests

  func request(_ method: String, _ params: [String: Any]) async throws -> JSONObject {
    try await start()
    return try await send(method, params)
  }

  func notify(_ method: String, _ params: [String: Any]) {
    write(["jsonrpc": "2.0", "method": method, "params": params])
  }

  private func send(_ method: String, _ params: [String: Any]) async throws -> JSONObject {
    try await withCheckedThrowingContinuation { continuation in
      let id = lock.withLockUnchecked { state -> Int in
        state.nextID += 1
        state.waiting[state.nextID] = continuation
        return state.nextID
      }
      write(["jsonrpc": "2.0", "id": id, "method": method, "params": params])
    }
  }

  private func write(_ message: [String: Any]) {
    guard var data = try? JSONSerialization.data(withJSONObject: message) else { return }
    data.append(0x0A)
    Self.log(">> ", data)
    let input = lock.withLockUnchecked { $0.input }
    try? input?.write(contentsOf: data)
  }

  // MARK: Process

  /// Starts the agent once; later calls wait for the same start.
  private func start() async throws {
    let task = lock.withLockUnchecked { state -> Task<Void, Error> in
      if let starting = state.starting { return starting }
      let task = Task { try await self.launch() }
      state.starting = task
      return task
    }
    do {
      try await task.value
    } catch {
      lock.withLockUnchecked { $0.starting = nil }
      throw error
    }
  }

  private func launch() async throws {
    let path = try Self.executable()
    let process = Process()
    process.executableURL = URL(fileURLWithPath: path)
    process.arguments = ["acp"]
    process.currentDirectoryURL = FileManager.default.homeDirectoryForCurrentUser
    var environment = ProcessInfo.processInfo.environment
    let extra = "/opt/homebrew/bin:/usr/local/bin:" + NSHomeDirectory() + "/.local/bin"
    environment["PATH"] = environment["PATH"].map { "\($0):\(extra)" } ?? extra
    environment["NO_COLOR"] = "1"
    process.environment = environment

    let output = Pipe()
    let input = Pipe()
    let errors = Pipe()
    process.standardOutput = output
    process.standardInput = input
    process.standardError = errors
    output.fileHandleForReading.readabilityHandler = { [weak self] handle in
      let data = handle.availableData
      if data.isEmpty { handle.readabilityHandler = nil } else { self?.received(data) }
    }
    errors.fileHandleForReading.readabilityHandler = { [weak self] handle in
      let data = handle.availableData
      guard !data.isEmpty else { return handle.readabilityHandler = nil }
      // Kept for error messages: the last couple of kilobytes is enough.
      self?.lock.withLockUnchecked { $0.errors = String(($0.errors + String(decoding: data, as: UTF8.self)).suffix(2000)) }
    }
    process.terminationHandler = { [weak self] _ in self?.exited() }
    try process.run()
    lock.withLockUnchecked {
      $0.process = process
      $0.input = input.fileHandleForWriting
    }

    let reply = try await send(
      "initialize",
      [
        "protocolVersion": 1,
        "clientCapabilities": [
          "fs": ["readTextFile": false, "writeTextFile": false], "terminal": false,
          // Plain model ids, with effort (and context, speed) as separate settings. Without it each
          // model is offered at one fixed effort.
          "_meta": ["parameterizedModelPicker": true],
        ],
        "clientInfo": ["name": "Mrow", "version": "1.0"],
      ])
    capabilities = reply.object("agentCapabilities") ?? JSONObject(value: [:])
  }

  private func exited() {
    let (waiting, errors) = lock.withLockUnchecked { state -> ([CheckedContinuation<JSONObject, Error>], String) in
      let ended = (Array(state.waiting.values), state.errors)
      state = State()
      return ended
    }
    let detail = errors.trimmingCharacters(in: .whitespacesAndNewlines)
    let failure = Failure(message: "Cursor's agent stopped." + (detail.isEmpty ? "" : " " + String(detail.suffix(300))))
    for continuation in waiting { continuation.resume(throwing: failure) }
  }

  /// Stops the agent (used when quitting); the next request starts a new one.
  func stop() {
    let process = lock.withLockUnchecked { $0.process }
    process?.terminate()
  }

  // MARK: Messages

  private func received(_ data: Data) {
    let lines = lock.withLockUnchecked { state -> [Data] in
      state.buffer.append(data)
      var lines: [Data] = []
      while let newline = state.buffer.firstIndex(of: 0x0A) {
        lines.append(state.buffer[state.buffer.startIndex..<newline])
        state.buffer.removeSubrange(state.buffer.startIndex...newline)
      }
      return lines
    }
    for line in lines where !line.isEmpty {
      Self.log("<< ", line + [0x0A])
      guard let message = (try? JSONSerialization.jsonObject(with: line)) as? [String: Any] else { continue }
      handle(JSONObject(value: message))
    }
  }

  private func handle(_ message: JSONObject) {
    let method = message.string("method")
    if let id = message["id"] as? Int, method == nil {
      // A reply to one of ours.
      let continuation = lock.withLockUnchecked { $0.waiting.removeValue(forKey: id) }
      if let error = message.object("error") {
        let text = error.string("message") ?? "The agent returned an error."
        continuation?.resume(throwing: Failure(message: Self.explain(text)))
      } else {
        continuation?.resume(returning: message.object("result") ?? JSONObject(value: [:]))
      }
    } else if let id = message["id"], let method {
      answer(id: id, method: method, params: message.object("params"))
    } else if method == "session/update", let params = message.object("params"),
      let session = params.string("sessionId"), let update = params.object("update")
    {
      let handler = onUpdate
      DispatchQueue.main.async { MainActor.assumeIsolated { handler?(session, update) } }
    }
  }

  /// The agent's own requests. Permission is always declined, so nothing it asks to do (edit a
  /// file, run a command) happens; there's nothing else it may ask Mrow for.
  private func answer(id: Any, method: String, params: JSONObject?) {
    if method == "session/request_permission" {
      lock.withLockUnchecked { $0.declined += 1 }
      let options = params?.objects("options") ?? []
      let reject = options.first { ($0.string("kind") ?? "").hasPrefix("reject") }
      let outcome: [String: Any] =
        if let optionID = reject?.string("optionId") { ["outcome": "selected", "optionId": optionID] }
        else { ["outcome": "cancelled"] }
      write(["jsonrpc": "2.0", "id": id, "result": ["outcome": outcome]])
      // Shown in the chat, so it's clear what the agent wanted and didn't get.
      if let session = params?.string("sessionId") {
        let update = JSONObject(value: [
          "sessionUpdate": "mrow_declined", "toolCall": params?["toolCall"] ?? [:],
        ])
        let handler = onUpdate
        DispatchQueue.main.async { MainActor.assumeIsolated { handler?(session, update) } }
      }
    } else {
      write(["jsonrpc": "2.0", "id": id, "error": ["code": -32601, "message": "Mrow doesn't offer \(method)."]])
    }
  }

  /// `MROW_ACP_LOG=<file>` appends every message, both ways, for debugging.
  private static let logFile: FileHandle? = ProcessInfo.processInfo.environment["MROW_ACP_LOG"].flatMap { path in
    FileManager.default.createFile(atPath: path, contents: nil)
    return FileHandle(forWritingAtPath: path)
  }

  private static func log(_ direction: String, _ data: Data) {
    guard let logFile else { return }
    try? logFile.write(contentsOf: Data(direction.utf8) + data)
  }

  private static func explain(_ message: String) -> String {
    if message.range(of: "auth|log ?in|unauthori", options: [.regularExpression, .caseInsensitive]) != nil {
      return "Cursor isn't signed in. Run `agent login` in a terminal, then ask again."
    }
    return message
  }

  // MARK: Finding the agent

  private static func executable() throws -> String {
    let custom = (UserDefaults.standard.string(forKey: "agentPath") ?? "").trimmingCharacters(in: .whitespaces)
    if !custom.isEmpty {
      guard FileManager.default.isExecutableFile(atPath: custom) else {
        throw Failure(message: "Cursor's agent isn't at \(custom). Fix the path in Settings.")
      }
      return custom
    }
    let home = NSHomeDirectory()
    let candidates = [
      home + "/.local/bin/agent", home + "/.local/bin/cursor-agent", "/opt/homebrew/bin/cursor-agent",
      "/usr/local/bin/cursor-agent",
    ]
    if let found = candidates.first(where: FileManager.default.isExecutableFile(atPath:)) { return found }
    throw Failure(
      message: "Couldn't find Cursor's agent. Install it (curl https://cursor.com/install -fsS | bash) or set its path in Settings.")
  }
}
