import Foundation

enum TokenKind: Sendable, Hashable {
  case plain, keyword, string, number, comment, type, function, attribute, constant, property, tag
  case operator_, punctuation, preprocessor, regex, heading, link, deleted, inserted
}

struct SyntaxToken: Sendable, Equatable {
  /// UTF-16 range into the code string.
  public let range: NSRange
  public let kind: TokenKind

  public init(range: NSRange, kind: TokenKind) {
    self.range = range
    self.kind = kind
  }
}

enum SyntaxHighlighter {
  /// Normalises a fenced-code info string ("ts", "TypeScript", "js {1,3}", "c++", "sh", "yml",
  /// "objc", "jsonc"...) to a canonical language id, or nil if unknown.
  public static func language(for info: String?) -> String? {
    guard let info else { return nil }
    var s = Substring(info.trimmingCharacters(in: .whitespacesAndNewlines))
    while let f = s.first, f == "{" || f == "." { s = s.dropFirst() }
    let stops: Set<Character> = [" ", "\t", "{", "}", ",", ":", "=", "[", "(", ";"]
    let end = s.firstIndex { stops.contains($0) } ?? s.endIndex
    var word = s[..<end].lowercased()
    for prefix in ["language-", "lang-"] where word.hasPrefix(prefix) {
      word.removeFirst(prefix.count)
    }
    if word.isEmpty { return nil }
    if let id = aliases[word] { return id }
    // File names: "main.rs", "Dockerfile.dev", "index.d.ts".
    if let dot = word.lastIndex(of: "."),
      let id = aliases[String(word[word.index(after: dot)...])]
    {
      return id
    }
    if word.hasPrefix("dockerfile") { return "dockerfile" }
    if word.hasPrefix("makefile") { return "makefile" }
    return nil
  }

  /// Display name for a canonical id, e.g. "typescript" -> "TypeScript".
  public static func displayName(for language: String) -> String {
    if let name = displayNames[language] { return name }
    if let id = self.language(for: language), let name = displayNames[id] { return name }
    return language.prefix(1).uppercased() + language.dropFirst()
  }

  /// Tokens for code: only non-plain tokens, sorted and non-overlapping. Unknown language -> [].
  public static func tokens(in code: String, language: String?) -> [SyntaxToken] {
    guard let language else { return [] }
    let engine: Engine
    if let e = engines[language] {
      engine = e
    } else if let id = self.language(for: language), let e = engines[id] {
      engine = e
    } else {
      return []
    }
    if case .plain = engine { return [] }
    let units = Array(code.utf16)
    return units.withUnsafeBufferPointer { buf in
      let hl = Highlighter(buf)
      hl.run(engine, 0, buf.count)
      return hl.toks.map {
        SyntaxToken(range: NSRange(location: $0.s, length: $0.e - $0.s), kind: $0.k)
      }
    }
  }

  /// Every canonical language id.
  static var allLanguages: [String] { engines.keys.sorted() }

  fileprivate static let aliases: [String: String] = {
    let groups: [String: String] = [
      "swift": "swift",
      "typescript": "typescript ts mts cts",
      "tsx": "tsx",
      "javascript": "javascript js mjs cjs node es6 ecmascript",
      "jsx": "jsx",
      "json": "json geojson jsonl ndjson webmanifest topojson",
      "jsonc": "jsonc json5 jsonld",
      "python": "python py python3 py3 pyi gyp sage",
      "rust": "rust rs",
      "go": "go golang",
      "c": "c h",
      "cpp": "cpp c++ cxx cc hpp hh hxx h++ ino arduino cuda cu",
      "objectivec": "objectivec objective-c objc obj-c m mm objective-c++ objc++ objectivecpp",
      "csharp": "csharp cs c# dotnet",
      "java": "java",
      "kotlin": "kotlin kt kts",
      "ruby": "ruby rb gemfile podfile rake rakefile gemspec ru erb",
      "php": "php php3 php4 php5 php7 php8 phtml",
      "bash": "bash sh zsh shell ksh fish shellscript bats ash dash",
      "console": "console shell-session shellsession sh-session terminal bash-session zsh-session",
      "powershell": "powershell ps1 pwsh ps psm1 psd1",
      "sql": "sql mysql postgres postgresql psql sqlite plsql tsql pgsql mssql mariadb",
      "yaml": "yaml yml",
      "toml": "toml",
      "ini": "ini cfg conf properties env dotenv editorconfig gitconfig desktop",
      "html": "html htm xhtml vue svelte astro",
      "xml": "xml plist xsd xsl xslt rss atom csproj storyboard xib wsdl xaml",
      "svg": "svg",
      "css": "css",
      "scss": "scss sass",
      "less": "less",
      "markdown": "markdown md mdx mkd mdown",
      "diff": "diff patch udiff",
      "dockerfile": "dockerfile docker containerfile",
      "makefile": "makefile make mk mak gnumakefile",
      "lua": "lua luau",
      "dart": "dart",
      "zig": "zig",
      "elixir": "elixir ex exs",
      "haskell": "haskell hs",
      "scala": "scala sc sbt",
      "graphql": "graphql gql",
      "nix": "nix",
      "r": "r rscript",
      "perl": "perl pl pm",
      "groovy": "groovy gradle",
      "protobuf": "protobuf proto",
      "hcl": "hcl tf terraform tfvars",
      "plaintext": "plaintext text txt plain",
    ]
    var map: [String: String] = [:]
    for (id, names) in groups {
      for name in names.split(separator: " ") { map[String(name)] = id }
    }
    return map
  }()

  fileprivate static let displayNames: [String: String] = [
    "swift": "Swift", "typescript": "TypeScript", "tsx": "TSX", "javascript": "JavaScript",
    "jsx": "JSX", "json": "JSON", "jsonc": "JSON with Comments", "python": "Python",
    "rust": "Rust", "go": "Go", "c": "C", "cpp": "C++", "objectivec": "Objective-C",
    "csharp": "C#", "java": "Java", "kotlin": "Kotlin", "ruby": "Ruby", "php": "PHP",
    "bash": "Shell", "console": "Console", "powershell": "PowerShell", "sql": "SQL",
    "yaml": "YAML", "toml": "TOML", "ini": "INI", "html": "HTML", "xml": "XML", "svg": "SVG",
    "css": "CSS", "scss": "SCSS", "less": "Less", "markdown": "Markdown", "diff": "Diff",
    "dockerfile": "Dockerfile", "makefile": "Makefile", "lua": "Lua", "dart": "Dart",
    "zig": "Zig", "elixir": "Elixir", "haskell": "Haskell", "scala": "Scala",
    "graphql": "GraphQL", "nix": "Nix", "r": "R", "perl": "Perl", "groovy": "Groovy",
    "protobuf": "Protocol Buffers", "hcl": "HCL", "plaintext": "Plain Text",
  ]

  fileprivate static let engines: [String: Engine] = [
    "swift": .code(.swift), "typescript": .code(.typescript), "tsx": .code(.tsx),
    "javascript": .code(.javascript), "jsx": .code(.javascript), "json": .code(.json),
    "jsonc": .code(.json), "python": .code(.python), "rust": .code(.rust), "go": .code(.go),
    "c": .code(.c), "cpp": .code(.cpp), "objectivec": .code(.objc), "csharp": .code(.csharp),
    "java": .code(.java), "kotlin": .code(.kotlin), "ruby": .code(.ruby), "php": .php,
    "bash": .shell, "console": .console, "powershell": .code(.powershell), "sql": .code(.sql),
    "yaml": .yaml, "toml": .code(.toml), "ini": .code(.ini), "html": .markup(html: true),
    "xml": .markup(html: false), "svg": .markup(html: false), "css": .css(scss: false),
    "scss": .css(scss: true), "less": .css(scss: true), "markdown": .markdown, "diff": .diff,
    "dockerfile": .dockerfile, "makefile": .makefile, "lua": .code(.lua), "dart": .code(.dart),
    "zig": .code(.zig), "elixir": .code(.elixir), "haskell": .code(.haskell),
    "scala": .code(.scala), "graphql": .code(.graphql), "nix": .code(.nix), "r": .code(.r),
    "perl": .code(.perl), "groovy": .code(.groovy), "protobuf": .code(.protobuf),
    "hcl": .code(.hcl), "plaintext": .plain,
  ]
}

// MARK: - Language definitions

private enum Engine: Sendable {
  case code(LangDef)
  case shell, console, php, yaml, markdown, diff, dockerfile, makefile, plain
  case markup(html: Bool)
  case css(scss: Bool)
}

private func p(_ s: String) -> [UInt16] { Array(s.utf16) }
private func words(_ s: String) -> Set<String> { Set(s.split(separator: " ").map(String.init)) }

private struct Delim: Sendable {
  let open: [UInt16]
  let close: [UInt16]
  init(_ open: String, _ close: String) {
    self.open = p(open)
    self.close = p(close)
  }
}

private enum Interp: Sendable {
  case none
  case dollarBrace  // `${expr}`
  case dollarIdent  // `${expr}` and `$name`
  case backslashParen  // Swift `\(expr)`
  case hashBrace  // Ruby/Elixir `#{expr}`
  case brace  // C# `$"{expr}"`
}

private struct StrRule: Sendable {
  let open: [UInt16]
  let close: [UInt16]
  var escape: UInt16
  var doubled: Bool
  var multiline: Bool
  var interp: Interp
  var kind: TokenKind

  init(
    _ open: String, _ close: String? = nil, escape: UInt16 = A.backslash, doubled: Bool = false,
    multiline: Bool = false, interp: Interp = .none, kind: TokenKind = .string
  ) {
    self.open = p(open)
    self.close = p(close ?? open)
    self.escape = escape
    self.doubled = doubled
    self.multiline = multiline
    self.interp = interp
    self.kind = kind
  }
}

private struct LangDef: Sendable {
  var lineComments: [[UInt16]] = []
  var blockComments: [Delim] = []
  var nestedComments = false
  /// Ruby `=begin`/`=end`, Perl POD: both markers must start a line.
  var lineStartBlocks: [Delim] = []
  var strings: [StrRule] = []
  /// Identifier prefixes that may glue onto a string (`r"..."`, `b'..'`, `L"..."`). Lowercase.
  var stringPrefixes: Set<String> = []
  /// Zig `\\` multiline string lines.
  var lineStrings: [[UInt16]] = []
  var keywords: Set<String> = []
  var types: Set<String> = []
  var constants: Set<String> = []
  var functionDecl: Set<String> = []
  var typeDecl: Set<String> = []
  var caseInsensitive = false
  var capitalizedTypes = true
  var constructorCalls = true
  var calls = true
  var properties = true
  var atKind: TokenKind? = nil
  var hashPreprocessor = false
  var hashDirectives = false
  var sigils: [UInt16] = []
  var identExtra: [UInt16] = []
  var identSuffix: [UInt16] = []
  var dollarIdent = false
  var regex = false
  var jsx = false
  var symbols = false
  var jsonKeys = false
  var assignKeys = false
  var keyLines = false
  var rustAttrs = false
  var rustLifetimes = false
  var macroBang = false
  var rawHashStrings = false
  var rustRaw = false
  var cppRaw = false
  var tildeSigils = false
  var perlQuotes = false
  var dashParams = false
  var dashFunctions = false
  var haskellSigs = false
  var shebang = true

  // Derived.
  var maxWord = 0
  var startsLo: UInt64 = 0
  var startsHi: UInt64 = 0

  init(_ build: (inout LangDef) -> Void) {
    build(&self)
    strings.sort { $0.open.count > $1.open.count }
    var longest = 0
    for set in [keywords, types, constants, stringPrefixes] {
      for word in set { longest = max(longest, word.utf16.count) }
    }
    if perlQuotes || rustRaw || cppRaw { longest = max(longest, 3) }
    maxWord = longest
    var firsts: [UInt16] = []
    firsts += lineComments.compactMap(\.first)
    firsts += blockComments.compactMap(\.open.first)
    firsts += lineStartBlocks.compactMap(\.open.first)
    firsts += strings.compactMap(\.open.first)
    firsts += lineStrings.compactMap(\.first)
    for c in firsts where c < 128 {
      if c < 64 { startsLo |= 1 << UInt64(c) } else { startsHi |= 1 << UInt64(c - 64) }
    }
  }

  @inline(__always) func startsSomething(_ c: UInt16) -> Bool {
    if c < 64 { return (startsLo >> UInt64(c)) & 1 == 1 }
    if c < 128 { return (startsHi >> UInt64(c - 64)) & 1 == 1 }
    return false
  }
}

extension LangDef {
  fileprivate static let slash = [p("//")]
  fileprivate static let cBlock = [Delim("/*", "*/")]
  fileprivate static let dq = StrRule("\"")
  fileprivate static let sq = StrRule("'")

  fileprivate static let swift = LangDef {
    $0.lineComments = slash
    $0.blockComments = cBlock
    $0.nestedComments = true
    $0.strings = [
      StrRule("\"\"\"", multiline: true, interp: .backslashParen),
      StrRule("\"", interp: .backslashParen),
    ]
    $0.keywords = words(
      "as associatedtype async await break case catch class continue convenience default defer "
        + "deinit didSet do dynamic else enum extension fallthrough fileprivate final for func get "
        + "guard if import in indirect infix init inout internal is lazy let mutating nonmutating "
        + "open operator optional override postfix precedencegroup prefix private protocol public "
        + "repeat required rethrows return set some any static struct subscript switch throw "
        + "throws try typealias unowned var weak where while willSet actor nonisolated isolated "
        + "package macro consume borrowing consuming sending each discard")
    $0.constants = words("true false nil self super")
    $0.functionDecl = ["func"]
    $0.typeDecl = words("class struct enum protocol extension actor typealias associatedtype macro")
    $0.atKind = .attribute
    $0.hashDirectives = true
    $0.rawHashStrings = true
    $0.dollarIdent = true
  }

  fileprivate static let jsKeywords =
    "as async await break case catch class const continue debugger default delete do else export "
    + "extends finally for from function get if import in instanceof let new of return set static "
    + "switch throw try typeof var void while with yield"
  fileprivate static let tsKeywords =
    " abstract asserts declare enum implements infer interface is keyof module namespace override "
    + "private protected public readonly satisfies type unique accessor out"

