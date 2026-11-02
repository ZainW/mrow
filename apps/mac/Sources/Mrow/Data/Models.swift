import Foundation

struct Person: Hashable, Codable {
  let username: String
  let name: String
  let avatarUrl: String?

  init(username: String, name: String, avatarUrl: String? = nil) {
    self.username = username
    self.name = name
    self.avatarUrl = avatarUrl
  }

  var initials: String {
    let parts = name.split(separator: " ").prefix(2)
    let letters = parts.compactMap(\.first).map(String.init).joined()
    return letters.isEmpty ? String(username.prefix(1)).uppercased() : letters.uppercased()
  }
}

/// A reviewer's progress, as GitLab reports it per reviewer.
enum ReviewState: String, Codable {
  case unreviewed = "UNREVIEWED"
  case reviewStarted = "REVIEW_STARTED"
  case reviewed = "REVIEWED"
  case requestedChanges = "REQUESTED_CHANGES"
  case approved = "APPROVED"
  /// Approval was reset, usually by new commits, so it needs another look.
  case unapproved = "UNAPPROVED"
  case unknown

  init(from decoder: Decoder) throws {
    let raw = try decoder.singleValueContainer().decode(String.self)
    self = ReviewState(rawValue: raw) ?? .unknown
  }

  /// Whether the ball is in the reviewer's court.
  var needsReview: Bool {
    switch self {
    case .unreviewed, .reviewStarted, .unapproved, .unknown: true
    case .reviewed, .requestedChanges, .approved: false
    }
  }

  var label: String {
    switch self {
    case .unreviewed: "Not reviewed"
    case .reviewStarted: "Reviewing"
    case .reviewed: "Commented"
    case .requestedChanges: "Requested changes"
    case .approved: "Approved"
    case .unapproved: "Approval reset"
    case .unknown: "Unknown"
    }
  }

  var symbol: String {
    switch self {
    case .unreviewed: "circle.dashed"
    case .reviewStarted: "circle.lefthalf.filled"
    case .reviewed: "text.bubble"
    case .requestedChanges: "exclamationmark.circle"
    case .approved: "checkmark.circle.fill"
    case .unapproved: "arrow.uturn.backward.circle"
    case .unknown: "circle"
    }
  }
}

struct Reviewer: Hashable, Codable {
  let person: Person
  let state: ReviewState
  let approved: Bool
}

struct GitLabLabel: Hashable, Codable {
  let title: String
  let color: String
  let textColor: String
}

enum PipelineStatus: String, Codable {
  case success = "SUCCESS", failed = "FAILED", running = "RUNNING", pending = "PENDING"
  case canceled = "CANCELED", skipped = "SKIPPED", manual = "MANUAL", created = "CREATED"
  case preparing = "PREPARING", scheduled = "SCHEDULED", waitingForResource = "WAITING_FOR_RESOURCE"
  case canceling = "CANCELING", waitingForCallback = "WAITING_FOR_CALLBACK"

  var label: String {
    switch self {
    case .success: "Passed"
    case .failed: "Failed"
    case .running, .canceling: "Running"
    case .canceled: "Canceled"
    case .skipped: "Skipped"
    case .manual: "Manual"
    default: "Pending"
    }
  }
}

/// One merge request (or GitHub pull request) I've been asked to review, with the moment I was asked.
struct MergeRequest: Identifiable, Hashable, Codable {
  let id: String
  let iid: String
  let title: String
  let webURL: URL
  let projectPath: String
  let projectName: String
  let author: Person
  let createdAt: Date
  let updatedAt: Date
  let mergedAt: Date?
  let closedAt: Date?
  let state: String
  let draft: Bool
  let sourceBranch: String
  let targetBranch: String
  let mergeStatus: String?
  let commentCount: Int
  let description: String
  let reviewers: [Reviewer]
  let approvedBy: [String]
  let additions: Int
  let deletions: Int
  let fileCount: Int
  let pipeline: PipelineStatus?
  let labels: [GitLabLabel]
  /// The last time someone asked me to review it: a re-request moves it back to the top.
  let requestedAt: Date
  /// The first time I was added, when that differs from the last request.
  let firstRequestedAt: Date
  let requestedBy: Person?
  /// No "requested review from" note was found, so `requestedAt` is the creation date.
  let requestedAtIsGuess: Bool
  let myState: ReviewState
  /// Where it came from; nil in recordings made before GitHub support, which were all GitLab.
  let forge: Forge?

  var kind: Forge { forge ?? .gitlab }
  var isOpen: Bool { state == "opened" }
  var wasReRequested: Bool { requestedAt.timeIntervalSince(firstRequestedAt) > 60 }
  /// "backend-service!5374" on GitLab, "web#42" on GitHub.
  var shortReference: String { "\(projectName)\(kind.referenceMark)\(iid)" }
  var fullReference: String { "\(projectPath)\(kind.referenceMark)\(iid)" }
  /// The project path escaped for REST URLs.
  var projectID: String {
    projectPath.addingPercentEncoding(withAllowedCharacters: .alphanumerics) ?? projectPath
  }

  var mergeStatusLabel: String? {
    switch mergeStatus {
    case "mergeable": "Ready to merge"
    case "conflict": "Has conflicts"
    case "need_rebase": "Needs rebase"
    case "not_approved": "Awaiting approval"
    case "discussions_not_resolved": "Unresolved threads"
    case "ci_must_pass", "ci_still_running": "Waiting on pipeline"
    case "draft_status": "Draft"
    case "blocked_status": "Blocked"
    default: nil
    }
  }
}

