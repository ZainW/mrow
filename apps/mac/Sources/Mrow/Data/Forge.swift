import Foundation

/// Where review requests come from. Each forge is one "account" in the sidebar's switcher.
enum Forge: String, Codable, CaseIterable, Identifiable, Sendable {
  case gitlab, github

  var id: String { rawValue }

  var name: String {
    switch self {
    case .gitlab: "GitLab"
    case .github: "GitHub"
    }
  }

  /// "merge request", "pull request".
  var noun: String {
    switch self {
    case .gitlab: "merge request"
    case .github: "pull request"
    }
  }

  var pluralNoun: String { noun + "s" }

  /// What goes between the project and the number: "web!42", "web#42".
  var referenceMark: String {
    switch self {
    case .gitlab: "!"
    case .github: "#"
    }
  }

  var cli: CLI {
    switch self {
    case .gitlab: .glab
    case .github: .gh
    }
  }

  /// The default host when Settings leaves it blank.
  var defaultHost: String {
    switch self {
    case .gitlab: "gitlab.com"
    case .github: "github.com"
    }
  }

  var provider: any ReviewProvider {
    switch self {
    case .gitlab: GitLabProvider()
    case .github: GitHubProvider()
    }
  }
}

/// One forge's review queue, as fetched.
struct QueueResult: Sendable {
  let me: Person
  let open: [MergeRequest]
  let done: [MergeRequest]
}

/// Everything Mrow asks of a forge. Each one maps its own API onto the shared models, so the
/// views never know which forge a merge request came from beyond its wording.
protocol ReviewProvider: Sendable {
  /// Open review requests, and the ones finished (merged or closed) since `doneSince`.
  func queue(doneSince: Date) async throws -> QueueResult
  /// Every file's diff, with the commits it was taken between (nil when they moved mid-load).
  func diffs(_ item: MergeRequest) async throws -> ([FileDiff], DiffRefs?)
  /// Comment threads, oldest first.
  func discussions(_ item: MergeRequest) async throws -> [Discussion]
  func approve(_ item: MergeRequest) async throws
  func unapprove(_ item: MergeRequest) async throws
  func comment(on item: MergeRequest, body: String) async throws
  func comment(on item: MergeRequest, file: FileDiff, line: DiffLine, refs: DiffRefs, body: String) async throws
  func reply(on item: MergeRequest, to thread: Discussion, body: String) async throws
}

/// Pages through a REST list 100 at a time.
func pagedREST<T: Decodable>(_ cli: CLI, _ path: String, maxPages: Int) async throws -> [T] {
  var all: [T] = []
  let joiner = path.contains("?") ? "&" : "?"
  for page in 1...maxPages {
    let data = try await cli.api(["\(path)\(joiner)per_page=100&page=\(page)"])
    let items = try JSONCoding.decoder.decode([T].self, from: data)
    all += items
    if items.count < 100 { break }
  }
  return all
}