  fileprivate static func js(ts: Bool, jsx: Bool) -> LangDef {
    LangDef {
      $0.lineComments = slash
      $0.blockComments = cBlock
      $0.strings = [dq, sq, StrRule("`", multiline: true, interp: .dollarBrace)]
      $0.keywords = words(jsKeywords + (ts ? tsKeywords : ""))
      $0.constants = words("true false null undefined this super NaN Infinity")
      if ts {
        $0.types = words("string number boolean any unknown never object symbol bigint")
      }
      $0.functionDecl = ["function"]
      $0.typeDecl = words("class interface type enum namespace")
      $0.atKind = .attribute
      $0.dollarIdent = true
      $0.regex = true
      $0.jsx = jsx
    }
  }
  fileprivate static let javascript = js(ts: false, jsx: true)
  fileprivate static let typescript = js(ts: true, jsx: false)
  fileprivate static let tsx = js(ts: true, jsx: true)

  fileprivate static let json = LangDef {
    $0.lineComments = slash
    $0.blockComments = cBlock
    $0.strings = [dq, sq]
    $0.constants = words("true false null NaN Infinity")
    $0.jsonKeys = true
    $0.capitalizedTypes = false
    $0.calls = false
    $0.properties = false
  }

  fileprivate static let python = LangDef {
    $0.lineComments = [p("#")]
    $0.strings = [
      StrRule("\"\"\"", multiline: true), StrRule("'''", multiline: true), dq, sq,
    ]
    $0.stringPrefixes = words("r u b f rb br fr rf t")
    $0.keywords = words(
      "and as assert async await break class continue def del elif else except finally for from "
        + "global if import in is lambda nonlocal not or pass raise return try while with yield "
        + "match case")
    $0.types = words(
      "int float str bool bytes list dict set tuple object complex frozenset bytearray")
    $0.constants = words("True False None self cls NotImplemented Ellipsis")
    $0.functionDecl = ["def"]
    $0.typeDecl = ["class"]
    $0.atKind = .attribute
  }

  fileprivate static let rust = LangDef {
    $0.lineComments = slash
    $0.blockComments = cBlock
    $0.nestedComments = true
    $0.strings = [StrRule("\"", multiline: true)]
    $0.stringPrefixes = words("b c")
    $0.keywords = words(
      "as async await break const continue crate dyn else enum extern fn for if impl in let loop "
        + "match mod move mut pub ref return static struct super trait type unsafe use where while "
        + "yield macro_rules union")
    $0.types = words(
      "i8 i16 i32 i64 i128 isize u8 u16 u32 u64 u128 usize f32 f64 bool char str")
    $0.constants = words("true false self")
    $0.functionDecl = ["fn"]
    $0.typeDecl = words("struct enum trait type union")
    $0.rustAttrs = true
    $0.rustLifetimes = true
    $0.macroBang = true
    $0.rustRaw = true
  }

  fileprivate static let go = LangDef {
    $0.lineComments = slash
    $0.blockComments = cBlock
    $0.strings = [dq, StrRule("`", escape: 0, multiline: true), sq]
    $0.keywords = words(
      "break case chan const continue default defer else fallthrough for func go goto if import "
        + "interface map package range return select struct switch type var")
    $0.types = words(
      "bool byte complex64 complex128 error float32 float64 int int8 int16 int32 int64 rune "
        + "string uint uint8 uint16 uint32 uint64 uintptr any comparable")
    $0.constants = words("true false nil iota")
    $0.functionDecl = ["func"]
    $0.typeDecl = ["type"]
    $0.constructorCalls = false
  }

  fileprivate static let cKeywords =
    "auto break case const continue default do else enum extern for goto if inline register "
    + "restrict return sizeof static struct switch typedef union volatile while _Alignas _Alignof "
    + "_Atomic _Bool _Complex _Generic _Imaginary _Noreturn _Static_assert _Thread_local alignas "
    + "alignof static_assert thread_local typeof constexpr"
  fileprivate static let cTypes =
    "char double float int long short signed unsigned void bool size_t ssize_t ptrdiff_t "
    + "intptr_t uintptr_t int8_t int16_t int32_t int64_t uint8_t uint16_t uint32_t uint64_t "
    + "wchar_t"

  fileprivate static func cFamily(cpp: Bool, objc: Bool) -> LangDef {
    LangDef {
      $0.lineComments = slash
      $0.blockComments = cBlock
      $0.strings = [dq, sq]
      $0.stringPrefixes = words("l u u8")
      var kw = cKeywords
      var constants = "NULL true false"
      var types = cTypes
      if cpp {
        kw +=
          " and and_eq asm bitand bitor catch class compl concept consteval constinit const_cast "
          + "co_await co_return co_yield decltype delete dynamic_cast explicit export friend "
          + "mutable namespace new noexcept not not_eq operator or or_eq private protected public "
          + "reinterpret_cast requires static_cast template throw try typeid typename using "
          + "virtual xor xor_eq override final import module"
        constants += " nullptr this"
        types += " char8_t char16_t char32_t"
        $0.cppRaw = true
      }
      if objc {
        kw +=
          " interface implementation end protocol property synthesize dynamic selector encode "
          + "class optional required import autoreleasepool try catch finally throw synchronized "
          + "public private protected package in out inout bycopy byref oneway __block __weak "
          + "__strong __unsafe_unretained __bridge nonatomic atomic strong weak copy assign "
          + "retain readonly readwrite nullable nonnull getter setter"
        constants += " YES NO nil Nil self super"
        types += " id SEL IMP BOOL instancetype"
        $0.strings.append(StrRule("@\""))
        $0.atKind = .keyword
      }
      $0.keywords = words(kw)
      $0.constants = words(constants)
      $0.types = words(types)
      $0.typeDecl = words("struct enum union class namespace concept")
      $0.hashPreprocessor = true
      $0.constructorCalls = cpp
    }
  }
  fileprivate static let c = cFamily(cpp: false, objc: false)
  fileprivate static let cpp = cFamily(cpp: true, objc: false)
  fileprivate static let objc = cFamily(cpp: false, objc: true)

  fileprivate static let csharp = LangDef {
    $0.lineComments = slash
    $0.blockComments = cBlock
    $0.strings = [
      StrRule("$@\"", "\"", escape: 0, doubled: true, multiline: true, interp: .brace),
      StrRule("@$\"", "\"", escape: 0, doubled: true, multiline: true, interp: .brace),
      StrRule("@\"", "\"", escape: 0, doubled: true, multiline: true),
      StrRule("\"\"\"", escape: 0, multiline: true),
      StrRule("$\"", "\"", interp: .brace), dq, sq,
    ]
    $0.keywords = words(
      "abstract as base break case catch checked class const continue default delegate do else "
        + "enum event explicit extern finally fixed for foreach goto if implicit in interface "
        + "internal is lock namespace new operator out override params private protected public "
        + "readonly ref return sealed sizeof stackalloc static struct switch throw try typeof "
        + "unchecked unsafe using virtual volatile while add alias ascending async await by "
        + "descending dynamic equals from get global group into join let nameof on orderby "
        + "partial remove select set value var when where yield record init required with file "
        + "scoped not and or")
    $0.constants = words("true false null this base")
    $0.types = words(
      "bool byte char decimal double float int long object sbyte short string uint ulong ushort "
        + "void nint nuint")
    $0.typeDecl = words("class struct interface enum record namespace")
    $0.hashPreprocessor = true
    $0.constructorCalls = false
  }

  fileprivate static let java = LangDef {
    $0.lineComments = slash
    $0.blockComments = cBlock
    $0.strings = [StrRule("\"\"\"", multiline: true), dq, sq]
    $0.keywords = words(
      "abstract assert break case catch class const continue default do else enum extends final "
        + "finally for goto if implements import instanceof interface native new package private "
        + "protected public return static strictfp switch synchronized throw throws transient try "
        + "volatile while var record sealed permits yield module requires exports")
    $0.constants = words("true false null this super")
    $0.types = words("boolean byte char double float int long short void")
    $0.typeDecl = words("class interface enum record")
    $0.atKind = .attribute
  }

  fileprivate static let groovy = LangDef {
    $0.lineComments = slash
    $0.blockComments = cBlock
    $0.strings = [
      StrRule("\"\"\"", multiline: true, interp: .dollarIdent), StrRule("'''", multiline: true),
      StrRule("\"", interp: .dollarIdent), sq,
    ]
    $0.keywords = words(
      "abstract as assert break case catch class const continue def default do else enum extends "
        + "final finally for goto if implements import in instanceof interface native new package "
        + "private protected public return static switch synchronized throw throws trait "
        + "transient try var void volatile while")
    $0.constants = words("true false null this super it")
    $0.types = words("boolean byte char double float int long short")
    $0.functionDecl = ["def"]
    $0.typeDecl = words("class interface enum trait")
    $0.atKind = .attribute
  }

  fileprivate static let kotlin = LangDef {
    $0.lineComments = slash
    $0.blockComments = cBlock
    $0.nestedComments = true
    $0.strings = [
      StrRule("\"\"\"", escape: 0, multiline: true, interp: .dollarIdent),
      StrRule("\"", interp: .dollarIdent), sq,
    ]
    $0.keywords = words(
      "as break class continue do else for fun if in interface is object package return throw "
        + "try typealias typeof val var when while by catch constructor delegate dynamic field "
        + "file finally get import init param property receiver set setparam where actual "
        + "abstract annotation companion const crossinline data enum expect external final infix "
        + "inline inner internal lateinit noinline open operator out override private protected "
        + "public reified sealed suspend tailrec vararg value")
    $0.constants = words("true false null this super it")
    $0.functionDecl = ["fun"]
    $0.typeDecl = words("class interface object typealias")
    $0.atKind = .attribute
  }

  fileprivate static let scala = LangDef {
    $0.lineComments = slash
    $0.blockComments = cBlock
    $0.nestedComments = true
    $0.strings = [
      StrRule("\"\"\"", escape: 0, multiline: true, interp: .dollarIdent),
      StrRule("\"", interp: .dollarIdent), sq,
    ]
    $0.stringPrefixes = words("s f raw")
    $0.keywords = words(
      "abstract case catch class def do else enum export extends final finally for forSome given "
        + "if implicit import lazy match new object override package private protected return "
        + "sealed throw trait try type using val var while with yield extension derives end "
        + "inline opaque transparent then")
    $0.constants = words("true false null this super")
    $0.functionDecl = ["def"]
    $0.typeDecl = words("class object trait type enum")
    $0.atKind = .attribute
  }

  fileprivate static let dart = LangDef {
    $0.lineComments = slash
    $0.blockComments = cBlock
    $0.strings = [
      StrRule("\"\"\"", multiline: true, interp: .dollarIdent),
      StrRule("'''", multiline: true, interp: .dollarIdent),
      StrRule("\"", interp: .dollarIdent), StrRule("'", interp: .dollarIdent),
    ]
    $0.stringPrefixes = ["r"]
    $0.keywords = words(
      "abstract as assert async await base break case catch class const continue covariant "
        + "default deferred do dynamic else enum export extends extension external factory final "
        + "finally for get hide if implements import in interface is late library mixin new on "
        + "operator part required rethrow return sealed set show static switch sync throw try "
        + "typedef var void when while with yield")
    $0.constants = words("true false null this super")
    $0.types = words("int double num bool dynamic")
    $0.typeDecl = words("class mixin enum extension typedef")
    $0.atKind = .attribute
  }

  fileprivate static let ruby = LangDef {
    $0.lineComments = [p("#")]
    $0.lineStartBlocks = [Delim("=begin", "=end")]
    $0.strings = [
      StrRule("\"", multiline: true, interp: .hashBrace), StrRule("'", multiline: true),
      StrRule("`", multiline: true, interp: .hashBrace),
    ]
    $0.keywords = words(
      "BEGIN END alias and begin break case class def defined? do else elsif end ensure for if "
        + "in module next not or redo rescue retry return super then undef unless until when "
        + "while yield require require_relative include extend prepend attr_accessor attr_reader "
        + "attr_writer private protected public raise loop lambda proc")
    $0.constants = words("true false nil self __FILE__ __LINE__ __method__")
    $0.functionDecl = ["def"]
    $0.typeDecl = words("class module")
    $0.atKind = .property
    $0.sigils = [A.dollar]
    $0.identSuffix = [A.question, A.bang]
    $0.symbols = true
    $0.regex = true
  }

  fileprivate static let php = LangDef {
    $0.lineComments = [p("//"), p("#")]
    $0.blockComments = cBlock
    $0.strings = [
      StrRule("\"", multiline: true, interp: .dollarIdent), StrRule("'", multiline: true),
      StrRule("`", multiline: true),
    ]
    $0.keywords = words(
      "abstract and array as break callable case catch class clone const continue declare "
        + "default do echo else elseif empty enddeclare endfor endforeach endif endswitch endwhile "
        + "enum extends final finally fn for foreach function global goto if implements include "
        + "include_once instanceof insteadof interface isset list match namespace new or print "
        + "private protected public readonly require require_once return static switch throw "
        + "trait try unset use var while xor yield from")
    $0.constants = words("true false null self parent __class__ __dir__ __file__ __line__")
    $0.types = words("int float bool string object mixed void never iterable")
    $0.functionDecl = words("function fn")
    $0.typeDecl = words("class interface trait enum")
    $0.caseInsensitive = true
    $0.sigils = [A.dollar]
    $0.rustAttrs = true
  }

  fileprivate static let powershell = LangDef {
    $0.lineComments = [p("#")]
    $0.blockComments = [Delim("<#", "#>")]
    $0.strings = [
      StrRule("@\"", "\"@", escape: A.backtick, multiline: true, interp: .dollarIdent),
      StrRule("@'", "'@", escape: 0, multiline: true),
      StrRule("\"", escape: A.backtick, multiline: true, interp: .dollarIdent),
      StrRule("'", escape: 0, doubled: true, multiline: true),
    ]
    $0.keywords = words(
      "begin break catch class continue data define do dynamicparam else elseif end enum exit "
        + "filter finally for foreach from function hidden if in param process return static "
        + "switch throw trap try until using var while workflow parallel sequence")
    $0.constants = words("true false null")
    $0.types = words(
      "string int long bool array hashtable object void psobject datetime double char byte "
        + "switch scriptblock pscustomobject")
    $0.functionDecl = ["function"]
    $0.typeDecl = words("class enum")
    $0.caseInsensitive = true
    $0.capitalizedTypes = false
    $0.sigils = [A.dollar]
    $0.identExtra = [A.minus]
    $0.dashParams = true
    $0.dashFunctions = true
  }

