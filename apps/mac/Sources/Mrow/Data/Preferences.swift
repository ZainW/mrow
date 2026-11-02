import AppKit
import Combine

enum AppearanceChoice: String, CaseIterable, Identifiable {
  case system, light, dark

  var id: String { rawValue }
  var displayName: String { rawValue.capitalized }

  var appearance: NSAppearance? {
    switch self {
    case .system: nil
    case .light: NSAppearance(named: .aqua)
    case .dark: NSAppearance(named: .darkAqua)
    }
  }
}

/// Settings and small bits of state, kept in UserDefaults.
final class Preferences: ObservableObject {
  nonisolated(unsafe) static let shared = Preferences()

  private let defaults = UserDefaults.standard

  /// Blank means "find glab on its own".
  @Published var glabPath: String { didSet { defaults.set(glabPath, forKey: "glabPath") } }
  /// Blank means glab's default host.
  @Published var hostname: String { didSet { defaults.set(hostname, forKey: "hostname") } }
  /// Blank means "find gh on its own".
  @Published var ghPath: String { didSet { defaults.set(ghPath, forKey: "ghPath") } }
  /// Blank means gh's default host (github.com); set for GitHub Enterprise.
  @Published var githubHost: String { didSet { defaults.set(githubHost, forKey: "githubHost") } }
  @Published var gitlabEnabled: Bool { didSet { defaults.set(gitlabEnabled, forKey: "gitlabEnabled") } }
  @Published var githubEnabled: Bool { didSet { defaults.set(githubEnabled, forKey: "githubEnabled") } }
  /// The account the sidebar shows.
  @Published var activeForge: Forge { didSet { defaults.set(activeForge.rawValue, forKey: "activeForge") } }
  /// How far back the Done list looks.
  @Published var doneDays: Int { didSet { defaults.set(doneDays, forKey: "doneDays") } }
  @Published var refreshMinutes: Int { didSet { defaults.set(refreshMinutes, forKey: "refreshMinutes") } }
  @Published var showDrafts: Bool { didSet { defaults.set(showDrafts, forKey: "showDrafts") } }
  @Published var oldestFirst: Bool { didSet { defaults.set(oldestFirst, forKey: "oldestFirst") } }
  @Published var notify: Bool { didSet { defaults.set(notify, forKey: "notify") } }
  @Published var sidebarWidth: Double { didSet { defaults.set(sidebarWidth, forKey: "sidebarWidth") } }
  /// The "Ask Cursor" panel's width, dragged from its left edge.
  @Published var askWidth: Double { didSet { defaults.set(askWidth, forKey: "askWidth") } }
  @Published var appearance: AppearanceChoice {
    didSet {
      defaults.set(appearance.rawValue, forKey: "appearance")
      NSApp?.appearance = appearance.appearance
    }
  }

  private init() {
    defaults.register(defaults: [
      "doneDays": 14, "refreshMinutes": 5, "showDrafts": true, "oldestFirst": false,
      "notify": true, "sidebarWidth": 340.0, "askWidth": 400.0,
      "gitlabEnabled": true, "githubEnabled": true,
    ])
    glabPath = defaults.string(forKey: "glabPath") ?? ""
    hostname = defaults.string(forKey: "hostname") ?? ""
    ghPath = defaults.string(forKey: "ghPath") ?? ""
    githubHost = defaults.string(forKey: "githubHost") ?? ""
    gitlabEnabled = defaults.bool(forKey: "gitlabEnabled")
    githubEnabled = defaults.bool(forKey: "githubEnabled")
    activeForge = Forge(rawValue: defaults.string(forKey: "activeForge") ?? "") ?? .gitlab
    doneDays = defaults.integer(forKey: "doneDays")
    refreshMinutes = defaults.integer(forKey: "refreshMinutes")
    showDrafts = defaults.bool(forKey: "showDrafts")
    oldestFirst = defaults.bool(forKey: "oldestFirst")
    notify = defaults.bool(forKey: "notify")
    sidebarWidth = defaults.double(forKey: "sidebarWidth")
    askWidth = defaults.double(forKey: "askWidth")
    appearance = AppearanceChoice(rawValue: defaults.string(forKey: "appearance") ?? "") ?? .system
  }

  func isEnabled(_ forge: Forge) -> Bool {
    switch forge {
    case .gitlab: gitlabEnabled
    case .github: githubEnabled
    }
  }

  var enabledForges: [Forge] { Forge.allCases.filter(isEnabled) }

  /// When each merge request was last opened here, by its forge's global ID.
  var lastSeen: [String: Date] {
    get {
      (defaults.dictionary(forKey: "lastSeen") as? [String: Double] ?? [:])
        .mapValues(Date.init(timeIntervalSince1970:))
    }
    set { defaults.set(newValue.mapValues(\.timeIntervalSince1970), forKey: "lastSeen") }
  }

  /// The merge requests open in tabs, in order, by their forge's global ID.
  var tabs: [String] {
    get { defaults.stringArray(forKey: "tabs") ?? [] }
    set { defaults.set(newValue, forKey: "tabs") }
  }

  /// Files marked viewed: merge request → file → the diff's digest when it was marked.
  var viewedFiles: [String: [String: Int]] {
    get { defaults.dictionary(forKey: "viewedFiles") as? [String: [String: Int]] ?? [:] }
    set { defaults.set(newValue, forKey: "viewedFiles") }
  }

  /// Merge requests already seen in the queue, so only new requests notify.
  var knownRequests: Set<String> {
    get { Set(defaults.stringArray(forKey: "knownRequests") ?? []) }
    set { defaults.set(Array(newValue), forKey: "knownRequests") }
  }
}
