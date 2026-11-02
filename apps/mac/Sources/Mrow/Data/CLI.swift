import Foundation
import os

/// A forge's command-line client (`glab` or `gh`), which holds the token in the keychain so this
/// app never sees it. Both run the same way; they differ in name, settings keys and environment.
struct CLI: Sendable {
  struct Failure: LocalizedError {
    let message: String
    var errorDescription: String? { message }
  }

  /// "glab", "gh".
  let name: String
  /// "GitLab", "GitHub", for messages.
  let service: String
  /// UserDefaults keys for the custom path and host from Settings.
  let pathKey: String
  let hostKey: String
  let environment: [String: String]
  let loginCommand: String

  static let glab = CLI(
    name: "glab", service: "GitLab", pathKey: "glabPath", hostKey: "hostname",
    environment: ["GLAB_NO_PROMPT": "1"], loginCommand: "glab auth login")

  static let gh = CLI(
    name: "gh", service: "GitHub", pathKey: "ghPath", hostKey: "githubHost",
    environment: ["GH_PROMPT_DISABLED": "1", "GH_NO_UPDATE_NOTIFIER": "1", "GH_NO_EXTENSION_UPDATE_NOTIFIER": "1"],
    loginCommand: "gh auth login")

  /// Where the CLIs usually live. An app opened from Finder gets a minimal PATH, so these are
  /// checked before asking a login shell.
  private var candidates: [String] {
    ["/opt/homebrew/bin/", "/usr/local/bin/", NSHomeDirectory() + "/.local/bin/", "/run/current-system/sw/bin/"]
      .map { $0 + name }
  }

  /// Found once per CLI; the first fetches run at the same time, hence the lock.
  private static let resolved = OSAllocatedUnfairLock<[String: String]>(initialState: [:])

  /// The CLIs run off the main thread, so settings come from UserDefaults (thread-safe) rather
  /// than from `Preferences`, which the Settings window edits on the main thread.
  private func setting(_ key: String) -> String {
    (UserDefaults.standard.string(forKey: key) ?? "").trimmingCharacters(in: .whitespaces)
  }

  /// The host from Settings, blank for the CLI's default.
  var host: String { setting(hostKey) }

  func executable() throws -> String {
    let custom = setting(pathKey)
    if !custom.isEmpty {
      guard FileManager.default.isExecutableFile(atPath: custom) else {
        throw Failure(message: "\(name) isn't at \(custom). Fix the path in Settings.")
      }
      return custom
    }
    if let known = Self.resolved.withLock({ $0[name] }) { return known }
    if let found = candidates.first(where: FileManager.default.isExecutableFile(atPath:)) {
      Self.resolved.withLock { $0[name] = found }
      return found
    }
    if let found = try? Self.shell("command -v \(name)"), !found.isEmpty,
      FileManager.default.isExecutableFile(atPath: found)
    {
      Self.resolved.withLock { $0[name] = found }
      return found
    }
    throw Failure(message: "Couldn't find \(name). Install it (brew install \(name)) or set its path in Settings.")
  }

  /// `<cli> api …` against the host from Settings, returning the response body.
  func api(_ arguments: [String], input: Data? = nil) async throws -> Data {
    var args = ["api"]
    if !host.isEmpty { args += ["--hostname", host] }
    return try await run(args + arguments, input: input)
  }

  /// A request with a JSON body, sent on stdin: no field parsing, no placeholders expanded inside
  /// it, and no limit on how long a comment can be.
  func send(_ path: String, method: String = "POST", json: [String: Any]) async throws -> Data {
    let body = try JSONSerialization.data(withJSONObject: json)
    return try await api(["-X", method, "-H", "Content-Type: application/json", "--input", "-", path], input: body)
  }

  /// A GraphQL query, with string variables passed as `-f name=value` and numbers as
  /// `-F name=value` (which the CLIs send as JSON numbers).
  func graphQL<T: Decodable>(
    _ query: String, variables: [String: String?] = [:], numbers: [String: Int] = [:], as type: T.Type
  ) async throws -> T {
    var args = ["graphql", "-f", "query=\(query)"]
    for (name, value) in variables.sorted(by: { $0.key < $1.key }) {
      if let value { args += ["-f", "\(name)=\(value)"] }
    }
    for (name, value) in numbers.sorted(by: { $0.key < $1.key }) { args += ["-F", "\(name)=\(value)"] }
    let data = try await api(args)
    let response = try JSONCoding.decoder.decode(GraphQLResponse<T>.self, from: data)
    if let errors = response.errors, !errors.isEmpty, response.data == nil {
      throw Failure(message: errors.map(\.message).joined(separator: "\n"))
    }
    guard let payload = response.data else { throw Failure(message: "\(service) returned no data.") }
    return payload
  }

  /// Longer than any sane request; a stuck CLI (network, keychain prompt) is stopped after this.
  private static let timeout: TimeInterval = 60