  fileprivate static let sql = LangDef {
    $0.lineComments = [p("--")]
    $0.blockComments = cBlock
    $0.strings = [
      StrRule("'", escape: 0, doubled: true, multiline: true), StrRule("\"", escape: 0),
      StrRule("`", escape: 0),
    ]
    $0.keywords = words(
      "select from where insert into values update set delete create table drop alter add column "
        + "index primary key foreign references constraint unique not default and or in is like "
        + "between exists join inner left right full outer cross on as group by order having "
        + "limit offset union all distinct case when then else end begin commit rollback "
        + "transaction view trigger procedure function returns return declare if while database "
        + "schema grant revoke with recursive asc desc cascade check top fetch next rows only over "
        + "partition window returning conflict do nothing replace temporary temp explain analyze "
        + "use show describe truncate merge using matched lateral natural except intersect any "
        + "some ilike similar collate auto_increment serial identity generated always stored "
        + "virtual language security definer extension materialized sequence type enum owner to")
    $0.types = words(
      "int integer smallint bigint tinyint decimal numeric real float double precision char "
        + "varchar text nchar nvarchar ntext date time timestamp timestamptz datetime interval "
        + "boolean bool blob clob binary varbinary uuid json jsonb xml money bit bytea")
    $0.constants = words("null true false current_date current_time current_timestamp")
    $0.caseInsensitive = true
    $0.capitalizedTypes = false
    $0.atKind = .property
  }

  fileprivate static let toml = LangDef {
    $0.lineComments = [p("#")]
    $0.strings = [
      StrRule("\"\"\"", multiline: true), StrRule("'''", escape: 0, multiline: true), dq,
      StrRule("'", escape: 0),
    ]
    $0.constants = words("true false inf nan")
    $0.keyLines = true
    $0.assignKeys = true
    $0.capitalizedTypes = false
    $0.calls = false
    $0.properties = false
    $0.identExtra = [A.minus]
  }

  fileprivate static let ini = LangDef {
    $0.lineComments = [p(";"), p("#")]
    $0.strings = [StrRule("\"", escape: 0), StrRule("'", escape: 0)]
    $0.constants = words("true false yes no on off null")
    $0.caseInsensitive = true
    $0.keyLines = true
    $0.capitalizedTypes = false
    $0.calls = false
    $0.properties = false
    $0.identExtra = [A.minus, A.dot]
  }

  fileprivate static let lua = LangDef {
    $0.lineComments = [p("--")]
    $0.blockComments = [Delim("--[[", "]]"), Delim("--[==[", "]==]"), Delim("--[=[", "]=]")]
    $0.strings = [StrRule("[[", "]]", escape: 0, multiline: true), dq, sq]
    $0.keywords = words(
      "and break do else elseif end for function goto if in local not or repeat return then "
        + "until while")
    $0.constants = words("true false nil self")
    $0.functionDecl = ["function"]
    $0.capitalizedTypes = false
  }

  fileprivate static let zig = LangDef {
    $0.lineComments = slash
    $0.lineStrings = [p("\\\\")]
    $0.strings = [dq, sq]
    $0.keywords = words(
      "addrspace align allowzero and anyframe anytype asm async await break callconv catch "
        + "comptime const continue defer else enum errdefer error export extern fn for if inline "
        + "linksection noalias noinline nosuspend opaque or orelse packed pub resume return "
        + "struct suspend switch test threadlocal try union unreachable usingnamespace var "
        + "volatile while")
    $0.constants = words("true false null undefined")
    $0.types = words(
      "i8 i16 i32 i64 i128 isize u8 u16 u32 u64 u128 usize f16 f32 f64 f80 f128 bool void "
        + "noreturn type anyerror anyopaque comptime_int comptime_float c_int c_uint c_long "
        + "c_ulong c_char c_short")
    $0.functionDecl = ["fn"]
    $0.atKind = .function
  }

  fileprivate static let elixir = LangDef {
    $0.lineComments = [p("#")]
    $0.strings = [
      StrRule("\"\"\"", multiline: true, interp: .hashBrace),
      StrRule("'''", multiline: true, interp: .hashBrace),
      StrRule("\"", multiline: true, interp: .hashBrace), sq,
    ]
    $0.keywords = words(
      "after and catch cond do else end fn for if in not or quote raise receive rescue try "
        + "unless unquote use when with alias import require case def defp defmodule defmacro "
        + "defmacrop defstruct defprotocol defimpl defdelegate defguard defexception "
        + "defoverridable")
    $0.constants = words("true false nil __MODULE__ __ENV__ __DIR__")
    $0.functionDecl = words("def defp defmacro defmacrop defguard defdelegate")
    $0.typeDecl = words("defmodule defprotocol")
    $0.atKind = .attribute
    $0.identSuffix = [A.question, A.bang]
    $0.symbols = true
    $0.tildeSigils = true
  }

  fileprivate static let haskell = LangDef {
    $0.lineComments = [p("--")]
    $0.blockComments = [Delim("{-", "-}")]
    $0.nestedComments = true
    $0.strings = [dq, sq]
    $0.keywords = words(
      "as case class data default deriving do else family forall foreign hiding if import in "
        + "infix infixl infixr instance let mdo module newtype of proc qualified rec then type "
        + "where")
    $0.constants = words("True False")
    $0.typeDecl = words("data newtype type class")
    $0.identExtra = [A.squote]
    $0.calls = false
    $0.properties = false
    $0.haskellSigs = true
  }

  fileprivate static let graphql = LangDef {
    $0.lineComments = [p("#")]
    $0.strings = [StrRule("\"\"\"", multiline: true), dq]
    $0.keywords = words(
      "query mutation subscription fragment on type interface union enum input scalar schema "
        + "extend directive implements repeatable")
    $0.constants = words("true false null")
    $0.functionDecl = words("query mutation subscription fragment")
    $0.typeDecl = words("type interface union enum input scalar")
    $0.sigils = [A.dollar]
    $0.atKind = .attribute
  }

  fileprivate static let nix = LangDef {
    $0.lineComments = [p("#")]
    $0.blockComments = cBlock
    $0.strings = [
      StrRule("''", escape: 0, multiline: true, interp: .dollarBrace),
      StrRule("\"", multiline: true, interp: .dollarBrace),
    ]
    $0.keywords = words("assert else if in inherit let or rec then with import")
    $0.constants = words("true false null builtins")
    $0.identExtra = [A.minus, A.squote]
    $0.assignKeys = true
    $0.capitalizedTypes = false
    $0.calls = false
  }

  fileprivate static let r = LangDef {
    $0.lineComments = [p("#")]
    $0.strings = [StrRule("\"", multiline: true), StrRule("'", multiline: true)]
    $0.keywords = words("if else repeat while function for in next break return")
    $0.constants = words(
      "TRUE FALSE NULL NA NA_integer_ NA_real_ NA_character_ NA_complex_ Inf NaN T F")
    $0.identExtra = [A.dot]
    $0.properties = false
    $0.capitalizedTypes = false
  }

  fileprivate static let perl = LangDef {
    $0.lineComments = [p("#")]
    $0.lineStartBlocks = ["=pod", "=head", "=over", "=item", "=begin", "=for", "=encoding"].map {
      Delim($0, "=cut")
    }
    $0.strings = [
      StrRule("\"", multiline: true, interp: .dollarIdent), StrRule("'", multiline: true),
      StrRule("`", multiline: true),
    ]
    $0.keywords = words(
      "my our local sub package use require no return if elsif else unless while until for "
        + "foreach last next redo do eval die print printf say and or not xor eq ne lt gt le ge "
        + "cmp defined undef wantarray chomp chop push pop shift unshift splice split join keys "
        + "values each delete exists scalar ref bless open close BEGIN END")
    $0.constants = words("__PACKAGE__ __FILE__ __LINE__ __DATA__ __END__")
    $0.functionDecl = ["sub"]
    $0.typeDecl = ["package"]
    $0.sigils = [A.dollar, A.at, A.percent]
    $0.perlQuotes = true
    $0.regex = true
  }

  fileprivate static let protobuf = LangDef {
    $0.lineComments = slash
    $0.blockComments = cBlock
    $0.strings = [dq, sq]
    $0.keywords = words(
      "syntax edition package import option message enum service rpc returns stream oneof map "
        + "reserved extensions extend to max optional required repeated public weak")
    $0.types = words(
      "double float int32 int64 uint32 uint64 sint32 sint64 fixed32 fixed64 sfixed32 sfixed64 "
        + "bool string bytes")
    $0.constants = words("true false")
    $0.functionDecl = ["rpc"]
    $0.typeDecl = words("message enum service")
  }

  fileprivate static let hcl = LangDef {
    $0.lineComments = [p("#"), p("//")]
    $0.blockComments = cBlock
    $0.strings = [StrRule("\"", interp: .dollarBrace)]
    $0.keywords = words(
      "resource data variable output module provider locals terraform for in if else endif "
        + "endfor dynamic content each count backend required_providers")
    $0.types = words("string number bool list map set object tuple any")
    $0.constants = words("true false null")
    $0.assignKeys = true
    $0.identExtra = [A.minus]
    $0.capitalizedTypes = false
  }

  /// Arguments of non-shell Dockerfile instructions.
  fileprivate static let dockerArgs = LangDef {
    $0.strings = [dq, StrRule("'", escape: 0)]
    $0.keywords = ["as"]
    $0.caseInsensitive = true
    $0.capitalizedTypes = false
    $0.calls = false
    $0.properties = false
    $0.sigils = [A.dollar]
    $0.assignKeys = true
    $0.dashParams = true
    $0.shebang = false
  }
}

private enum A {
  static let tab: UInt16 = 9, nl: UInt16 = 10, cr: UInt16 = 13, space: UInt16 = 32
  static let bang: UInt16 = 33, dquote: UInt16 = 34, hash: UInt16 = 35, dollar: UInt16 = 36
  static let percent: UInt16 = 37, amp: UInt16 = 38, squote: UInt16 = 39, lparen: UInt16 = 40
  static let rparen: UInt16 = 41, star: UInt16 = 42, plus: UInt16 = 43, comma: UInt16 = 44
  static let minus: UInt16 = 45, dot: UInt16 = 46, slash: UInt16 = 47, zero: UInt16 = 48
  static let colon: UInt16 = 58, semi: UInt16 = 59, lt: UInt16 = 60, eq: UInt16 = 61
  static let gt: UInt16 = 62, question: UInt16 = 63, at: UInt16 = 64, lbracket: UInt16 = 91
  static let backslash: UInt16 = 92, rbracket: UInt16 = 93, underscore: UInt16 = 95
  static let backtick: UInt16 = 96, lbrace: UInt16 = 123, pipe: UInt16 = 124
  static let rbrace: UInt16 = 125, tilde: UInt16 = 126
}

@inline(__always) private func isDigit(_ c: UInt16) -> Bool { c &- 48 < 10 }
@inline(__always) private func isAlpha(_ c: UInt16) -> Bool { (c | 0x20) &- 97 < 26 }
@inline(__always) private func isUpper(_ c: UInt16) -> Bool { c &- 65 < 26 }
@inline(__always) private func isLower(_ c: UInt16) -> Bool { c &- 97 < 26 }
@inline(__always) private func isHex(_ c: UInt16) -> Bool { isDigit(c) || (c | 0x20) &- 97 < 6 }
@inline(__always) private func isIdentStart(_ c: UInt16) -> Bool {
  isAlpha(c) || c == 95 || c >= 0x80
}
@inline(__always) private func isIdentChar(_ c: UInt16) -> Bool { isIdentStart(c) || isDigit(c) }
@inline(__always) private func isSpace(_ c: UInt16) -> Bool {
  c == 32 || c == 9 || c == 13 || c == 12
}
@inline(__always) private func isWS(_ c: UInt16) -> Bool { isSpace(c) || c == 10 }
@inline(__always) private func isOp(_ c: UInt16) -> Bool {
  switch c {
  case 33, 37, 38, 42, 43, 45, 47, 58, 60, 61, 62, 63, 94, 124, 126: true
  default: false
  }
}

/// What came before, for telling `/regex/` and `<jsx>` apart from division and comparison.
private enum Sig {
  case start, op, open, close, keyword, value
  var allowsExpr: Bool { self == .start || self == .op || self == .open || self == .keyword }
}

private struct Tok {
  var s: Int
  var e: Int
  let k: TokenKind
}

// MARK: - Scanner

private final class Highlighter {
  let u: UnsafeBufferPointer<UInt16>
  var toks: [Tok] = []
  var depth = 0

  init(_ u: UnsafeBufferPointer<UInt16>) {
    self.u = u
    toks.reserveCapacity(u.count / 6)
  }

  func add(_ s: Int, _ e: Int, _ k: TokenKind) {
    guard e > s, k != .plain else { return }
    var s = s
    if let last = toks.last {
      if s < last.e {
        s = last.e
        if e <= s { return }
      }
      if last.e == s && last.k == k {
        toks[toks.count - 1].e = e
        return
      }
    }
    toks.append(Tok(s: s, e: e, k: k))
  }

  @inline(__always) func at(_ i: Int, _ end: Int, _ pat: [UInt16]) -> Bool {
    let n = pat.count
    if i + n > end { return false }
    var k = 0
    while k < n {
      if u[i + k] != pat[k] { return false }
      k += 1
    }
    return true
  }

  func eol(_ i: Int, _ end: Int) -> Int {
    var j = i
    while j < end && u[j] != A.nl { j += 1 }
    return j
  }

