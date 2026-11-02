import Foundation

/// `Mrow --queue <gitlab|github>` fetches one forge's queue through its CLI, prints it, loads the
/// first open item's diffs and threads, and quits. Read-only: for checking a forge's mapping
/// from a terminal.
@MainActor enum QueueDump {
  static func run(_ name: String) async {
    guard let forge = Forge(rawValue: name.lowercased()) else {
      print("unknown forge \(name); use gitlab or github")
      exit(2)
    }
    let provider = forge.provider
    do {
      let since = Calendar.current.date(byAdding: .day, value: -14, to: Date())!
      let queue = try await provider.queue(doneSince: since)
      print("\(forge.name) as \(queue.me.name) (@\(queue.me.username))")
      print("open: \(queue.open.count), done: \(queue.done.count)")
      for item in queue.open + queue.done {
        let reviewers = item.reviewers.map { "\($0.person.username):\($0.state.rawValue)" }.joined(separator: ",")
        print(
          "  \(item.fullReference) [\(item.state)] mine=\(item.myState.rawValue) asked=\(item.requestedAt)"
            + " by=\(item.requestedBy?.username ?? "?") guess=\(item.requestedAtIsGuess)"
            + " +\(item.additions)/-\(item.deletions) files=\(item.fileCount)"
            + " ci=\(item.pipeline?.rawValue ?? "-") merge=\(item.mergeStatus ?? "-") reviewers=[\(reviewers)]")
      }
      if let item = queue.open.first ?? queue.done.first {
        let (files, refs) = try await provider.diffs(item)
        print("diffs of \(item.fullReference): \(files.count) files, \(files.reduce(0) { $0 + $1.lines.count }) lines, head=\(refs?.headSha.prefix(8) ?? "nil")")
        let threads = try await provider.discussions(item)
        print("threads: \(threads.count), on lines: \(threads.filter { $0.notes.first?.position != nil }.count)")
      }
      exit(0)
    } catch {
      print("error: \(error.localizedDescription)")
      exit(1)
    }
  }
}
