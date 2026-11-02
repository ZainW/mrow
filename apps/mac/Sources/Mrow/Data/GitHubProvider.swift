import Foundation

/// GitHub through `gh`: GraphQL searches for the queue, REST for files, comments and reviews,
/// all mapped onto the same models GitLab fills.
///
/// GitHub has no per-reviewer state like GitLab's, so it's pieced together: a pending review
/// request means it's my turn; otherwise my latest review (approved, changes requested,
/// commented, dismissed) says where it stands. "When I was asked" comes from the timeline's
/// review-requested events.
struct GitHubProvider: ReviewProvider {
  private var cli: CLI { .gh }

  // MARK: Queue

  func queue(doneSince since: Date) async throws -> QueueResult {
    let day = Self.day.string(from: since)
    let common = "is:pr archived:false"
    async let requested = search("\(common) is:open review-requested:@me", pages: 5)
    async let reviewed = search("\(common) is:open reviewed-by:@me -author:@me", pages: 3)
    async let closedReviewed = search("\(common) is:closed reviewed-by:@me -author:@me updated:>=\(day)", pages: 2)
    async let closedRequested = search("\(common) is:closed review-requested:@me updated:>=\(day)", pages: 1)
    let (asked, mine, doneA, doneB) = try await (requested, reviewed, closedReviewed, closedRequested)
    guard let viewer = asked.viewer ?? mine.viewer else { throw CLI.Failure(message: "gh isn't signed in.") }
    let me = Person(username: viewer.login, name: viewer.name ?? viewer.login, avatarUrl: viewer.avatarUrl)

    // Asked through a team still counts as asked, though no request names me.
    let askedIDs = Set(asked.nodes.map(\.id))
    let open = Self.unique(asked.nodes + mine.nodes).map {
      Self.build($0, me: me.username, pending: askedIDs.contains($0.id))
    }
    let done = Self.unique(doneA.nodes + doneB.nodes).map { Self.build($0, me: me.username, pending: false) }
    return QueueResult(me: me, open: open, done: done)
  }

  private struct Page {
    var viewer: GitHubSearchPayload.Viewer?
    var nodes: [GitHubPullNode] = []
  }

  private func search(_ query: String, pages: Int) async throws -> Page {
    var page = Page()
    var cursor: String?
    for _ in 0..<pages {
      let payload = try await cli.graphQL(
        GitHubQueries.search, variables: ["q": query, "after": cursor], as: GitHubSearchPayload.self)
      page.viewer = payload.viewer
      // Search results can include issues, and nodes the token can't read come back null.
      page.nodes += payload.search.nodes.compactMap { $0 }.filter { $0.id != nil }.map(\.pull)
      guard payload.search.pageInfo.hasNextPage, let next = payload.search.pageInfo.endCursor else { break }
      cursor = next
    }
    return page
  }

  private static func unique(_ nodes: [GitHubPullNode]) -> [GitHubPullNode] {
    var seen = Set<String>()
    return nodes.filter { seen.insert($0.id).inserted }
  }

  private static let day: DateFormatter = {
    let formatter = DateFormatter()
    formatter.locale = Locale(identifier: "en_US_POSIX")
    formatter.timeZone = TimeZone(identifier: "UTC")
    formatter.dateFormat = "yyyy-MM-dd"
    return formatter
  }()

  static func reviewState(_ raw: String) -> ReviewState {
    switch raw {
    case "APPROVED": .approved
    case "CHANGES_REQUESTED": .requestedChanges
    case "COMMENTED": .reviewed
    case "DISMISSED": .unapproved
    case "PENDING": .reviewStarted
    default: .unknown
    }
  }

  private static func person(_ actor: GitHubActor?) -> Person {
    guard let actor else { return Person(username: "ghost", name: "Unknown") }
    return Person(username: actor.login, name: actor.name ?? actor.login, avatarUrl: actor.avatarUrl)
  }