  func atLineStart(_ i: Int, _ start: Int) -> Bool {
    var k = i - 1
    while k >= start && (u[k] == A.space || u[k] == A.tab) { k -= 1 }
    return k < start || u[k] == A.nl
  }

  func word(_ s: Int, _ e: Int) -> String {
    String(decoding: UnsafeBufferPointer(rebasing: u[s..<e]), as: UTF16.self)
  }

  @inline(__always) func nested<T>(_ f: () -> T) -> T {
    depth += 1
    defer { depth -= 1 }
    return f()
  }

  var canNest: Bool { depth < 40 }

  /// End of a line, following backslash continuations.
  func logicalEnd(_ i: Int, _ end: Int) -> Int {
    var j = i
    while true {
      let le = eol(j, end)
      var k = le - 1
      while k >= j && (u[k] == A.space || u[k] == A.tab || u[k] == A.cr) { k -= 1 }
      if k >= j && u[k] == A.backslash && le < end {
        j = le + 1
        continue
      }
      return le
    }
  }

  func run(_ engine: Engine, _ s: Int, _ e: Int) {
    switch engine {
    case .code(let d): code(d, s, e)
    case .shell: shell(s, e)
    case .console: console(s, e)
    case .php: php(s, e)
    case .yaml: yaml(s, e)
    case .markdown: markdown(s, e)
    case .diff: diff(s, e)
    case .dockerfile: dockerfile(s, e)
    case .makefile: makefile(s, e)
    case .markup(let html): markup(s, e, html: html)
    case .css(let scss): css(s, e, scss: scss)
    case .plain: break
    }
  }

  func run(_ id: String, _ s: Int, _ e: Int) {
    guard let engine = SyntaxHighlighter.engines[id] else { return }
    nested { run(engine, s, e) }
  }

  // MARK: Generic code scanner

  /// Scans code. With `stop`, returns just past the first unbalanced `stop` character (for
  /// string interpolation and JSX expressions).
  @discardableResult
  func code(_ d: LangDef, _ start: Int, _ end: Int, stop: UInt16? = nil) -> Int {
    var i = start
    var last = Sig.start
    var pending: TokenKind? = nil
    var nest = 0
    let opener: UInt16 = stop == A.rparen ? A.lparen : A.lbrace
    while i < end {
      let c = u[i]
      if c == A.space || c == A.nl || c == A.tab || c == A.cr {
        i += 1
        continue
      }
      if let stop {
        if c == stop {
          if nest == 0 { return i + 1 }
          nest -= 1
        } else if c == opener {
          nest += 1
        }
      }
      if d.keyLines && atLineStart(i, start), let j = keyLine(i, end) {
        i = j
        last = .op
        pending = nil
        continue
      }
      if isIdentStart(c) || (d.dollarIdent && c == A.dollar) {
        i = ident(d, i, end, start, &last, &pending)
        continue
      }
      pending = nil
      let next: UInt16 = i + 1 < end ? u[i + 1] : 0

      if i == 0 && d.shebang && c == A.hash && next == A.bang {
        let e = eol(i, end)
        add(i, e, .comment)
        i = e
        continue
      }
      if d.startsSomething(c), let j = commentOrString(d, i, start, end) {
        i = j
        last = .value
        continue
      }
      if d.rustAttrs && c == A.hash
        && (next == A.lbracket || (next == A.bang && i + 2 < end && u[i + 2] == A.lbracket))
      {
        let e = matchBracket(i + (next == A.bang ? 2 : 1), end, A.lbracket, A.rbracket)
        add(i, e, .attribute)
        i = e
        continue
      }
      if c == A.hash && d.hashPreprocessor && atLineStart(i, start) {
        i = preprocessor(i, end)
        last = .open
        continue
      }
      if c == A.hash && d.rawHashStrings, let j = swiftRaw(i, end) {
        i = j
        last = .value
        continue
      }
      if c == A.hash && d.hashDirectives && isIdentStart(next) {
        var j = i + 1
        while j < end && isIdentChar(u[j]) { j += 1 }
        add(i, j, .preprocessor)
        i = j
        last = .keyword
        continue
      }
      if c == A.squote && d.rustLifetimes {
        i = rustQuote(i, end)
        last = .value
        continue
      }
      let prev: UInt16 = i > start ? u[i - 1] : 0
      if isDigit(c) || (c == A.dot && isDigit(next) && !isIdentChar(prev) && prev != A.dot) {
        let j = number(i, end)
        add(i, j, .number)
        i = j
        last = .value
        continue
      }
      if !d.sigils.isEmpty && d.sigils.contains(c), let j = sigil(d, i, end, last) {
        i = j
        last = .value
        continue
      }
      if c == A.at, let kind = d.atKind {
        var j = i + 1
        if j < end && u[j] == A.at { j += 1 }
        if j < end && isIdentStart(u[j]) {
          while j < end && isIdentChar(u[j]) { j += 1 }
          add(i, j, kind)
          i = j
          last = .value
          continue
        }
        if kind == .keyword { add(i, i + 1, .keyword) }
        i += 1
        last = .open
        continue
      }
      if c == A.slash && d.regex && last.allowsExpr, let j = regexEnd(i, end) {
        add(i, j, .regex)
        i = j
        last = .value
        continue
      }
      if c == A.lt && d.jsx && last.allowsExpr && (isAlpha(next) || next == A.gt) && canNest {
        i = nested { jsx(d, i, end) }
        last = .value
        continue
      }
      if c == A.colon && d.symbols && isIdentStart(next) && !(i > start && u[i - 1] == A.colon) {
        var j = i + 2
        while j < end && isIdentChar(u[j]) { j += 1 }
        if j < end && (u[j] == A.question || u[j] == A.bang) { j += 1 }
        add(i, j, .constant)
        i = j
        last = .value
        continue
      }
      if c == A.tilde && d.tildeSigils && isAlpha(next), let j = tildeSigil(i, end) {
        i = j
        last = .value
        continue
      }
      if c == A.minus && d.dashParams && (isAlpha(next) || (next == A.minus && i + 2 < end
        && isAlpha(u[i + 2]))) && (i == start || isWS(u[i - 1]) || u[i - 1] == A.lparen)
      {
        var j = i + 1
        while j < end && (isIdentChar(u[j]) || u[j] == A.minus) { j += 1 }
        add(i, j, .attribute)
        i = j
        last = .op
        continue
      }
      if isOp(c) {
        add(i, i + 1, .operator_)
        last = .op
      } else if c == A.lparen || c == A.lbracket || c == A.lbrace || c == A.comma || c == A.semi {
        last = .open
      } else if c == A.rparen || c == A.rbracket || c == A.rbrace || c == A.dot {
        last = .close
      }
      i += 1
    }
    return end
  }

  private func commentOrString(_ d: LangDef, _ i: Int, _ start: Int, _ end: Int) -> Int? {
    if !d.lineStartBlocks.isEmpty && (i == start || u[i - 1] == A.nl) {
      for b in d.lineStartBlocks where at(i, end, b.open) {
        var ls = eol(i, end) + 1
        while ls < end && !at(ls, end, b.close) { ls = eol(ls, end) + 1 }
        let e = min(eol(min(ls, end), end), end)
        add(i, e, .comment)
        return e
      }
    }
    for b in d.blockComments where at(i, end, b.open) {
      return blockComment(b, nested: d.nestedComments, i, end)
    }
    for marker in d.lineComments where at(i, end, marker) {
      let e = eol(i, end)
      add(i, e, .comment)
      return e
    }
    for marker in d.lineStrings where at(i, end, marker) {
      let e = eol(i, end)
      add(i, e, .string)
      return e
    }
    for rule in d.strings where at(i, end, rule.open) {
      return string(d, rule, i, i, end)
    }
    return nil
  }

  func blockComment(_ b: Delim, nested: Bool, _ i: Int, _ end: Int) -> Int {
    var j = i + b.open.count
    var level = 1
    let o0 = b.open[0]
    let c0 = b.close[0]
    while j < end {
      let c = u[j]
      if c == c0 && at(j, end, b.close) {
        j += b.close.count
        level -= 1
        if level == 0 { break }
        continue
      }
      if nested && c == o0 && at(j, end, b.open) {
        level += 1
        j += b.open.count
        continue
      }
      j += 1
    }
    let e = min(j, end)
    add(i, e, .comment)
    return e
  }

  func string(_ d: LangDef, _ r: StrRule, _ q: Int, _ ts: Int, _ end: Int) -> Int {
    var j = q + r.open.count
    var seg = ts
    let close0 = r.close[0]
    let closeLen = r.close.count
    while j < end {
      let c = u[j]
      if c == r.escape && r.escape != 0 {
        if r.interp == .backslashParen && j + 1 < end && u[j + 1] == A.lparen && canNest {
          add(seg, j, r.kind)
          j = nested { code(d, j + 2, end, stop: A.rparen) }
          seg = j
          continue
        }
        j += 2
        continue
      }
      if c == A.nl && !r.multiline { break }
      if c == close0 && at(j, end, r.close) {
        if r.doubled && at(j + closeLen, end, r.close) {
          j += 2 * closeLen
          continue
        }
        j += closeLen
        var kind = r.kind
        if d.jsonKeys && seg == ts {
          var k = j
          while k < end && isWS(u[k]) { k += 1 }
          if k < end && u[k] == A.colon { kind = .property }
        }
        add(seg, j, kind)
        return j
      }
      switch r.interp {
      case .dollarBrace, .dollarIdent:
        if c == A.dollar && j + 1 < end {
          if u[j + 1] == A.lbrace && canNest {
            add(seg, j, r.kind)
            j = nested { code(d, j + 2, end, stop: A.rbrace) }
            seg = j
            continue
          }
          if r.interp == .dollarIdent && isIdentStart(u[j + 1]) {
            add(seg, j, r.kind)
            var k = j + 1
            while k < end && isIdentChar(u[k]) { k += 1 }
            add(j, k, .property)
            seg = k
            j = k
            continue
          }
        }
      case .hashBrace:
        if c == A.hash && j + 1 < end && u[j + 1] == A.lbrace && canNest {
          add(seg, j, r.kind)
          j = nested { code(d, j + 2, end, stop: A.rbrace) }
          seg = j
          continue
        }
      case .brace:
        if c == A.lbrace {
          if j + 1 < end && u[j + 1] == A.lbrace {
            j += 2
            continue
          }
          if canNest {
            add(seg, j, r.kind)
            j = nested { code(d, j + 1, end, stop: A.rbrace) }
            seg = j
            continue
          }
        }
      case .none, .backslashParen:
        break
      }
      j += 1
    }
    let e = min(j, end)
    add(seg, e, r.kind)
    return e
  }

  func number(_ i: Int, _ end: Int) -> Int {
    var j = i
    if u[j] == A.zero && j + 1 < end, case let x = u[j + 1] | 0x20, x == 120 || x == 98 || x == 111
    {
      j += 2
      while j < end && (isHex(u[j]) || u[j] == A.underscore) { j += 1 }
    } else {
      while j < end && (isDigit(u[j]) || u[j] == A.underscore) { j += 1 }
      if j + 1 < end && u[j] == A.dot && isDigit(u[j + 1]) {
        j += 1
        while j < end && (isDigit(u[j]) || u[j] == A.underscore) { j += 1 }
      }
      if j < end && (u[j] | 0x20) == 101 {
        var k = j + 1
        if k < end && (u[k] == A.plus || u[k] == A.minus) { k += 1 }
        if k < end && isDigit(u[k]) {
          j = k
          while j < end && isDigit(u[j]) { j += 1 }
        }
      }
    }
    while j < end && (isIdentChar(u[j]) && u[j] < 0x80) { j += 1 }
    return j
  }

  private func ident(
    _ d: LangDef, _ i: Int, _ end: Int, _ start: Int, _ last: inout Sig,
    _ pending: inout TokenKind?
  ) -> Int {
    var j = i + 1
    while j < end {
      let c = u[j]
      if isIdentChar(c) || (d.dollarIdent && c == A.dollar) {
        j += 1
        continue
      }
      if !d.identExtra.isEmpty && d.identExtra.contains(c) {
        if c == A.squote || (j + 1 < end && isIdentChar(u[j + 1])) {
          j += 1
          continue
        }
      }
      break
    }
    if !d.identSuffix.isEmpty && j < end && d.identSuffix.contains(u[j])
      && !(j + 1 < end && (u[j + 1] == A.eq || u[j + 1] == A.colon))
    {
      j += 1
    }
    let len = j - i
    let w: String? = len <= d.maxWord ? word(i, j) : nil
    let n: UInt16 = j < end ? u[j] : 0

    if let w, j < end {
      if !d.stringPrefixes.isEmpty && (n == A.dquote || n == A.squote)
        && d.stringPrefixes.contains(w.lowercased())
      {
        for rule in d.strings where at(j, end, rule.open) {
          pending = nil
          last = .value
          return string(d, rule, j, i, end)
        }
      }
      if d.rustRaw && (w == "r" || w == "br" || w == "cr") && (n == A.hash || n == A.dquote),
        let e = hashRaw(j, end)
      {
        add(i, e, .string)
        last = .value
        return e
      }
      if d.cppRaw && n == A.dquote && ["R", "LR", "uR", "UR", "u8R"].contains(w),
        let e = cppRawString(j, end)
      {
        add(i, e, .string)
        last = .value
        return e
      }
      if d.perlQuotes && perlQuoteWords.contains(w) && isPerlDelimiter(n),
        let e = perlQuote(w, j, end)
      {
        add(i, e, w.hasPrefix("q") && w != "qr" ? .string : .regex)
        last = .value
        return e
      }
    }
    if d.macroBang && n == A.bang && j + 1 < end
      && (u[j + 1] == A.lparen || u[j + 1] == A.lbracket || u[j + 1] == A.lbrace)
    {
      add(i, j + 1, .function)
      pending = nil
      last = .value
      return j + 1
    }

    let prev: UInt16 = i > start ? u[i - 1] : 0
    let afterDot =
      d.properties
      && ((prev == A.dot && !(i - 2 >= start && u[i - 2] == A.dot))
        || (prev == A.gt && i - 2 >= start && u[i - 2] == A.minus))
    var kind = TokenKind.plain
    var nextPending: TokenKind? = nil
    var sig = Sig.value
    if d.symbols && n == A.colon && j + 1 < end && isWS(u[j + 1]) {
      // Ruby/Elixir keyword-list keys: `key: value`, `do: x`.
      add(i, j + 1, .constant)
      pending = nil
      last = .value
      return j + 1
    }
    if let w {
      let key = d.caseInsensitive ? w.lowercased() : w
      if !afterDot && d.keywords.contains(key) {
        kind = .keyword
        sig = .keyword
        if d.functionDecl.contains(key) {
          nextPending = .function
        } else if d.typeDecl.contains(key) {
          nextPending = .type
        }
      } else if d.constants.contains(key) {
        kind = .constant
      } else if !afterDot && d.types.contains(key) {
        kind = .type
      }
    }
    if kind == .plain {
      if let p = pending {
        kind = p
      } else if d.calls && n == A.lparen {
        // `URL(string:)`, `Some(x)`: a capitalised call is usually a constructor.
        let ctor = d.capitalizedTypes && d.constructorCalls && !afterDot && isTypeName(i, j)
        kind = ctor ? .type : .function
      } else if afterDot {
        kind = .property
      } else if d.dashFunctions && contains(A.minus, i, j) {
        kind = .function
      } else if d.assignKeys && isAssignment(j, end) {
        kind = .property
      } else if d.haskellSigs && atLineStart(i, start) && isDoubleColon(j, end) {
        kind = .function
      } else if d.capitalizedTypes && isTypeName(i, j) {
        kind = .type
      }
    }
    pending = nextPending
    add(i, j, kind)
    last = sig
    return j
  }