  func run(_ arguments: [String], input: Data? = nil) async throws -> Data {
    let path = try executable()
    let extraEnvironment = environment
    let cli = self
    return try await withCheckedThrowingContinuation { continuation in
      DispatchQueue.global(qos: .userInitiated).async {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: path)
        process.arguments = arguments
        var environment = ProcessInfo.processInfo.environment
        let extra = "/opt/homebrew/bin:/usr/local/bin"
        environment["PATH"] = environment["PATH"].map { "\($0):\(extra)" } ?? extra
        environment["NO_COLOR"] = "1"
        environment.merge(extraEnvironment) { _, new in new }
        process.environment = environment
        let output = Pipe()
        let errors = Pipe()
        process.standardOutput = output
        process.standardError = errors
        let stdin = Pipe()
        process.standardInput = input == nil ? FileHandle.nullDevice : stdin
        do {
          try process.run()
        } catch {
          continuation.resume(throwing: Failure(message: "Couldn't run \(cli.name): \(error.localizedDescription)"))
          return
        }
        if let input {
          DispatchQueue.global().async {
            try? stdin.fileHandleForWriting.write(contentsOf: input)
            try? stdin.fileHandleForWriting.close()
          }
        }
        let timedOut = OSAllocatedUnfairLock(initialState: false)
        let watchdog = DispatchWorkItem {
          guard process.isRunning else { return }
          timedOut.withLock { $0 = true }
          process.terminate()
        }
        DispatchQueue.global().asyncAfter(deadline: .now() + Self.timeout, execute: watchdog)
        // Read both pipes before waiting, so a large response can't fill a pipe and stall the CLI.
        var stderrData = Data()
        let group = DispatchGroup()
        group.enter()
        DispatchQueue.global().async {
          stderrData = errors.fileHandleForReading.readDataToEndOfFile()
          group.leave()
        }
        let stdoutData = output.fileHandleForReading.readDataToEndOfFile()
        group.wait()
        process.waitUntilExit()
        watchdog.cancel()
        if timedOut.withLock({ $0 }) {
          continuation.resume(
            throwing: Failure(message: "\(cli.name) didn't answer within a minute, so Mrow stopped it."))
        } else if process.terminationStatus == 0 {
          continuation.resume(returning: stdoutData)
        } else {
          let message = String(decoding: stderrData.isEmpty ? stdoutData : stderrData, as: UTF8.self)
          continuation.resume(throwing: Failure(message: cli.tidy(message, status: process.terminationStatus)))
        }
      }
    }
  }

  /// The CLIs' errors come boxed with ANSI art and blank lines; keep the words.
  private func tidy(_ message: String, status: Int32) -> String {
    let lines = message.split(whereSeparator: \.isNewline)
      .map { $0.trimmingCharacters(in: .whitespaces) }
      .filter { !$0.isEmpty && $0 != "ERROR" }
    if lines.isEmpty { return "\(name) exited with status \(status)." }
    let text = lines.joined(separator: " ")
    // The HTTP status, not any "401" (merge request !401, line 4010…).
    if text.range(
      of: #"\b401\b.{0,3}Unauthorized|Unauthorized|invalid_token|Bad credentials|not logged in|gh auth login"#,
      options: [.regularExpression, .caseInsensitive]) != nil
    {
      return "\(service) rejected \(name)'s token. Run `\(loginCommand)` in a terminal, then refresh."
    }
    return text
  }

  private static func shell(_ command: String) throws -> String {
    let process = Process()
    process.executableURL = URL(fileURLWithPath: "/bin/zsh")
    process.arguments = ["-lc", command]
    let output = Pipe()
    process.standardOutput = output
    process.standardError = FileHandle.nullDevice
    try process.run()
    let data = output.fileHandleForReading.readDataToEndOfFile()
    process.waitUntilExit()
    return String(decoding: data, as: UTF8.self).trimmingCharacters(in: .whitespacesAndNewlines)
  }
}

/// The GitLab CLI, kept under its old name for the call sites that only ever talk to GitLab
/// (recording benchmarks, fetching uploads).
enum Glab {
  typealias Failure = CLI.Failure
  static func executable() throws -> String { try CLI.glab.executable() }
  static func api(_ arguments: [String]) async throws -> Data { try await CLI.glab.api(arguments) }
}

struct GraphQLResponse<T: Decodable>: Decodable {
  struct Error: Decodable { let message: String }
  let data: T?
  let errors: [Error]?
}

enum JSONCoding {
  static let decoder: JSONDecoder = {
    let decoder = JSONDecoder()
    decoder.dateDecodingStrategy = .custom { decoder in
      let container = try decoder.singleValueContainer()
      let string = try container.decode(String.self)
      if let date = parse(string) { return date }
      throw DecodingError.dataCorruptedError(in: container, debugDescription: "Bad date \(string)")
    }
    return decoder
  }()

  /// GraphQL sends whole seconds, REST sends milliseconds.
  static func parse(_ string: String) -> Date? {
    plain.date(from: string) ?? fractional.date(from: string)
  }

  // ISO8601DateFormatter is thread-safe, and making one is far slower than using one.
  nonisolated(unsafe) private static let plain = ISO8601DateFormatter()
  nonisolated(unsafe) private static let fractional: ISO8601DateFormatter = {
    let formatter = ISO8601DateFormatter()
    formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
    return formatter
  }()
}