  static func build(_ node: GitHubPullNode, me: String, pending: Bool) -> MergeRequest {
    let isMe = { (login: String?) in login?.caseInsensitiveCompare(me) == .orderedSame }

    // Reviewers: pending requests first (a re-request supersedes the old review), then reviews.
    var reviewers: [Reviewer] = []
    var listed = Set<String>()
    var requestedFromMe = false
    for request in node.reviewRequests?.nodes ?? [] {
      guard let reviewer = request.requestedReviewer else { continue }
      if let login = reviewer.login {
        if isMe(login) { requestedFromMe = true }
        guard listed.insert(login.lowercased()).inserted else { continue }
        reviewers.append(Reviewer(
          person: Person(username: login, name: reviewer.name ?? login, avatarUrl: reviewer.avatarUrl),
          state: .unreviewed, approved: false))
      } else if let slug = reviewer.slug {
        guard listed.insert("team:" + slug).inserted else { continue }
        reviewers.append(Reviewer(
          person: Person(username: slug, name: reviewer.name ?? slug), state: .unreviewed, approved: false))
      }
    }
    var myReview: ReviewState?
    for review in node.latestReviews?.nodes ?? [] {
      let author = person(review.author)
      let state = reviewState(review.state)
      if isMe(author.username) { myReview = state }
      guard listed.insert(author.username.lowercased()).inserted else { continue }
      reviewers.append(Reviewer(person: author, state: state, approved: state == .approved))
    }

    let myState: ReviewState
    if pending || requestedFromMe {
      // Asked (again): it's my turn, whatever I said last time.
      myState = myReview == .unapproved ? .unapproved : .unreviewed
    } else {
      myState = myReview ?? .unknown
    }

    // When I was asked: requests naming me, or else (asked through a team) any team request.
    let events = (node.timelineItems?.nodes ?? []).compactMap { $0 }.filter { $0.createdAt != nil }
    var requests = events.filter { isMe($0.requestedReviewer?.login) }
    if requests.isEmpty { requests = events.filter { $0.requestedReviewer?.slug != nil } }
    requests.sort { $0.createdAt! < $1.createdAt! }

    let state: String = switch node.state {
    case "MERGED": "merged"
    case "CLOSED": "closed"
    default: "opened"
    }
    let rollup = node.commits?.nodes.last?.commit.statusCheckRollup?.state
    let pipeline: PipelineStatus? = switch rollup {
    case "SUCCESS": .success
    case "FAILURE", "ERROR": .failed
    case "PENDING", "EXPECTED": .running
    default: nil
    }
    let mergeStatus: String? = switch node.mergeStateStatus {
    case "CLEAN", "HAS_HOOKS": "mergeable"
    case "DIRTY": "conflict"
    case "BEHIND": "need_rebase"
    case "BLOCKED": "blocked_status"
    case "DRAFT": "draft_status"
    default: nil
    }
    let labels = (node.labels?.nodes ?? []).map {
      GitLabLabel(title: $0.name, color: "#" + $0.color, textColor: Self.textColor(onHex: $0.color))
    }
    return MergeRequest(
      id: node.id, iid: String(node.number), title: node.title,
      webURL: URL(string: node.url) ?? URL(string: "https://github.com")!,
      projectPath: node.repository.nameWithOwner, projectName: node.repository.name,
      author: person(node.author),
      createdAt: node.createdAt, updatedAt: node.updatedAt, mergedAt: node.mergedAt,
      closedAt: node.closedAt, state: state, draft: node.isDraft,
      sourceBranch: node.headRefName, targetBranch: node.baseRefName,
      mergeStatus: mergeStatus, commentCount: node.totalCommentsCount ?? 0,
      description: node.body ?? "", reviewers: reviewers,
      approvedBy: reviewers.filter(\.approved).map(\.person.username),
      additions: node.additions, deletions: node.deletions, fileCount: node.changedFiles,
      pipeline: pipeline, labels: labels,
      requestedAt: requests.last?.createdAt ?? node.createdAt,
      firstRequestedAt: requests.first?.createdAt ?? node.createdAt,
      requestedBy: requests.last.map { person($0.actor) },
      requestedAtIsGuess: requests.isEmpty,
      myState: myState,
      forge: .github)
  }

  /// GitHub gives a label only its background; pick black or white text by its luminance.
  static func textColor(onHex hex: String) -> String {
    guard hex.count == 6, let value = Int(hex, radix: 16) else { return "#1f2328" }
    let channel = { (shift: Int) -> Double in
      let c = Double((value >> shift) & 0xFF) / 255
      return c <= 0.03928 ? c / 12.92 : pow((c + 0.055) / 1.055, 2.4)
    }
    let luminance = 0.2126 * channel(16) + 0.7152 * channel(8) + 0.0722 * channel(0)
    return luminance > 0.4 ? "#1f2328" : "#ffffff"
  }

  // MARK: Details

  private func base(_ item: MergeRequest) -> String { "repos/\(item.projectPath)/pulls/\(item.iid)" }

  func diffs(_ item: MergeRequest) async throws -> ([FileDiff], DiffRefs?) {
    for _ in 0..<2 {
      let before = try? await refs(item)
      let files = try await files(item)
      let after = try? await refs(item)
      if before == after { return (files, after) }
    }
    return (try await files(item), nil)
  }