  private func contains(_ c: UInt16, _ s: Int, _ e: Int) -> Bool {
    var k = s
    while k < e {
      if u[k] == c { return true }
      k += 1
    }
    return false
  }

  /// `Foo`, `T`, `URL`; not `MAX_SIZE`.
  private func isTypeName(_ s: Int, _ e: Int) -> Bool {
    isUpper(u[s]) && (e - s <= 4 && !contains(A.underscore, s, e) || hasLower(s, e))
  }

  private func hasLower(_ s: Int, _ e: Int) -> Bool {
    var k = s
    while k < e {
      if isLower(u[k]) { return true }
      k += 1
    }
    return false
  }

  private func isAssignment(_ j: Int, _ end: Int) -> Bool {
    var k = j
    while k < end && isSpace(u[k]) { k += 1 }
    guard k < end && u[k] == A.eq else { return false }
    if k + 1 < end {
      let n = u[k + 1]
      if n == A.eq || n == A.gt || n == A.tilde { return false }
    }
    return true
  }

  private func isDoubleColon(_ j: Int, _ end: Int) -> Bool {
    var k = j
    while k < end && isSpace(u[k]) { k += 1 }
    return k + 1 < end && u[k] == A.colon && u[k + 1] == A.colon
  }

  /// TOML / INI line starts: `[section]` headers and `key =` names.
  private func keyLine(_ i: Int, _ end: Int) -> Int? {
    let le = eol(i, end)
    if u[i] == A.lbracket {
      var j = i
      while j < le && u[j] != A.rbracket { j += 1 }
      while j < le && u[j] == A.rbracket { j += 1 }
      add(i, j, .type)
      return j
    }
    var j = i
    while j < le {
      let c = u[j]
      if c == A.eq { break }
      if c == A.hash || c == A.semi || c == A.lbrace || c == A.lbracket { return nil }
      j += 1
    }
    guard j < le, j > i else { return nil }
    var k = j
    while k > i && isSpace(u[k - 1]) { k -= 1 }
    add(i, k, .property)
    return k
  }

  private func preprocessor(_ i: Int, _ end: Int) -> Int {
    var j = i + 1
    while j < end && isSpace(u[j]) { j += 1 }
    let ws = j
    while j < end && isIdentChar(u[j]) { j += 1 }
    add(i, j, .preprocessor)
    let directive = word(ws, j)
    if directive == "include" || directive == "import" || directive == "include_next" {
      var k = j
      while k < end && isSpace(u[k]) { k += 1 }
      if k < end && u[k] == A.lt {
        let le = eol(k, end)
        var m = k + 1
        while m < le && u[m] != A.gt { m += 1 }
        if m < le {
          add(k, m + 1, .string)
          return m + 1
        }
      }
    }
    return j
  }

  private func matchBracket(_ i: Int, _ end: Int, _ open: UInt16, _ close: UInt16) -> Int {
    var j = i
    var level = 0
    while j < end {
      let c = u[j]
      if c == open {
        level += 1
      } else if c == close {
        level -= 1
        if level == 0 { return j + 1 }
      } else if c == A.dquote {
        j += 1
        while j < end && u[j] != A.dquote && u[j] != A.nl {
          if u[j] == A.backslash { j += 1 }
          j += 1
        }
      }
      j += 1
    }
    return end
  }

  /// Swift `#"raw"#` and `##"""…"""##`.
  private func swiftRaw(_ i: Int, _ end: Int) -> Int? {
    var j = i
    while j < end && u[j] == A.hash { j += 1 }
    let hashes = j - i
    guard j < end && u[j] == A.dquote else { return nil }
    let multi = at(j, end, p("\"\"\""))
    var close = multi ? p("\"\"\"") : p("\"")
    close += Array(repeating: A.hash, count: hashes)
    j += multi ? 3 : 1
    while j < end {
      if u[j] == A.nl && !multi { break }
      if u[j] == A.dquote && at(j, end, close) {
        j += close.count
        break
      }
      j += 1
    }
    let e = min(j, end)
    add(i, e, .string)
    return e
  }

  /// Rust `r#"…"#` (j points just past the `r`).
  private func hashRaw(_ j: Int, _ end: Int) -> Int? {
    var k = j
    while k < end && u[k] == A.hash { k += 1 }
    guard k < end && u[k] == A.dquote else { return nil }
    let close = [A.dquote] + Array(repeating: A.hash, count: k - j)
    k += 1
    while k < end {
      if u[k] == A.dquote && at(k, end, close) { return k + close.count }
      k += 1
    }
    return end
  }

  private func cppRawString(_ j: Int, _ end: Int) -> Int? {
    var k = j + 1
    while k < end && k - j < 18 && u[k] != A.lparen && !isWS(u[k]) { k += 1 }
    guard k < end && u[k] == A.lparen else { return nil }
    let close = [A.rparen] + Array(u[(j + 1)..<k]) + [A.dquote]
    k += 1
    while k < end {
      if u[k] == A.rparen && at(k, end, close) { return k + close.count }
      k += 1
    }
    return end
  }

  private func rustQuote(_ i: Int, _ end: Int) -> Int {
    let n: UInt16 = i + 1 < end ? u[i + 1] : 0
    if n == A.backslash {
      var j = i + 2
      while j < end && j - i < 14 && u[j] != A.squote && u[j] != A.nl { j += 1 }
      let e = j < end && u[j] == A.squote ? j + 1 : i + 2
      add(i, e, .string)
      return e
    }
    let k = i + 1 + (n >= 0xD800 && n < 0xDC00 ? 2 : 1)
    if k < end && u[k] == A.squote && n != A.nl {
      add(i, k + 1, .string)
      return k + 1
    }
    if isIdentStart(n) {
      var j = i + 2
      while j < end && isIdentChar(u[j]) { j += 1 }
      add(i, j, .attribute)
      return j
    }
    add(i, i + 1, .operator_)
    return i + 1
  }

  private func sigil(_ d: LangDef, _ i: Int, _ end: Int, _ last: Sig) -> Int? {
    guard i + 1 < end else { return nil }
    let c = u[i]
    let n = u[i + 1]
    if c != A.dollar && !last.allowsExpr { return nil }
    if n == A.lbrace && c == A.dollar {
      let le = eol(i, end)
      var j = i + 2
      while j < le && u[j] != A.rbrace { j += 1 }
      let e = min(j + 1, le)
      add(i, e, .property)
      return e
    }
    if isIdentStart(n) || (c == A.dollar && isDigit(n)) {
      var j = i + 2
      while j < end && isIdentChar(u[j]) { j += 1 }
      let name = j - i - 1 <= d.maxWord ? word(i + 1, j) : ""
      let key = d.caseInsensitive ? name.lowercased() : name
      add(i, j, d.constants.contains(key) ? .constant : .property)
      return j
    }
    return nil
  }

  private func regexEnd(_ i: Int, _ end: Int) -> Int? {
    var j = i + 1
    var inClass = false
    if j < end && (u[j] == A.slash || u[j] == A.star || isSpace(u[j])) { return nil }
    while j < end {
      let c = u[j]
      if c == A.nl { return nil }
      if c == A.backslash {
        j += 2
        continue
      }
      if c == A.lbracket {
        inClass = true
      } else if c == A.rbracket {
        inClass = false
      } else if c == A.slash && !inClass {
        j += 1
        while j < end && isAlpha(u[j]) { j += 1 }
        return j
      }
      j += 1
    }
    return nil
  }

  private func jsx(_ d: LangDef, _ i: Int, _ end: Int) -> Int {
    var j = i + 1
    while j < end
      && (isIdentChar(u[j]) || u[j] == A.dot || u[j] == A.colon || u[j] == A.minus)
    {
      j += 1
    }
    add(i, j, .tag)
    var opened = false
    while j < end {
      let c = u[j]
      if isWS(c) {
        j += 1
        continue
      }
      if c == A.slash && j + 1 < end && u[j + 1] == A.gt {
        add(j, j + 2, .tag)
        return j + 2
      }
      if c == A.gt {
        add(j, j + 1, .tag)
        j += 1
        opened = true
        break
      }
      if c == A.lbrace {
        j = code(d, j + 1, end, stop: A.rbrace)
        continue
      }
      if isIdentStart(c) {
        let k = j
        while j < end && (isIdentChar(u[j]) || u[j] == A.minus || u[j] == A.colon) { j += 1 }
        add(k, j, .attribute)
        while j < end && isWS(u[j]) { j += 1 }
        if j < end && u[j] == A.eq {
          j += 1
          while j < end && isWS(u[j]) { j += 1 }
          if j < end && (u[j] == A.dquote || u[j] == A.squote) {
            let q = u[j]
            var m = j + 1
            while m < end && u[m] != q { m += 1 }
            m = min(m + 1, end)
            add(j, m, .string)
            j = m
          } else if j < end && u[j] == A.lbrace {
            j = code(d, j + 1, end, stop: A.rbrace)
          }
        }
        continue
      }
      return j
    }
    guard opened else { return j }
    while j < end {
      let c = u[j]
      if c == A.lt {
        if j + 1 < end && u[j + 1] == A.slash {
          var k = j + 2
          while k < end && u[k] != A.gt && u[k] != A.nl { k += 1 }
          let e = min(k + 1, end)
          add(j, e, .tag)
          return e
        }
        if j + 1 < end && (isAlpha(u[j + 1]) || u[j + 1] == A.gt) && canNest {
          j = nested { jsx(d, j, end) }
          continue
        }
      } else if c == A.lbrace && canNest {
        j = nested { code(d, j + 1, end, stop: A.rbrace) }
        continue
      }
      j += 1
    }
    return end
  }

  private func tildeSigil(_ i: Int, _ end: Int) -> Int? {
    var j = i + 1
    let letter = u[j]
    while j < end && isAlpha(u[j]) { j += 1 }
    guard j < end else { return nil }
    let open = u[j]
    let close: UInt16
    switch open {
    case A.lparen: close = A.rparen
    case A.lbracket: close = A.rbracket
    case A.lbrace: close = A.rbrace
    case A.lt: close = A.gt
    case A.slash, A.pipe, A.dquote, A.squote: close = open
    default: return nil
    }
    let heredoc = open == A.dquote && at(j, end, p("\"\"\""))
    j += heredoc ? 3 : 1
    while j < end {
      let c = u[j]
      if c == A.backslash {
        j += 2
        continue
      }
      if heredoc {
        if at(j, end, p("\"\"\"")) {
          j += 3
          break
        }
      } else if c == close {
        j += 1
        break
      }
      j += 1
    }
    while j < end && isAlpha(u[j]) { j += 1 }
    let e = min(j, end)
    add(i, e, letter == 114 || letter == 82 ? .regex : .string)
    return e
  }

  private let perlQuoteWords: Set<String> = ["q", "qq", "qw", "qx", "m", "qr", "s", "tr", "y"]

  private func isPerlDelimiter(_ c: UInt16) -> Bool {
    switch c {
    case A.slash, A.pipe, A.bang, A.lbrace, A.lparen, A.lbracket, A.lt, A.hash, A.tilde,
      A.percent, A.dquote, A.squote:
      true
    default: false
    }
  }

  private func perlQuote(_ w: String, _ j: Int, _ end: Int) -> Int? {
    let parts = (w == "s" || w == "tr" || w == "y") ? 2 : 1
    var k = j
    for part in 0..<parts {
      guard k < end else { return end }
      let open = u[k]
      let close: UInt16
      switch open {
      case A.lparen: close = A.rparen
      case A.lbracket: close = A.rbracket
      case A.lbrace: close = A.rbrace
      case A.lt: close = A.gt
      default: close = open
      }
      let bracketed = close != open
      var level = 1
      k += 1
      while k < end {
        let c = u[k]
        if c == A.backslash {
          k += 2
          continue
        }
        if bracketed && c == open {
          level += 1
        } else if c == close {
          level -= 1
          if level == 0 { break }
        }
        k += 1
      }
      if k >= end { return end }
      if part == 0 && parts == 2 && !bracketed {
        // `s/a/b/`: the middle delimiter opens the replacement.
        continue
      }
      k += 1
      if part == 0 && parts == 2 {
        while k < end && isWS(u[k]) { k += 1 }
      }
    }
    while k < end && isAlpha(u[k]) { k += 1 }
    return min(k, end)
  }

  // MARK: Shell

  private static let shKeywords = words(
    "if then else elif fi case esac for select while until do done in function time coproc "
      + "return break continue local export declare readonly typeset")
  private static let shThenCommand = words("if then else elif do while until time ! {")
  private static let shDecl = words("local export declare readonly typeset")
  private static let shPrefixCommands = words(
    "sudo doas time exec nohup xargs env command builtin nice watch")

