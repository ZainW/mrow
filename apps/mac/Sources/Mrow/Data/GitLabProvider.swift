import Foundation

/// GitLab through `glab`: one GraphQL query for the queue, REST for diffs, threads and actions.
struct GitLabProvider: ReviewProvider {
  private var cli: CLI { .glab }

  // MARK: Queue

  func queue(doneSince since: Date) async throws -> QueueResult {
    async let openNodes = fetch(state: "opened", since: nil, pages: 5)
    async let mergedNodes = fetch(state: "merged", since: since, pages: 2)
    async let closedNodes = fetch(state: "closed", since: since, pages: 1)
    let (opened, merged, closed) = try await (openNodes, mergedNodes, closedNodes)
    guard let me = opened.user else { throw CLI.Failure(message: "glab isn't signed in.") }

    let all = opened.nodes + merged.nodes + closed.nodes
    let extraNotes = await fetchMissingRequestNotes(all, me: me.username)
    return QueueResult(
      me: me,
      open: opened.nodes.map { Self.build($0, me: me.username, extra: extraNotes[$0.id] ?? []) },
      done: (merged.nodes + closed.nodes).map { Self.build($0, me: me.username, extra: extraNotes[$0.id] ?? []) })
  }

  private struct Page {
    var user: Person?
    var nodes: [MergeRequestNode] = []
  }

  private func fetch(state: String, since: Date?, pages: Int) async throws -> Page {
    var page = Page()
    var cursor: String?
    for _ in 0..<pages {
      let payload = try await cli.graphQL(
        Queries.queue,
        variables: [
          "state": state, "after": cursor,
          "updatedAfter": since.map { ISO8601DateFormatter().string(from: $0) },
        ],
        as: QueuePayload.self)
      guard let user = payload.currentUser else { break }
      page.user = Person(username: user.username, name: user.name)
      let connection = user.reviewRequestedMergeRequests
      page.nodes += connection.nodes
      guard connection.pageInfo?.hasNextPage == true, let next = connection.pageInfo?.endCursor else { break }
      cursor = next
    }
    return page
  }

  /// Busy merge requests can have more than 100 activity notes, and only the newest 100 come
  /// with the queue; page back through older ones until the review request turns up.
  private func fetchMissingRequestNotes(_ nodes: [MergeRequestNode], me: String) async -> [String: [ActivityNote]] {
    let missing = nodes.filter {
      Self.requestNotes($0.notes.nodes, me: me).isEmpty && $0.notes.pageInfo?.hasPreviousPage == true
    }
    let cli = self.cli
    return await withTaskGroup(of: (String, [ActivityNote]).self) { group in
      for node in missing.prefix(12) {
        group.addTask {
          var notes: [ActivityNote] = []
          var cursor = node.notes.pageInfo?.startCursor
          for _ in 0..<5 {
            guard let before = cursor,
              let payload = try? await cli.graphQL(
                Queries.moreNotes,
                variables: ["path": node.project.fullPath, "iid": node.iid, "before": before],
                as: NotesPayload.self),
              let connection = payload.project?.mergeRequest?.notes
            else { break }
            notes += connection.nodes
            cursor = connection.pageInfo?.hasPreviousPage == true ? connection.pageInfo?.startCursor : nil
          }
          return (node.id, notes)
        }
      }
      var result: [String: [ActivityNote]] = [:]
      for await (id, notes) in group { result[id] = notes }
      return result
    }
  }

  /// Notes like "requested review from @a and @me", but not "removed review request for @me".
  static func requestNotes(_ notes: [ActivityNote], me: String) -> [ActivityNote] {
    notes.filter { note in
      guard let start = note.body.range(of: "requested review from") else { return false }
      var tail = note.body[start.upperBound...]
      if let removed = tail.range(of: "removed review request") { tail = tail[..<removed.lowerBound] }
      return mentions(tail, username: me)
    }
  }

  private static func mentions(_ text: Substring, username: String) -> Bool {
    let handle = "@" + username
    var searchStart = text.startIndex
    while let range = text.range(of: handle, options: .caseInsensitive, range: searchStart..<text.endIndex) {
      // "@zain42" shouldn't match "@zain42bot".
      let next = text[range.upperBound...]
      // A "." only continues the handle if more of it follows ("@alex.smith", not "@alex.").
      let continues =
        next.first.map { $0.isLetter || $0.isNumber || $0 == "_" || $0 == "-" } ?? false
        || (next.first == "." && next.dropFirst().first.map { $0.isLetter || $0.isNumber } == true)
      if !continues { return true }
      searchStart = range.upperBound
    }
    return false
  }