  private func files(_ item: MergeRequest) async throws -> [FileDiff] {
    let files: [GitHubFile] = try await pagedREST(cli, "\(base(item))/files", maxPages: 30)
    return files.map { file in
      FileDiff(
        oldPath: file.previousFilename ?? file.filename, newPath: file.filename, diff: file.patch ?? "",
        newFile: file.status == "added", renamedFile: file.status == "renamed",
        deletedFile: file.status == "removed",
        // No patch for a file with changes means GitHub thought it too big (or binary).
        tooLarge: file.patch == nil && file.changes > 0, collapsed: nil, generatedFile: nil
      ).prepared()
    }
  }

  private func refs(_ item: MergeRequest) async throws -> DiffRefs {
    struct Payload: Decodable {
      struct Ref: Decodable { let sha: String }
      let base: Ref
      let head: Ref
    }
    let data = try await cli.api([base(item)])
    let payload = try JSONCoding.decoder.decode(Payload.self, from: data)
    return DiffRefs(baseSha: payload.base.sha, startSha: payload.base.sha, headSha: payload.head.sha)
  }

  /// Conversation comments, review summaries and line threads, in the order they started.
  func discussions(_ item: MergeRequest) async throws -> [Discussion] {
    let repo = "repos/\(item.projectPath)"
    async let issueComments: [GitHubComment] = pagedREST(cli, "\(repo)/issues/\(item.iid)/comments", maxPages: 10)
    async let lineComments: [GitHubComment] = pagedREST(cli, "\(base(item))/comments", maxPages: 10)
    async let reviews: [GitHubReview] = pagedREST(cli, "\(base(item))/reviews", maxPages: 5)
    async let resolution = resolvedThreads(item)
    let (comments, lines, summaries) = try await (issueComments, lineComments, reviews)
    let resolved = await resolution

    var threads: [Discussion] = comments.map {
      Discussion(id: "ic-\($0.id)", individualNote: true, notes: [note($0, position: nil)])
    }
    for review in summaries {
      guard let body = review.body?.trimmingCharacters(in: .whitespacesAndNewlines), !body.isEmpty,
        let at = review.submittedAt
      else { continue }
      let verdict = switch review.state {
      case "APPROVED": "**Approved**\n\n"
      case "CHANGES_REQUESTED": "**Requested changes**\n\n"
      default: ""
      }
      threads.append(Discussion(
        id: "rv-\(review.id)", individualNote: true,
        notes: [Note(
          id: review.id, body: verdict + body,
          author: .init(username: review.user?.login ?? "ghost", name: review.user?.login ?? "Unknown"),
          createdAt: at, system: false, resolvable: nil, resolved: nil, position: nil)]))
    }
    // Replies point at the thread's first comment.
    var order: [Int] = []
    var grouped: [Int: [GitHubComment]] = [:]
    for comment in lines.sorted(by: { $0.createdAt < $1.createdAt }) {
      let root = comment.inReplyToId ?? comment.id
      if grouped[root] == nil { order.append(root) }
      grouped[root, default: []].append(comment)
    }
    for root in order {
      guard let notes = grouped[root], let first = notes.first else { continue }
      let position = Note.Position(
        newPath: first.path, oldPath: first.path,
        newLine: first.line != nil ? (first.side == "LEFT" ? nil : first.line) : first.originalLine,
        oldLine: first.line != nil && first.side == "LEFT" ? first.line : nil,
        // A thread whose line is gone (outdated) is pinned to the commit it was written on,
        // which isn't the head any more, so it isn't placed in the diff.
        headSha: first.line == nil ? (first.originalCommitId ?? "outdated") : nil)
      let isResolved = resolved?[root]
      threads.append(Discussion(
        id: "rc-\(root)", individualNote: false,
        notes: notes.map { comment in
          var note = note(comment, position: position)
          note = Note(
            id: note.id, body: note.body, author: note.author, createdAt: note.createdAt, system: false,
            resolvable: isResolved != nil, resolved: isResolved, position: position)
          return note
        }))
    }
    return threads.sorted { ($0.notes.first?.createdAt ?? .distantPast) < ($1.notes.first?.createdAt ?? .distantPast) }
  }

  private func note(_ comment: GitHubComment, position: Note.Position?) -> Note {
    Note(
      id: comment.id, body: comment.body ?? "",
      author: .init(username: comment.user?.login ?? "ghost", name: comment.user?.login ?? "Unknown"),
      createdAt: comment.createdAt, system: false, resolvable: nil, resolved: nil, position: position)
  }