  @discardableResult
  func shell(_ start: Int, _ end: Int, stop: Bool = false, make: Bool = false) -> Int {
    var i = start
    var cmd = true
    var decl = false
    var value = false
    var forContext = false
    var nest = 0
    var docs: [(term: String, dash: Bool)] = []
    while i < end {
      let c = u[i]
      if c == A.nl {
        i += 1
        if !docs.isEmpty {
          i = heredocBodies(&docs, i, end)
        }
        cmd = true
        decl = false
        value = false
        forContext = false
        continue
      }
      if isSpace(c) {
        value = false
        i += 1
        continue
      }
      if stop {
        if c == A.lparen {
          nest += 1
        } else if c == A.rparen {
          if nest == 0 { return i + 1 }
          nest -= 1
        }
      }
      let prev: UInt16 = i > start ? u[i - 1] : A.nl
      if c == A.hash && (isWS(prev) || prev == A.semi || prev == A.pipe || prev == A.amp
        || prev == A.lparen)
      {
        let e = eol(i, end)
        add(i, e, .comment)
        i = e
        continue
      }
      if c == A.backslash {
        i += 2
        continue
      }
      if c == A.dquote || c == A.squote || c == A.backtick || c == A.dollar {
        if c == A.dollar && cmd && atLineStart(i, start) && i + 1 < end && u[i + 1] == A.space {
          add(i, i + 1, .comment)
          i += 2
          continue
        }
        if c == A.dquote {
          i = shellDQ(i, end, make)
        } else if c == A.dollar {
          i = shellDollar(i, end, make)
        } else {
          var j = i + 1
          while j < end && u[j] != c {
            if c == A.backtick && u[j] == A.backslash { j += 1 }
            j += 1
          }
          j = min(j + 1, end)
          add(i, j, .string)
          i = j
        }
        if value { value = false } else { cmd = false }
        continue
      }
      if c == A.lt && i + 1 < end && u[i + 1] == A.lt && !(i + 2 < end && u[i + 2] == A.lt) {
        var j = i + 2
        var dash = false
        if j < end && u[j] == A.minus {
          dash = true
          j += 1
        }
        let opEnd = j
        while j < end && isSpace(u[j]) { j += 1 }
        var quote: UInt16 = 0
        if j < end && (u[j] == A.squote || u[j] == A.dquote) {
          quote = u[j]
          j += 1
        }
        let ws = j
        while j < end && (isIdentChar(u[j]) || u[j] == A.minus) { j += 1 }
        if j > ws {
          let term = word(ws, j)
          if quote != 0 && j < end && u[j] == quote { j += 1 }
          add(i, opEnd, .operator_)
          add(quote != 0 ? ws - 1 : ws, j, .string)
          docs.append((term, dash))
          i = j
          continue
        }
      }
      if c == A.pipe || c == A.amp || c == A.semi || c == A.lparen || c == A.rparen || c == A.lt
        || c == A.gt
        || ((c == A.lbrace || c == A.rbrace)
          && (i + 1 >= end || isWS(u[i + 1]) || u[i + 1] == A.semi))
      {
        if c != A.lparen && c != A.rparen && c != A.lbrace && c != A.rbrace {
          add(i, i + 1, .operator_)
        }
        if c == A.pipe || c == A.amp || c == A.semi || c == A.lparen || c == A.lbrace {
          cmd = true
          decl = false
          forContext = false
        }
        i += 1
        continue
      }
      // A word.
      var j = i
      var assign = -1
      while j < end {
        let ch = u[j]
        if isWS(ch) || ch == A.pipe || ch == A.amp || ch == A.semi || ch == A.lparen
          || ch == A.rparen || ch == A.lt || ch == A.gt || ch == A.dquote || ch == A.squote
          || ch == A.backtick || ch == A.dollar || ch == A.backslash
        {
          break
        }
        if ch == A.eq && (cmd || decl) && !value && j > i && isShellName(i, j) {
          assign = j
          break
        }
        j += 1
      }
      if j == i {
        i += 1
        continue
      }
      if assign >= 0 {
        add(i, assign, .property)
        add(assign, assign + 1, .operator_)
        i = assign + 1
        value = true
        continue
      }
      if value {
        value = false
        i = j
        continue
      }
      let w = j - i <= 12 ? word(i, j) : ""
      if cmd {
        if Self.shKeywords.contains(w) {
          add(i, j, .keyword)
          cmd = Self.shThenCommand.contains(w)
          decl = Self.shDecl.contains(w)
          forContext = w == "for" || w == "select" || w == "case"
        } else if u[i] == A.minus || isDigit(u[i]) {
          cmd = false
        } else {
          add(i, j, .function)
          cmd = Self.shPrefixCommands.contains(w)
        }
      } else if decl {
        add(i, j, .property)
      } else if forContext && w == "in" {
        add(i, j, .keyword)
      } else if allDigits(i, j) {
        add(i, j, .number)
      }
      i = j
    }
    return end
  }

  private func isShellName(_ s: Int, _ e: Int) -> Bool {
    guard isAlpha(u[s]) || u[s] == A.underscore else { return false }
    var k = s
    while k < e {
      if !(isIdentChar(u[k]) && u[k] < 0x80) { return false }
      k += 1
    }
    return true
  }

  private func allDigits(_ s: Int, _ e: Int) -> Bool {
    var k = s
    while k < e {
      if !isDigit(u[k]) { return false }
      k += 1
    }
    return true
  }

  private func heredocBodies(_ docs: inout [(term: String, dash: Bool)], _ i: Int, _ end: Int)
    -> Int
  {
    var i = i
    for doc in docs {
      let body = i
      var found = false
      while i < end {
        let le = eol(i, end)
        var k = i
        if doc.dash { while k < le && u[k] == A.tab { k += 1 } }
        var e = le
        while e > k && isSpace(u[e - 1]) { e -= 1 }
        if e - k == doc.term.utf16.count && word(k, e) == doc.term {
          add(body, e, .string)
          i = min(le + 1, end)
          found = true
          break
        }
        i = le + 1
      }
      if !found {
        add(body, end, .string)
        i = end
      }
    }
    docs.removeAll()
    return min(i, end)
  }

  private func shellDQ(_ i: Int, _ end: Int, _ make: Bool) -> Int {
    var j = i + 1
    var seg = i
    while j < end {
      let c = u[j]
      if c == A.backslash {
        j += 2
        continue
      }
      if c == A.dquote {
        j += 1
        add(seg, j, .string)
        return j
      }
      if c == A.dollar && j + 1 < end {
        let n = u[j + 1]
        if isIdentStart(n) || isDigit(n) || n == A.lbrace || n == A.lparen || n == A.at
          || n == A.question || n == A.hash || n == A.star || n == A.dollar
        {
          add(seg, j, .string)
          j = shellDollar(j, end, make)
          seg = j
          continue
        }
      }
      j += 1
    }
    add(seg, end, .string)
    return end
  }

  private func shellDollar(_ i: Int, _ end: Int, _ make: Bool) -> Int {
    guard i + 1 < end else { return end }
    let n = u[i + 1]
    if make && (n == A.lparen || n == A.lbrace) { return makeRef(i, end) }
    if make && n == A.dollar {
      var j = i + 2
      if j < end && u[j] == A.lbrace {
        while j < end && u[j] != A.rbrace && u[j] != A.nl { j += 1 }
        j = min(j + 1, end)
      } else {
        while j < end && isIdentChar(u[j]) { j += 1 }
      }
      add(i, j, .property)
      return j
    }
    if n == A.lparen {
      guard canNest else { return i + 2 }
      return nested { shell(i + 2, end, stop: true, make: make) }
    }
    if n == A.lbrace {
      var j = i + 2
      var level = 1
      while j < end && u[j] != A.nl {
        if u[j] == A.lbrace { level += 1 }
        if u[j] == A.rbrace {
          level -= 1
          if level == 0 { break }
        }
        j += 1
      }
      let e = min(j + 1, end)
      add(i, e, .property)
      return e
    }
    if isIdentStart(n) {
      var j = i + 2
      while j < end && isIdentChar(u[j]) { j += 1 }
      add(i, j, .property)
      return j
    }
    if isDigit(n) || n == A.at || n == A.star || n == A.hash || n == A.question || n == A.bang
      || n == A.dollar || n == A.minus || n == A.underscore
    {
      add(i, i + 2, .property)
      return i + 2
    }
    if n == A.squote {
      var j = i + 2
      while j < end && u[j] != A.squote {
        if u[j] == A.backslash { j += 1 }
        j += 1
      }
      let e = min(j + 1, end)
      add(i, e, .string)
      return e
    }
    return i + 1
  }

  /// Terminal transcripts: prompt lines are shell, everything else is output.
  func console(_ start: Int, _ end: Int) {
    var i = start
    while i < end {
      let le = eol(i, end)
      var promptEnd = -1
      let c = u[i]
      if (c == A.dollar || c == A.percent || c == 0x276F || c == 0x279C)
        && (i + 1 == le || u[i + 1] == A.space)
      {
        promptEnd = i + 1
      } else if at(i, le, p("PS ")) || at(i, le, p("PS>")) {
        var k = i
        while k < le && u[k] != A.gt { k += 1 }
        if k < le { promptEnd = k + 1 }
      } else {
        // `user@host:~/dir$ ` style prompts.
        var k = i
        var sawMarker = false
        while k < le && k - i < 80 && u[k] != A.space {
          if u[k] == A.at || u[k] == A.colon || u[k] == A.tilde { sawMarker = true }
          k += 1
        }
        if sawMarker && k > i && k < le
          && (u[k - 1] == A.dollar || u[k - 1] == A.hash || u[k - 1] == A.percent
            || u[k - 1] == A.gt)
        {
          promptEnd = k
        }
      }
      if promptEnd >= 0 {
        add(i, promptEnd, .comment)
        let ce = logicalEnd(promptEnd, end)
        shell(promptEnd, ce)
        i = ce + 1
      } else {
        if c == A.hash { add(i, le, .comment) }
        i = le + 1
      }
    }
  }

  // MARK: Make

  private static let makeDirectives = words(
    "include -include sinclude ifeq ifneq ifdef ifndef else endif define endef export unexport "
      + "override undefine vpath")
  private static let makeFunctions = words(
    "shell wildcard patsubst subst foreach call if or and filter filter-out dir notdir basename "
      + "suffix addprefix addsuffix join sort strip words word wordlist firstword lastword "
      + "origin flavor eval info warning error abspath realpath value file findstring")

  func makefile(_ start: Int, _ end: Int) {
    var i = start
    while i < end {
      let le = eol(i, end)
      if u[i] == A.tab {
        let ce = logicalEnd(i, end)
        var k = i + 1
        while k < ce && (u[k] == A.at || u[k] == A.minus || u[k] == A.plus) { k += 1 }
        add(i + 1, k, .operator_)
        shell(k, ce, make: true)
        i = ce + 1
        continue
      }
      var k = i
      while k < le && isSpace(u[k]) { k += 1 }
      if k == le {
        i = le + 1
        continue
      }
      if u[k] == A.hash {
        add(k, le, .comment)
        i = le + 1
        continue
      }
      var j = k
      while j < le && !isWS(u[j]) && u[j] != A.colon && u[j] != A.eq && u[j] != A.lparen {
        j += 1
      }
      let first = word(k, j)
      if Self.makeDirectives.contains(first) {
        add(k, j, .keyword)
        makeText(j, logicalEnd(j, end))
        i = logicalEnd(j, end) + 1
        continue
      }
      // Find the first `:` or `=` outside `$(…)`.
      var m = k
      var level = 0
      var op = -1
      while m < le {
        let c = u[m]
        if c == A.dollar && m + 1 < le && (u[m + 1] == A.lparen || u[m + 1] == A.lbrace) {
          level += 1
          m += 2
          continue
        }
        if (c == A.rparen || c == A.rbrace) && level > 0 { level -= 1 }
        if level == 0 && (c == A.colon || c == A.eq || c == A.hash) {
          op = m
          break
        }
        m += 1
      }
      let ce = logicalEnd(k, end)
      if op >= 0 && u[op] != A.hash {
        let isAssign =
          u[op] == A.eq || (op + 1 < le && u[op + 1] == A.eq)
          || (op + 2 < le && u[op + 1] == A.colon && u[op + 2] == A.eq)
        var nameEnd = op
        if isAssign && nameEnd > k && (u[nameEnd - 1] == A.question || u[nameEnd - 1] == A.plus
          || u[nameEnd - 1] == A.bang)
        {
          nameEnd -= 1
        }
        var trimmed = nameEnd
        while trimmed > k && isSpace(u[trimmed - 1]) { trimmed -= 1 }
        let kind: TokenKind = isAssign ? .property : (u[k] == A.dot ? .keyword : .function)
        if contains(A.dollar, k, trimmed) {
          makeText(k, trimmed)
        } else {
          add(k, trimmed, kind)
        }
        var opEnd = op + 1
        while opEnd < le && (u[opEnd] == A.eq || u[opEnd] == A.colon) { opEnd += 1 }
        add(nameEnd, opEnd, .operator_)
        makeText(opEnd, ce)
      } else {
        makeText(k, ce)
      }
      i = ce + 1
    }
  }

  private func makeText(_ s: Int, _ e: Int) {
    var j = s
    while j < e {
      let c = u[j]
      if c == A.dollar {
        j = makeRef(j, e)
        continue
      }
      if c == A.hash {
        let le = eol(j, e)
        add(j, le, .comment)
        j = le
        continue
      }
      j += 1
    }
  }

