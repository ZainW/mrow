import Foundation

/// One of the models Cursor's agent offers. Mrow asks for Cursor's "parameterized" model list, so
/// ids are plain ("claude-sonnet-5-5") and effort is a separate setting (`EffortOptions`).
struct AgentModel: Codable, Hashable, Identifiable {
  let id: String
  let name: String

  /// Ids from before the parameterized list carried the variant: "claude-sonnet-5-5[effort=high]".
  static func base(_ id: String) -> String {
    id.firstIndex(of: "[").map { String(id[..<$0]) } ?? id
  }

  /// Cursor's own name ("Claude Sonnet 5.5"), or one made from the id when it sends none.
  var title: String { name.isEmpty || name == id ? Self.pretty(id) : name }

  /// The title without the maker, for the narrow header button: "Sonnet 5.5".
  var shortTitle: String {
    title.hasPrefix("Claude ") ? String(title.dropFirst(7)) : title
  }

  var maker: String {
    switch id.split(separator: "-").first ?? "" {
    case "claude": "Anthropic"
    case "gpt": "OpenAI"
    case "gemini": "Google"
    case "grok": "xAI"
    case "composer", "auto": "Cursor"
    default: "Other"
    }
  }

  static let makers = ["Cursor", "Anthropic", "OpenAI", "Google", "xAI", "Other"]

  /// The id without its version: "claude-opus-4-8" → "claude-opus", "gemini-3.1-pro" →
  /// "gemini-pro", "gpt-5.6-sol" → "gpt-sol".
  var family: String {
    if id.hasPrefix("auto") { return "auto" }
    return id.split(separator: "-").filter { !Self.isVersion($0) }.joined(separator: "-")
  }

  /// "claude-opus-4-8" → [4, 8], "gpt-5.4-mini" → [5, 4]: compared to find a family's newest.
  var version: [Int] {
    id.split(separator: "-").filter(Self.isVersion).flatMap { $0.split(separator: ".") }.compactMap { Int($0) }
  }

  private static func isVersion(_ part: Substring) -> Bool {
    !part.isEmpty && part.allSatisfy { $0.isNumber || $0 == "." }
  }

  static func newer(_ a: AgentModel, than b: AgentModel) -> Bool {
    let (x, y) = (a.version, b.version)
    for index in 0..<max(x.count, y.count) {
      let (l, r) = (index < x.count ? x[index] : 0, index < y.count ? y[index] : 0)
      if l != r { return l > r }
    }
    return false
  }

  /// The short list the picker leads with, as Claude's does: the newest of a few families, each
  /// with a line on when it suits. Everything else is under "More models".
  static let featuredFamilies: [(family: String, blurb: String)] = [
    ("auto", "Cursor picks a model for each question"),
    ("claude-opus", "Most capable, for the hardest questions"),
    ("claude-sonnet", "Smart and quick, for most reviews"),
    ("claude-haiku", "Fastest, for quick lookups"),
    ("composer", "Cursor's own model, built for code"),
  ]

  /// This model's line in the short list, if its family has one.
  var blurb: String? { Self.featuredFamilies.first { $0.family == family }?.blurb }

  /// The newest model of each featured family, in the list's order.
  static func featured(in models: [AgentModel]) -> [AgentModel] {
    featuredFamilies.compactMap { entry in
      models.filter { $0.family == entry.family }.max { newer($1, than: $0) }
    }
  }

  /// "claude-opus-4-8" → "Claude Opus 4.8", "gpt-5.3-codex" → "GPT-5.3 Codex".
  static func pretty(_ id: String) -> String {
    let parts = id.split(separator: "-").map(String.init)
    func word(_ part: String) -> String {
      switch part {
      case "glm": "GLM"
      default: part.prefix(1).uppercased() + part.dropFirst()
      }
    }
    if parts.first == "gpt", parts.count > 1 {
      return (["GPT-" + parts[1]] + parts.dropFirst(2).map(word)).joined(separator: " ")
    }
    func isNumber(_ part: String) -> Bool { !part.isEmpty && part.allSatisfy { $0.isNumber || $0 == "." } }
    var words: [String] = []
    for part in parts {
      if let last = words.last, isNumber(part), isNumber(last) {
        words[words.count - 1] = last + "." + part
      } else {
        words.append(isNumber(part) ? part : word(part))
      }
    }
    return words.joined(separator: " ")
  }
}

/// A model's effort setting as Cursor offers it. Its id differs by model ("reasoning_effort" with
/// low…max for Sonnet, "effort" for Opus, "reasoning" with none…extra-high for GPT); `values` is
/// empty for a model without one.
struct EffortOptions: Codable, Equatable {
  struct Option: Codable, Hashable {
    let value: String
    let name: String
  }

  let configID: String
  let values: [Option]
  var current: String

  func option(_ value: String?) -> Option? { values.first { $0.value == value } }

  /// The model a session is on and its effort setting, from a `configOptions` snapshot (as
  /// `session/new` and `session/set_config_option` return it).
  static func from(_ configOptions: [JSONObject]) -> (model: String, effort: EffortOptions)? {
    guard let model = configOptions.first(where: { $0.string("id") == "model" })?.string("currentValue") else {
      return nil
    }
    guard let thought = configOptions.first(where: { $0.string("category") == "thought_level" }),
      let id = thought.string("id")
    else {
      return (model, EffortOptions(configID: "", values: [], current: ""))
    }
    let values = thought.objects("options").compactMap { option in
      option.string("value").map { Option(value: $0, name: option.string("name") ?? $0) }
    }
    return (model, EffortOptions(configID: id, values: values, current: thought.string("currentValue") ?? ""))
  }
}
