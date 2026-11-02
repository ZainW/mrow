import Foundation

/// Where the user's own clone of a GitLab project lives, so Cursor's agent can read the code
/// around a change. Found by reading `.git/config` remotes in the usual places; a folder picked
/// by hand wins. Nothing here runs git or changes the checkout.
enum Checkouts {
  private static let key = "checkouts"

  /// Folders whose children are searched, then the home folder's own children.
  private static let roots = ["caribou", "code", "src", "dev", "Developer", "projects", "repos", "work", "git", "GitHub"]

  /// A remembered or found clone of `project` ("group/name"), or nil.
  static func find(project: String) -> URL? {
    if let saved = (UserDefaults.standard.dictionary(forKey: key) as? [String: String])?[project],
      FileManager.default.fileExists(atPath: saved)
    {
      return URL(fileURLWithPath: saved, isDirectory: true)
    }
    let home = FileManager.default.homeDirectoryForCurrentUser
    let folders = roots.map { home.appendingPathComponent($0, isDirectory: true) } + [home]
    for folder in folders {
      let children = (try? FileManager.default.contentsOfDirectory(
        at: folder, includingPropertiesForKeys: nil, options: [.skipsHiddenFiles])) ?? []
      // The plain name first (backend-service before backend-service-cr8018).
      let name = project.split(separator: "/").last.map(String.init) ?? project
      let ordered = children.sorted { a, b in
        (a.lastPathComponent == name) != (b.lastPathComponent == name) ? a.lastPathComponent == name : a.path < b.path
      }
      for child in ordered where matches(child, project: project) {
        return child
      }
    }
    return nil
  }

  static func remember(_ folder: URL, for project: String) {
    var saved = UserDefaults.standard.dictionary(forKey: key) as? [String: String] ?? [:]
    saved[project] = folder.path
    UserDefaults.standard.set(saved, forKey: key)
  }

  /// A clone (not a worktree: those have a `.git` file) with a remote ending in the project path.
  private static func matches(_ folder: URL, project: String) -> Bool {
    let config = folder.appendingPathComponent(".git/config")
    guard let text = try? String(contentsOf: config, encoding: .utf8) else { return false }
    let wanted = project.lowercased()
    return text.split(separator: "\n").contains { line in
      let trimmed = line.trimmingCharacters(in: .whitespaces).lowercased()
      guard trimmed.hasPrefix("url") else { return false }
      var url = trimmed.split(separator: "=", maxSplits: 1).last.map { $0.trimmingCharacters(in: .whitespaces) } ?? ""
      if url.hasSuffix(".git") { url.removeLast(4) }
      if url.hasSuffix("/") { url.removeLast() }
      return url.hasSuffix(":" + wanted) || url.hasSuffix("/" + wanted)
    }
  }
}