  private func makeRef(_ i: Int, _ end: Int) -> Int {
    guard i + 1 < end else { return end }
    let n = u[i + 1]
    if n == A.lparen || n == A.lbrace {
      let close = n == A.lparen ? A.rparen : A.rbrace
      var k = i + 2
      while k < end && (isIdentChar(u[k]) || u[k] == A.minus) { k += 1 }
      let name = k - i - 2 <= 12 ? word(i + 2, k) : ""
      if Self.makeFunctions.contains(name) && k < end && isSpace(u[k]) {
        add(i, k, .function)
        var j = k
        var level = 0
        while j < end && u[j] != A.nl {
          let c = u[j]
          if c == A.dollar && canNest {
            j = nested { makeRef(j, end) }
            continue
          }
          if c == n { level += 1 }
          if c == close {
            if level == 0 { return j + 1 }
            level -= 1
          }
          j += 1
        }
        return j
      }
      var j = i + 2
      var level = 0
      while j < end && u[j] != A.nl {
        if u[j] == n { level += 1 }
        if u[j] == close {
          if level == 0 { break }
          level -= 1
        }
        j += 1
      }
      let e = min(j + 1, end)
      add(i, e, .property)
      return e
    }
    if !isWS(n) {
      add(i, i + 2, .property)
      return i + 2
    }
    return i + 1
  }

  // MARK: Dockerfile

  private static let dockerInstructions = words(
    "FROM RUN CMD LABEL MAINTAINER EXPOSE ENV ADD COPY ENTRYPOINT VOLUME USER WORKDIR ARG ONBUILD "
      + "STOPSIGNAL HEALTHCHECK SHELL")

  func dockerfile(_ start: Int, _ end: Int) {
    var i = start
    while i < end {
      let le = eol(i, end)
      var k = i
      while k < le && isSpace(u[k]) { k += 1 }
      if k == le {
        i = le + 1
        continue
      }
      if u[k] == A.hash {
        add(k, le, .comment)
        i = le + 1
        continue
      }
      var j = k
      while j < le && isAlpha(u[j]) { j += 1 }
      let instruction = j - k <= 12 ? word(k, j).uppercased() : ""
      guard Self.dockerInstructions.contains(instruction) else {
        i = le + 1
        continue
      }
      add(k, j, .keyword)
      let ce = logicalEnd(j, end)
      var r = j
      while r < ce && isSpace(u[r]) { r += 1 }
      if ["RUN", "CMD", "ENTRYPOINT", "SHELL"].contains(instruction) {
        if r < ce && u[r] == A.lbracket {
          code(.json, r, ce)
        } else {
          shell(r, ce)
        }
      } else {
        code(.dockerArgs, r, ce)
      }
      i = ce + 1
    }
  }

  // MARK: PHP

  func php(_ start: Int, _ end: Int) {
    guard let first = find(p("<?"), start, end) else {
      code(.php, start, end)
      return
    }
    var i = start
    var open: Int? = first
    while let o = open {
      markup(i, o, html: true)
      var codeStart = o + 2
      if at(codeStart, end, p("php")) || at(codeStart, end, p("PHP")) {
        codeStart += 3
      } else if at(codeStart, end, p("=")) {
        codeStart += 1
      }
      add(o, codeStart, .preprocessor)
      let close = find(p("?>"), codeStart, end) ?? end
      code(.php, codeStart, close)
      if close >= end { return }
      add(close, close + 2, .preprocessor)
      i = close + 2
      open = find(p("<?"), i, end)
    }
    markup(i, end, html: true)
  }

  private func find(_ pat: [UInt16], _ s: Int, _ e: Int) -> Int? {
    var k = s
    let c0 = pat[0]
    while k < e {
      if u[k] == c0 && at(k, e, pat) { return k }
      k += 1
    }
    return nil
  }

  // MARK: HTML / XML

  func markup(_ start: Int, _ end: Int, html: Bool) {
    var i = start
    while i < end {
      let c = u[i]
      if c == A.lt {
        let n: UInt16 = i + 1 < end ? u[i + 1] : 0
        if at(i, end, p("<!--")) {
          let e = (find(p("-->"), i + 4, end)).map { $0 + 3 } ?? end
          add(i, e, .comment)
          i = e
        } else if at(i, end, p("<![CDATA[")) {
          let e = (find(p("]]>"), i + 9, end)).map { $0 + 3 } ?? end
          add(i, e, .string)
          i = e
        } else if n == A.bang || n == A.question {
          let e =
            (n == A.question ? find(p("?>"), i + 2, end).map { $0 + 2 }
              : find([A.gt], i + 2, end).map { $0 + 1 }) ?? end
          add(i, e, .preprocessor)
          i = e
        } else if n == A.slash {
          var j = i + 2
          while j < end && (isIdentChar(u[j]) || u[j] == A.minus || u[j] == A.colon
            || u[j] == A.dot)
          {
            j += 1
          }
          while j < end && isWS(u[j]) { j += 1 }
          if j < end && u[j] == A.gt { j += 1 }
          add(i, j, .tag)
          i = j
        } else if isAlpha(n) || n == A.underscore {
          i = tag(i, end, html: html)
        } else {
          i += 1
        }
        continue
      }
      if c == A.amp {
        var j = i + 1
        if j < end && u[j] == A.hash { j += 1 }
        while j < end && j - i < 12 && isIdentChar(u[j]) { j += 1 }
        if j < end && j > i + 1 && u[j] == A.semi {
          add(i, j + 1, .constant)
          i = j + 1
          continue
        }
      }
      i += 1
    }
  }

  private func tag(_ i: Int, _ end: Int, html: Bool) -> Int {
    var j = i + 1
    while j < end && (isIdentChar(u[j]) || u[j] == A.minus || u[j] == A.colon || u[j] == A.dot) {
      j += 1
    }
    let name = html && j - i <= 8 ? word(i + 1, j).lowercased() : ""
    add(i, j, .tag)
    var embedded: String? = name == "script" ? "javascript" : name == "style" ? "css" : nil
    var attribute = ""
    var afterEq = false
    var closed = false
    while j < end {
      let c = u[j]
      if isWS(c) {
        j += 1
        continue
      }
      if c == A.gt {
        add(j, j + 1, .tag)
        j += 1
        closed = true
        break
      }
      if c == A.slash && j + 1 < end && u[j + 1] == A.gt {
        add(j, j + 2, .tag)
        return j + 2
      }
      if c == A.lt { return j }
      if c == A.eq {
        afterEq = true
        j += 1
        continue
      }
      if c == A.dquote || c == A.squote {
        var k = j + 1
        while k < end && u[k] != c { k += 1 }
        let e = min(k + 1, end)
        add(j, e, .string)
        if embedded != nil {
          let value = k - j <= 40 ? word(j + 1, min(k, end)).lowercased() : ""
          if attribute == "lang" && (value == "ts" || value == "typescript") {
            embedded = "typescript"
          } else if attribute == "lang" && (value == "scss" || value == "less") {
            embedded = "scss"
          } else if attribute == "type" && value.contains("json") {
            embedded = "json"
          } else if attribute == "type" && (value.contains("template") || value.contains("html"))
          {
            embedded = "html"
          }
        }
        afterEq = false
        j = e
        continue
      }
      let k = j
      while j < end {
        let ch = u[j]
        if isWS(ch) || ch == A.eq || ch == A.gt || ch == A.dquote || ch == A.squote || ch == A.lt
          || (ch == A.slash && j + 1 < end && u[j + 1] == A.gt)
        {
          break
        }
        j += 1
      }
      if j == k {
        j += 1
        continue
      }
      if afterEq {
        add(k, j, .string)
        afterEq = false
      } else {
        add(k, j, .attribute)
        if html && j - k <= 8 { attribute = word(k, j).lowercased() }
      }
    }
    guard closed, let lang = embedded, j < end else { return j }
    let close = closingTag(name, j, end)
    if close > j && canNest { run(lang, j, close) }
    return close
  }

  private func closingTag(_ name: String, _ s: Int, _ e: Int) -> Int {
    let pat = Array(name.utf16)
    var k = s
    while k + 1 < e {
      if u[k] == A.lt && u[k + 1] == A.slash {
        var m = 0
        while m < pat.count && k + 2 + m < e && (u[k + 2 + m] | 0x20) == pat[m] { m += 1 }
        if m == pat.count { return k }
      }
      k += 1
    }
    return e
  }

  // MARK: CSS

  private enum CSSMode { case selector, declaration, atRule }

  func css(_ start: Int, _ end: Int, scss: Bool) {
    var i = start
    var mode = CSSMode.selector
    var fresh = true
    var value = false
    var nest = 0
    while i < end {
      let c = u[i]
      if isWS(c) {
        i += 1
        continue
      }
      let n: UInt16 = i + 1 < end ? u[i + 1] : 0
      if c == A.slash && n == A.star {
        i = blockComment(Delim("/*", "*/"), nested: false, i, end)
        continue
      }
      if scss && c == A.slash && n == A.slash {
        let e = eol(i, end)
        add(i, e, .comment)
        i = e
        continue
      }
      if fresh {
        fresh = false
        value = false
        mode = cssMode(i, end, nest)
      }
      if c == A.dquote || c == A.squote {
        var j = i + 1
        while j < end && u[j] != c && u[j] != A.nl {
          if u[j] == A.backslash { j += 1 }
          j += 1
        }
        let e = min(j + 1, end)
        add(i, e, .string)
        i = e
        continue
      }
      if c == A.lbrace || c == A.rbrace || c == A.semi {
        if c == A.lbrace { nest += 1 }
        if c == A.rbrace { nest = max(0, nest - 1) }
        fresh = true
        i += 1
        continue
      }
      if c == A.at || (c == A.dollar && scss) {
        var j = i + 1
        while j < end && (isIdentChar(u[j]) || u[j] == A.minus) { j += 1 }
        add(i, j, c == A.at && !value ? .keyword : .property)
        i = j
        continue
      }
      if c == A.hash {
        if n == A.lbrace {
          var j = i + 2
          while j < end && u[j] != A.rbrace && u[j] != A.nl { j += 1 }
          i = min(j + 1, end)
          continue
        }
        var j = i + 1
        while j < end && (isIdentChar(u[j]) || u[j] == A.minus) { j += 1 }
        add(i, j, mode == .selector ? .property : .number)
        i = j
        continue
      }
      if c == A.dot && mode == .selector && (isIdentStart(n) || n == A.minus) {
        var j = i + 1
        while j < end && (isIdentChar(u[j]) || u[j] == A.minus) { j += 1 }
        add(i, j, .type)
        i = j
        continue
      }
      if c == A.colon {
        if mode == .declaration && !value {
          value = true
          i += 1
          continue
        }
        if mode == .selector {
          var j = i + 1
          if j < end && u[j] == A.colon { j += 1 }
          while j < end && (isIdentChar(u[j]) || u[j] == A.minus) { j += 1 }
          add(i, j, .attribute)
          i = j
          continue
        }
        i += 1
        continue
      }
      if c == A.bang && isAlpha(n) {
        var j = i + 1
        while j < end && isAlpha(u[j]) { j += 1 }
        add(i, j, .keyword)
        i = j
        continue
      }
      if c == A.lbracket && mode == .selector {
        var j = i + 1
        while j < end && isWS(u[j]) { j += 1 }
        let k = j
        while j < end && (isIdentChar(u[j]) || u[j] == A.minus) { j += 1 }
        add(k, j, .attribute)
        i = j
        continue
      }
      if isDigit(c) || (c == A.dot && isDigit(n))
        || ((c == A.minus || c == A.plus) && (isDigit(n) || n == A.dot) && mode != .selector)
      {
        var j = i + 1
        while j < end && (isDigit(u[j]) || u[j] == A.dot) { j += 1 }
        while j < end && (isAlpha(u[j]) || u[j] == A.percent) { j += 1 }
        add(i, j, .number)
        i = j
        continue
      }
      if isIdentStart(c) || (c == A.minus && (isIdentStart(n) || n == A.minus)) {
        var j = i + 1
        while j < end && (isIdentChar(u[j]) || u[j] == A.minus) { j += 1 }
        if j < end && u[j] == A.lparen {
          add(i, j, .function)
          if j - i == 3 && word(i, j).lowercased() == "url" {
            var k = j + 1
            while k < end && u[k] != A.rparen && u[k] != A.nl { k += 1 }
            add(j + 1, k, .string)
            i = k
          } else {
            i = j
          }
          continue
        }
        switch mode {
        case .selector:
          add(i, j, .tag)
        case .declaration:
          if !value || (c == A.minus && n == A.minus) { add(i, j, .property) }
        case .atRule:
          var k = j
          while k < end && isSpace(u[k]) { k += 1 }
          if k < end && u[k] == A.colon { add(i, j, .property) }
        }
        i = j
        continue
      }
      if mode == .selector
        && (c == A.gt || c == A.plus || c == A.tilde || c == A.amp || c == A.star)
      {
        add(i, i + 1, .operator_)
      }
      i += 1
    }
  }

  private func cssMode(_ i: Int, _ end: Int, _ nest: Int) -> CSSMode {
    if u[i] == A.at { return .atRule }
    if u[i] == A.dollar || (u[i] == A.minus && i + 1 < end && u[i + 1] == A.minus) {
      return .declaration
    }
    var k = i
    while k < end {
      let c = u[k]
      if c == A.lbrace { return .selector }
      if c == A.semi || c == A.rbrace { return .declaration }
      if c == A.hash && k + 1 < end && u[k + 1] == A.lbrace {
        k += 2
        continue
      }
      k += 1
    }
    return nest > 0 ? .declaration : .selector
  }

  // MARK: Markdown