  /// Whether each line thread is resolved, by its first comment's ID. REST doesn't say, so it's
  /// one GraphQL query; without it threads just show no resolved state.
  private func resolvedThreads(_ item: MergeRequest) async -> [Int: Bool]? {
    let parts = item.projectPath.split(separator: "/", maxSplits: 1).map(String.init)
    guard parts.count == 2, let number = Int(item.iid),
      let payload = try? await cli.graphQL(
        GitHubQueries.reviewThreads,
        variables: ["owner": parts[0], "name": parts[1]], numbers: ["number": number],
        as: GitHubThreadsPayload.self),
      let threads = payload.repository?.pullRequest?.reviewThreads.nodes
    else { return nil }
    var result: [Int: Bool] = [:]
    for thread in threads {
      if let id = thread.comments.nodes.first?.databaseId { result[id] = thread.isResolved }
    }
    return result
  }

  // MARK: Actions

  func approve(_ item: MergeRequest) async throws {
    _ = try await cli.send("\(base(item))/reviews", json: ["event": "APPROVE"])
  }

  /// GitHub has no "revoke approval"; the closest is dismissing my approving review, which the
  /// repository has to allow me (maintainers and admins can).
  func unapprove(_ item: MergeRequest) async throws {
    let me = try await viewerLogin()
    let reviews: [GitHubReview] = try await pagedREST(cli, "\(base(item))/reviews", maxPages: 5)
    guard let mine = reviews.last(where: {
      $0.state == "APPROVED" && $0.user?.login.caseInsensitiveCompare(me) == .orderedSame
    }) else {
      throw CLI.Failure(message: "There's no approval of yours to revoke.")
    }
    _ = try await cli.send(
      "\(base(item))/reviews/\(mine.id)/dismissals", method: "PUT",
      json: ["message": "Approval revoked.", "event": "DISMISS"])
  }

  private func viewerLogin() async throws -> String {
    struct User: Decodable { let login: String }
    return try JSONCoding.decoder.decode(User.self, from: try await cli.api(["user"])).login
  }

  func comment(on item: MergeRequest, body: String) async throws {
    _ = try await cli.send("repos/\(item.projectPath)/issues/\(item.iid)/comments", json: ["body": body])
  }

  func comment(on item: MergeRequest, file: FileDiff, line: DiffLine, refs: DiffRefs, body: String) async throws {
    // A removed line lives on the left (old) side, everything else on the right.
    let left = line.kind == .removed
    guard let number = left ? line.oldNumber : line.newNumber else {
      throw CLI.Failure(message: "GitHub can't place a comment on that line.")
    }
    _ = try await cli.send(
      "\(base(item))/comments",
      json: [
        "body": body, "commit_id": refs.headSha, "path": file.newPath,
        "line": number, "side": left ? "LEFT" : "RIGHT",
      ])
  }

  func reply(on item: MergeRequest, to thread: Discussion, body: String) async throws {
    if thread.id.hasPrefix("rc-"), let root = thread.notes.first?.id {
      _ = try await cli.send("\(base(item))/comments/\(root)/replies", json: ["body": body])
    } else {
      // Conversation comments aren't threaded on GitHub; a reply is a new comment.
      try await comment(on: item, body: body)
    }
  }
}

// MARK: GraphQL

enum GitHubQueries {
  static let actor = "login avatarUrl ... on User { name }"

  static let pullFields = """
    id number title url createdAt updatedAt mergedAt closedAt state isDraft
    headRefName baseRefName mergeStateStatus totalCommentsCount body
    additions deletions changedFiles
    repository { nameWithOwner name }
    author { \(actor) }
    reviewRequests(first: 20) {
      nodes { requestedReviewer { ... on User { login name avatarUrl } ... on Team { slug name } } }
    }
    latestReviews(first: 30) { nodes { state author { \(actor) } } }
    labels(first: 20) { nodes { name color } }
    commits(last: 1) { nodes { commit { statusCheckRollup { state } } } }
    timelineItems(itemTypes: [REVIEW_REQUESTED_EVENT], last: 50) {
      nodes {
        ... on ReviewRequestedEvent {
          createdAt
          actor { \(actor) }
          requestedReviewer { ... on User { login } ... on Team { slug } }
        }
      }
    }
    """

  static let search = """
    query($q: String!, $after: String) {
      viewer { login name avatarUrl }
      search(query: $q, type: ISSUE, first: 30, after: $after) {
        pageInfo { hasNextPage endCursor }
        nodes { ... on PullRequest { \(pullFields) } }
      }
    }
    """

  static let reviewThreads = """
    query($owner: String!, $name: String!, $number: Int!) {
      repository(owner: $owner, name: $name) {
        pullRequest(number: $number) {
          reviewThreads(first: 100) { nodes { isResolved comments(first: 1) { nodes { databaseId } } } }
        }
      }
    }
    """
}