// MARK: GraphQL payloads

struct Connection<T: Decodable>: Decodable {
  /// Lists page forwards; activity notes page backwards from the newest.
  struct PageInfo: Decodable {
    let hasNextPage: Bool?
    let endCursor: String?
    let hasPreviousPage: Bool?
    let startCursor: String?
  }

  let pageInfo: PageInfo?
  let nodes: [T]
}

struct QueuePayload: Decodable {
  struct User: Decodable {
    let username: String
    let name: String
    let reviewRequestedMergeRequests: Connection<MergeRequestNode>
  }

  let currentUser: User?
}

struct NotesPayload: Decodable {
  struct Project: Decodable {
    struct MR: Decodable { let notes: Connection<ActivityNote> }
    let mergeRequest: MR?
  }

  let project: Project?
}

struct ActivityNote: Decodable {
  let body: String
  let createdAt: Date
  let author: Person?
}

struct MergeRequestNode: Decodable {
  struct Project: Decodable {
    let fullPath: String
    let name: String
  }

  struct ReviewerNode: Decodable {
    struct Interaction: Decodable {
      let reviewState: ReviewState?
      let approved: Bool?
    }

    let username: String
    let name: String
    let avatarUrl: String?
    let mergeRequestInteraction: Interaction?
  }

  struct Username: Decodable { let username: String }

  struct DiffStats: Decodable {
    let additions: Int
    let deletions: Int
    let fileCount: Int
  }

  struct Pipeline: Decodable { let status: String }

  let id: String
  let iid: String
  let title: String
  let webUrl: String
  let createdAt: Date
  let updatedAt: Date
  let mergedAt: Date?
  let closedAt: Date?
  let state: String
  let draft: Bool
  let sourceBranch: String
  let targetBranch: String
  let detailedMergeStatus: String?
  let userNotesCount: Int?
  let description: String?
  let project: Project
  let author: Person?
  let reviewers: Connection<ReviewerNode>
  let approvedBy: Connection<Username>?
  let diffStatsSummary: DiffStats?
  let headPipeline: Pipeline?
  let labels: Connection<GitLabLabel>?
  let notes: Connection<ActivityNote>
}

// MARK: REST payloads

struct FileDiff: Decodable, Identifiable, Hashable {
  let oldPath: String
  let newPath: String
  let diff: String
  let newFile: Bool
  let renamedFile: Bool
  let deletedFile: Bool
  let tooLarge: Bool?
  let collapsed: Bool?
  let generatedFile: Bool?
  /// Filled in by `prepared()` once, off the main thread, so drawing never re-parses the diff.
  var lines: [DiffLine] = []
  var added = 0
  var removed = 0
  /// Tells two versions of the same file apart, for caching their styled lines and noticing a
  /// file changed after it was marked viewed. Stable across launches, unlike `hashValue`.
  var digest = 0

  var id: String { oldPath + "→" + newPath }

  func prepared() -> FileDiff {
    var copy = self
    copy.lines = DiffLine.parse(diff)
    copy.digest = Self.fnv1a(diff)
    for line in copy.lines {
      if line.kind == .added { copy.added += 1 } else if line.kind == .removed { copy.removed += 1 }
    }
    return copy
  }

  static func fnv1a(_ text: String) -> Int {
    var hash: UInt64 = 0xcbf2_9ce4_8422_2325
    var copy = text
    copy.withUTF8 { bytes in
      for byte in bytes {
        hash ^= UInt64(byte)
        hash &*= 0x100_0000_01b3
      }
    }
    return Int(bitPattern: UInt(hash))
  }

  enum CodingKeys: String, CodingKey {
    case oldPath = "old_path", newPath = "new_path", diff
    case newFile = "new_file", renamedFile = "renamed_file", deletedFile = "deleted_file"
    case tooLarge = "too_large", collapsed, generatedFile = "generated_file"
  }
}

struct DiffRefs: Decodable, Hashable {
  let baseSha: String
  let startSha: String
  let headSha: String

  enum CodingKeys: String, CodingKey {
    case baseSha = "base_sha", startSha = "start_sha", headSha = "head_sha"
  }
}

struct Discussion: Decodable, Identifiable, Hashable {
  let id: String
  let individualNote: Bool
  let notes: [Note]

  enum CodingKeys: String, CodingKey {
    case id, individualNote = "individual_note", notes
  }
}

struct Note: Decodable, Identifiable, Hashable {
  struct Author: Decodable, Hashable {
    let username: String
    let name: String
  }

  struct Position: Decodable, Hashable {
    let newPath: String?
    let oldPath: String?
    let newLine: Int?
    let oldLine: Int?
    /// The commit the line numbers belong to; an older one means the thread is outdated.
    let headSha: String?

    enum CodingKeys: String, CodingKey {
      case newPath = "new_path", oldPath = "old_path", newLine = "new_line", oldLine = "old_line"
      case headSha = "head_sha"
    }
  }

  let id: Int
  let body: String
  let author: Author
  let createdAt: Date
  let system: Bool
  let resolvable: Bool?
  let resolved: Bool?
  let position: Position?

  enum CodingKeys: String, CodingKey {
    case id, body, author, system, resolvable, resolved, position
    case createdAt = "created_at"
  }
}