  private static func build(_ node: MergeRequestNode, me: String, extra: [ActivityNote]) -> MergeRequest {
    let requests = requestNotes(node.notes.nodes + extra, me: me).sorted { $0.createdAt < $1.createdAt }
    let reviewers = node.reviewers.nodes.map {
      Reviewer(
        person: Person(username: $0.username, name: $0.name, avatarUrl: $0.avatarUrl),
        state: $0.mergeRequestInteraction?.reviewState ?? .unknown,
        approved: $0.mergeRequestInteraction?.approved ?? false)
    }
    let mine = reviewers.first { $0.person.username.caseInsensitiveCompare(me) == .orderedSame }
    return MergeRequest(
      id: node.id, iid: node.iid, title: node.title,
      webURL: URL(string: node.webUrl) ?? URL(string: "https://gitlab.com")!,
      projectPath: node.project.fullPath, projectName: node.project.name,
      author: node.author ?? Person(username: "ghost", name: "Unknown"),
      createdAt: node.createdAt, updatedAt: node.updatedAt, mergedAt: node.mergedAt,
      closedAt: node.closedAt, state: node.state, draft: node.draft,
      sourceBranch: node.sourceBranch, targetBranch: node.targetBranch,
      mergeStatus: node.detailedMergeStatus, commentCount: node.userNotesCount ?? 0,
      description: node.description ?? "", reviewers: reviewers,
      approvedBy: node.approvedBy?.nodes.map(\.username) ?? [],
      additions: node.diffStatsSummary?.additions ?? 0,
      deletions: node.diffStatsSummary?.deletions ?? 0,
      fileCount: node.diffStatsSummary?.fileCount ?? 0,
      pipeline: node.headPipeline.flatMap { PipelineStatus(rawValue: $0.status) },
      labels: node.labels?.nodes ?? [],
      requestedAt: requests.last?.createdAt ?? node.createdAt,
      firstRequestedAt: requests.first?.createdAt ?? node.createdAt,
      requestedBy: requests.last?.author,
      requestedAtIsGuess: requests.isEmpty,
      myState: mine?.state ?? .unknown,
      forge: .gitlab)
  }

  // MARK: Details

  private func base(_ item: MergeRequest) -> String { "projects/\(item.projectID)/merge_requests/\(item.iid)" }

  /// The diffs come from two endpoints, so the commits are read on both sides of the diffs: if a
  /// push landed in between, try again, and failing that, leave the commits out (which turns off
  /// comments on lines until a reload).
  func diffs(_ item: MergeRequest) async throws -> ([FileDiff], DiffRefs?) {
    for _ in 0..<2 {
      let before = try? await diffRefs(item)
      let files = try await fileDiffs(item)
      let after = try? await diffRefs(item)
      if before == after { return (files, after) }
    }
    return (try await fileDiffs(item), nil)
  }

  private func fileDiffs(_ item: MergeRequest) async throws -> [FileDiff] {
    let files: [FileDiff] = try await pagedREST(cli, "\(base(item))/diffs", maxPages: 10)
    return files.map { $0.prepared() }
  }

  private func diffRefs(_ item: MergeRequest) async throws -> DiffRefs? {
    struct Payload: Decodable {
      let diffRefs: DiffRefs?
      enum CodingKeys: String, CodingKey { case diffRefs = "diff_refs" }
    }
    let data = try await cli.api([base(item)])
    return try JSONCoding.decoder.decode(Payload.self, from: data).diffRefs
  }

  func discussions(_ item: MergeRequest) async throws -> [Discussion] {
    try await pagedREST(cli, "\(base(item))/discussions", maxPages: 10)
  }

  // MARK: Actions

  func approve(_ item: MergeRequest) async throws {
    _ = try await cli.send("\(base(item))/approve", json: [:])
  }

  func unapprove(_ item: MergeRequest) async throws {
    _ = try await cli.send("\(base(item))/unapprove", json: [:])
  }

  func comment(on item: MergeRequest, body: String) async throws {
    _ = try await cli.send("\(base(item))/notes", json: ["body": body])
  }

  func comment(on item: MergeRequest, file: FileDiff, line: DiffLine, refs: DiffRefs, body: String) async throws {
    var position: [String: Any] = [
      "position_type": "text", "base_sha": refs.baseSha, "start_sha": refs.startSha, "head_sha": refs.headSha,
      "old_path": file.oldPath, "new_path": file.newPath,
    ]
    // GitLab places a comment by the side it's on: new line for added, old for removed, both for context.
    if let new = line.newNumber, line.kind != .removed { position["new_line"] = new }
    if let old = line.oldNumber, line.kind != .added { position["old_line"] = old }
    _ = try await cli.send("\(base(item))/discussions", json: ["body": body, "position": position])
  }

  func reply(on item: MergeRequest, to thread: Discussion, body: String) async throws {
    _ = try await cli.send("\(base(item))/discussions/\(thread.id)/notes", json: ["body": body])
  }
}

extension ReviewStore {
  /// Kept here for the self-test.
  nonisolated static func requestNotes(_ notes: [ActivityNote], me: String) -> [ActivityNote] {
    GitLabProvider.requestNotes(notes, me: me)
  }
}