struct GitHubActor: Decodable {
  let login: String
  let name: String?
  let avatarUrl: String?
}

struct GitHubSearchPayload: Decodable {
  struct Viewer: Decodable {
    let login: String
    let name: String?
    let avatarUrl: String?
  }

  struct Search: Decodable {
    struct PageInfo: Decodable {
      let hasNextPage: Bool
      let endCursor: String?
    }

    let pageInfo: PageInfo
    let nodes: [MaybePull?]
  }

  /// A search hit that may be an issue (an empty object) rather than a pull request.
  struct MaybePull: Decodable {
    let id: String?
    let pull: GitHubPullNode!

    init(from decoder: Decoder) throws {
      let keyed = try decoder.container(keyedBy: AnyKey.self)
      id = try keyed.decodeIfPresent(String.self, forKey: AnyKey("id"))
      pull = id == nil ? nil : try GitHubPullNode(from: decoder)
    }
  }

  let viewer: Viewer?
  let search: Search
}

struct AnyKey: CodingKey {
  var stringValue: String
  var intValue: Int? { nil }
  init(_ string: String) { stringValue = string }
  init?(stringValue: String) { self.stringValue = stringValue }
  init?(intValue: Int) { nil }
}

struct GitHubPullNode: Decodable {
  struct Repository: Decodable {
    let nameWithOwner: String
    let name: String
  }

  struct RequestedReviewer: Decodable {
    /// A user has a login, a team a slug; each may have a name.
    let login: String?
    let slug: String?
    let name: String?
    let avatarUrl: String?
  }

  struct ReviewRequest: Decodable { let requestedReviewer: RequestedReviewer? }

  struct Review: Decodable {
    let state: String
    let author: GitHubActor?
  }

  struct Label: Decodable {
    let name: String
    let color: String
  }

  struct Commit: Decodable {
    struct Inner: Decodable {
      struct Rollup: Decodable { let state: String }
      let statusCheckRollup: Rollup?
    }

    let commit: Inner
  }

  struct RequestEvent: Decodable {
    let createdAt: Date?
    let actor: GitHubActor?
    let requestedReviewer: RequestedReviewer?
  }

  struct Nodes<T: Decodable>: Decodable { let nodes: [T] }

  let id: String
  let number: Int
  let title: String
  let url: String
  let createdAt: Date
  let updatedAt: Date
  let mergedAt: Date?
  let closedAt: Date?
  let state: String
  let isDraft: Bool
  let headRefName: String
  let baseRefName: String
  let mergeStateStatus: String?
  let totalCommentsCount: Int?
  let body: String?
  let additions: Int
  let deletions: Int
  let changedFiles: Int
  let repository: Repository
  let author: GitHubActor?
  let reviewRequests: Nodes<ReviewRequest>?
  let latestReviews: Nodes<Review>?
  let labels: Nodes<Label>?
  let commits: Nodes<Commit>?
  let timelineItems: Nodes<RequestEvent?>?
}

struct GitHubThreadsPayload: Decodable {
  struct Repository: Decodable {
    struct Pull: Decodable {
      struct Thread: Decodable {
        struct Comment: Decodable { let databaseId: Int? }
        let isResolved: Bool
        let comments: GitHubPullNode.Nodes<Comment>
      }

      let reviewThreads: GitHubPullNode.Nodes<Thread>
    }

    let pullRequest: Pull?
  }

  let repository: Repository?
}

// MARK: REST

struct GitHubFile: Decodable {
  let filename: String
  let previousFilename: String?
  let status: String
  let changes: Int
  let patch: String?

  enum CodingKeys: String, CodingKey {
    case filename, status, changes, patch, previousFilename = "previous_filename"
  }
}

struct GitHubUser: Decodable { let login: String }

struct GitHubComment: Decodable {
  let id: Int
  let body: String?
  let user: GitHubUser?
  let createdAt: Date
  let path: String?
  let line: Int?
  let originalLine: Int?
  let side: String?
  let originalCommitId: String?
  let inReplyToId: Int?

  enum CodingKeys: String, CodingKey {
    case id, body, user, path, line, side
    case createdAt = "created_at", originalLine = "original_line"
    case originalCommitId = "original_commit_id", inReplyToId = "in_reply_to_id"
  }
}

struct GitHubReview: Decodable {
  let id: Int
  let body: String?
  let user: GitHubUser?
  let state: String
  let submittedAt: Date?

  enum CodingKeys: String, CodingKey {
    case id, body, user, state, submittedAt = "submitted_at"
  }
}