  func markdown(_ start: Int, _ end: Int) {
    var i = start
    if at(i, end, p("---")) && eol(i, end) == i + 3 {
      var ls = i + 4
      while ls < end {
        let le = eol(ls, end)
        if (at(ls, end, p("---")) || at(ls, end, p("..."))) && le == ls + 3 {
          add(i, i + 3, .comment)
          if canNest { run("yaml", i + 4, ls) }
          add(ls, le, .comment)
          i = min(le + 1, end)
          break
        }
        ls = le + 1
      }
    }
    var fence: (ch: UInt16, count: Int, lang: String?, body: Int)? = nil
    while i < end {
      let le = eol(i, end)
      var k = i
      while k < le && isSpace(u[k]) { k += 1 }
      let indent = k - i
      if let f = fence {
        var m = k
        while m < le && u[m] == f.ch { m += 1 }
        var tail = m
        while tail < le && isSpace(u[tail]) { tail += 1 }
        if m - k >= f.count && tail == le {
          fencedBody(f.lang, f.body, i)
          add(k, le, .comment)
          fence = nil
        }
        i = le + 1
        continue
      }
      if k < le && (u[k] == A.backtick || u[k] == A.tilde) {
        let ch = u[k]
        var m = k
        while m < le && u[m] == ch { m += 1 }
        if m - k >= 3 && !(ch == A.backtick && contains(A.backtick, m, le)) {
          let info = word(m, le)
          add(k, le, .comment)
          fence = (ch, m - k, SyntaxHighlighter.language(for: info), min(le + 1, end))
          i = le + 1
          continue
        }
      }
      if indent < 4 && k < le && u[k] == A.hash {
        var m = k
        while m < le && u[m] == A.hash { m += 1 }
        if m - k <= 6 && (m == le || isSpace(u[m])) {
          add(k, le, .heading)
          i = le + 1
          continue
        }
      }
      if indent < 4, let kind = ruleLine(k, le) {
        add(k, le, kind)
        i = le + 1
        continue
      }
      var m = k
      while m < le && u[m] == A.gt {
        add(m, m + 1, .keyword)
        m += 1
        while m < le && isSpace(u[m]) { m += 1 }
      }
      if m + 1 < le && (u[m] == A.minus || u[m] == A.star || u[m] == A.plus)
        && isSpace(u[m + 1])
      {
        add(m, m + 1, .keyword)
        m += 2
        if m + 3 <= le && u[m] == A.lbracket && u[m + 2] == A.rbracket
          && (u[m + 1] == A.space || u[m + 1] | 0x20 == 120)
        {
          add(m, m + 3, .keyword)
          m += 3
        }
      } else {
        var d = m
        while d < le && isDigit(u[d]) { d += 1 }
        if d > m && d - m < 10 && d + 1 < le && (u[d] == A.dot || u[d] == A.rparen)
          && isSpace(u[d + 1])
        {
          add(m, d + 1, .keyword)
          m = d + 1
        }
      }
      if m < le && u[m] == A.lbracket, let close = find([A.rbracket], m, le), close + 1 < le,
        u[close + 1] == A.colon
      {
        add(m, le, .link)
      } else {
        inline(m, le)
      }
      i = le + 1
    }
    if let f = fence { fencedBody(f.lang, f.body, end) }
  }

  private func fencedBody(_ lang: String?, _ s: Int, _ e: Int) {
    guard s < e else { return }
    if let lang, canNest {
      if lang != "plaintext" { run(lang, s, e) }
    } else {
      add(s, e, .string)
    }
  }

  /// Thematic breaks (`---`, `***`) and setext underlines (`===`).
  private func ruleLine(_ s: Int, _ e: Int) -> TokenKind? {
    guard s < e else { return nil }
    let c = u[s]
    guard c == A.minus || c == A.star || c == A.underscore || c == A.eq else { return nil }
    var count = 0
    var k = s
    while k < e {
      if u[k] == c {
        count += 1
      } else if !isSpace(u[k]) {
        return nil
      }
      k += 1
    }
    if c == A.eq { return .heading }
    return count >= 3 ? .comment : nil
  }

  private func inline(_ s: Int, _ e: Int) {
    var j = s
    while j < e {
      let c = u[j]
      let n: UInt16 = j + 1 < e ? u[j + 1] : 0
      if c == A.backslash {
        j += 2
        continue
      }
      if c == A.backtick {
        var m = j
        while m < e && u[m] == A.backtick { m += 1 }
        let run = m - j
        var k = m
        var found = -1
        while k < e {
          if u[k] == A.backtick {
            var r = k
            while r < e && u[r] == A.backtick { r += 1 }
            if r - k == run {
              found = r
              break
            }
            k = r
          } else {
            k += 1
          }
        }
        if found >= 0 {
          add(j, found, .string)
          j = found
        } else {
          j = m
        }
        continue
      }
      if (c == A.star || c == A.underscore) && n == c {
        if c == A.underscore && j > s && isIdentChar(u[j - 1]) {
          j += 2
          continue
        }
        if let close = find([c, c], j + 2, e), close > j + 2 {
          add(j, close + 2, .keyword)
          j = close + 2
        } else {
          j += 2
        }
        continue
      }
      if c == A.tilde && n == A.tilde {
        if let close = find([A.tilde, A.tilde], j + 2, e), close > j + 2 {
          add(j, close + 2, .deleted)
          j = close + 2
        } else {
          j += 2
        }
        continue
      }
      if c == A.lbracket || (c == A.bang && n == A.lbracket) {
        let open = c == A.bang ? j + 1 : j
        if let close = matchingBracket(open, e), close + 1 < e {
          let after = u[close + 1]
          if after == A.lparen, let paren = find([A.rparen], close + 1, e) {
            add(j, paren + 1, .link)
            j = paren + 1
            continue
          }
          if after == A.lbracket, let ref = find([A.rbracket], close + 1, e) {
            add(j, ref + 1, .link)
            j = ref + 1
            continue
          }
        }
        j += 1
        continue
      }
      if c == A.lt {
        if at(j, e, p("<http")) || at(j, e, p("<mailto:")) {
          if let close = find([A.gt], j, e) {
            add(j, close + 1, .link)
            j = close + 1
            continue
          }
        } else if isAlpha(n) || n == A.slash || n == A.bang, let close = find([A.gt], j, e),
          canNest
        {
          nested { markup(j, close + 1, html: true) }
          j = close + 1
          continue
        }
      }
      if c == 104 && (j == s || !isIdentChar(u[j - 1]))
        && (at(j, e, p("https://")) || at(j, e, p("http://")))
      {
        var k = j
        while k < e && !isWS(u[k]) && u[k] != A.gt && u[k] != A.rparen && u[k] != A.rbracket {
          k += 1
        }
        while k > j && [A.dot, A.comma, A.semi, A.colon, A.bang, A.question].contains(u[k - 1]) {
          k -= 1
        }
        add(j, k, .link)
        j = k
        continue
      }
      j += 1
    }
  }

  private func matchingBracket(_ i: Int, _ e: Int) -> Int? {
    var level = 0
    var k = i
    while k < e {
      if u[k] == A.backslash {
        k += 2
        continue
      }
      if u[k] == A.lbracket { level += 1 }
      if u[k] == A.rbracket {
        level -= 1
        if level == 0 { return k }
      }
      k += 1
    }
    return nil
  }

  // MARK: Diff

  func diff(_ start: Int, _ end: Int) {
    var i = start
    while i < end {
      let le = eol(i, end)
      let c = u[i]
      if (at(i, le, p("+++")) || at(i, le, p("---"))) && (i + 3 == le || isSpace(u[i + 3])) {
        add(i, le, .heading)
      } else if at(i, le, p("@@")) {
        let e = find(p("@@"), i + 2, le).map { $0 + 2 } ?? le
        add(i, e, .preprocessor)
      } else if c == A.plus || c == A.gt {
        add(i, le, .inserted)
      } else if c == A.minus || c == A.lt {
        add(i, le, .deleted)
      } else if at(i, le, p("diff ")) {
        add(i, le, .heading)
      } else if at(i, le, p("index ")) || at(i, le, p("new file")) || at(i, le, p("deleted file"))
        || at(i, le, p("similarity")) || at(i, le, p("rename ")) || at(i, le, p("Index: "))
        || at(i, le, p("===")) || c == A.backslash
      {
        add(i, le, .comment)
      }
      i = le + 1
    }
  }

  // MARK: YAML

  func yaml(_ start: Int, _ end: Int) {
    var i = start
    var blockIndent = -1
    while i < end {
      let le = eol(i, end)
      var k = i
      while k < le && (u[k] == A.space || u[k] == A.tab) { k += 1 }
      let indent = k - i
      if blockIndent >= 0 {
        if k == le || (k + 1 == le && u[k] == A.cr) {
          i = le + 1
          continue
        }
        if indent > blockIndent {
          add(k, le, .string)
          i = le + 1
          continue
        }
        blockIndent = -1
      }
      if k == le {
        i = le + 1
        continue
      }
      if u[k] == A.hash {
        add(k, le, .comment)
        i = le + 1
        continue
      }
      if indent == 0 && (at(k, le, p("---")) || at(k, le, p("...")))
        && (k + 3 == le || isSpace(u[k + 3]))
      {
        add(k, k + 3, .preprocessor)
        yamlValue(k + 3, le, indent, &blockIndent)
        i = le + 1
        continue
      }
      while k < le && u[k] == A.minus && (k + 1 == le || isSpace(u[k + 1])) {
        add(k, k + 1, .operator_)
        k += 1
        while k < le && isSpace(u[k]) { k += 1 }
      }
      if k < le, let (ks, ke, after) = yamlKey(k, le) {
        add(ks, ke, .property)
        k = after
      }
      yamlValue(k, le, indent, &blockIndent)
      i = le + 1
    }
  }

  private func yamlKey(_ k: Int, _ le: Int) -> (Int, Int, Int)? {
    let c = u[k]
    if c == A.dquote || c == A.squote {
      var j = k + 1
      while j < le && u[j] != c {
        if c == A.dquote && u[j] == A.backslash { j += 1 }
        j += 1
      }
      guard j < le else { return nil }
      var m = j + 1
      while m < le && isSpace(u[m]) { m += 1 }
      if m < le && u[m] == A.colon && (m + 1 == le || isWS(u[m + 1])) { return (k, j + 1, m + 1) }
      return nil
    }
    if c == A.lbracket || c == A.lbrace || c == A.hash || c == A.amp || c == A.star
      || c == A.bang || c == A.pipe || c == A.gt || c == A.percent || c == A.at
      || c == A.backtick
    {
      return nil
    }
    var j = k
    while j < le {
      if u[j] == A.colon && (j + 1 == le || isWS(u[j + 1])) {
        var e = j
        while e > k && isSpace(u[e - 1]) { e -= 1 }
        return (k, e, j + 1)
      }
      if u[j] == A.hash && j > k && isSpace(u[j - 1]) { return nil }
      j += 1
    }
    return nil
  }

  private func yamlValue(_ s: Int, _ le: Int, _ indent: Int, _ blockIndent: inout Int) {
    var j = s
    while j < le {
      let c = u[j]
      if isSpace(c) {
        j += 1
        continue
      }
      if c == A.hash && (j == s || isSpace(u[j - 1])) {
        add(j, le, .comment)
        return
      }
      if c == A.amp || c == A.star || c == A.bang {
        var k = j + 1
        while k < le && !isWS(u[k]) { k += 1 }
        add(j, k, c == A.bang ? .type : .attribute)
        j = k
        continue
      }
      if c == A.pipe || c == A.gt {
        var k = j + 1
        while k < le && (u[k] == A.plus || u[k] == A.minus || isDigit(u[k])) { k += 1 }
        add(j, k, .operator_)
        blockIndent = indent
        j = k
        continue
      }
      if c == A.dquote || c == A.squote {
        j = yamlQuoted(j, le)
        continue
      }
      if c == A.lbracket || c == A.lbrace {
        yamlFlow(j, le)
        return
      }
      var e = j
      while e < le && !(u[e] == A.hash && isSpace(u[e - 1])) { e += 1 }
      while e > j && isSpace(u[e - 1]) { e -= 1 }
      add(j, e, yamlScalar(j, e))
      j = e
    }
  }

  private func yamlQuoted(_ j: Int, _ le: Int) -> Int {
    let q = u[j]
    var k = j + 1
    while k < le {
      if q == A.dquote && u[k] == A.backslash {
        k += 2
        continue
      }
      if u[k] == q {
        if q == A.squote && k + 1 < le && u[k + 1] == A.squote {
          k += 2
          continue
        }
        k += 1
        break
      }
      k += 1
    }
    let e = min(k, le)
    add(j, e, .string)
    return e
  }

  private func yamlFlow(_ s: Int, _ le: Int) {
    var j = s
    while j < le {
      let c = u[j]
      if isSpace(c) || c == A.lbracket || c == A.rbracket || c == A.lbrace || c == A.rbrace
        || c == A.comma || c == A.colon
      {
        j += 1
        continue
      }
      if c == A.hash && isSpace(u[j - 1]) {
        add(j, le, .comment)
        return
      }
      if c == A.dquote || c == A.squote {
        let e = yamlQuoted(j, le)
        var m = e
        while m < le && isSpace(u[m]) { m += 1 }
        if m < le && u[m] == A.colon { recolor(j, e, .property) }
        j = e
        continue
      }
      var e = j
      while e < le {
        let ch = u[e]
        if ch == A.comma || ch == A.rbracket || ch == A.rbrace || ch == A.lbracket
          || ch == A.lbrace || (ch == A.colon && (e + 1 == le || isWS(u[e + 1])))
        {
          break
        }
        e += 1
      }
      var t = e
      while t > j && isSpace(u[t - 1]) { t -= 1 }
      add(j, t, e < le && u[e] == A.colon ? .property : yamlScalar(j, t))
      j = max(e, j + 1)
    }
  }

  private func recolor(_ s: Int, _ e: Int, _ k: TokenKind) {
    if let last = toks.last, last.s == s, last.e == e {
      toks[toks.count - 1] = Tok(s: s, e: e, k: k)
    }
  }

  private static let yamlConstants = words("true false yes no on off null ~ y n")

  private func yamlScalar(_ s: Int, _ e: Int) -> TokenKind {
    guard e > s else { return .plain }
    if e - s <= 5 && Self.yamlConstants.contains(word(s, e).lowercased()) { return .constant }
    let c = u[s]
    let n: UInt16 = s + 1 < e ? u[s + 1] : 0
    if isDigit(c) || ((c == A.minus || c == A.plus || c == A.dot) && isDigit(n)) {
      if e - s <= 32 {
        let w = word(s, e).replacingOccurrences(of: "_", with: "")
        if Double(w) != nil || (w.hasPrefix("0x") && Int(w.dropFirst(2), radix: 16) != nil)
          || (w.hasPrefix("0o") && Int(w.dropFirst(2), radix: 8) != nil)
        {
          return .number
        }
      }
    }
    if c == A.dot, e - s <= 5, [".inf", ".nan", "-.inf"].contains(word(s, e).lowercased()) {
      return .number
    }
    return .string
  }
}
