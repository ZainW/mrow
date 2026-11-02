import AppKit
import CryptoKit
import ImageIO

/// Where an image in a description or comment comes from. GitLab uploads sit behind the user's
/// login, so they go through `glab api` (which holds the token); anything else is a plain fetch.
enum ImageSource: Hashable, Sendable {
  case upload(project: String, secret: String, file: String)
  case web(URL)

  /// `/uploads/<secret>/<file>` (relative to the project), `/-/project/<id>/uploads/…`, or a full
  /// URL, which is an upload too when it's on the project's host.
  static func resolve(_ raw: String, base: URL?) -> (source: ImageSource, web: URL?)? {
    let raw = raw.trimmingCharacters(in: .whitespaces)
    let project = base.map { $0.path.trimmingCharacters(in: CharacterSet(charactersIn: "/")) }
    if raw.hasPrefix("/uploads/") || raw.hasPrefix("uploads/") {
      guard let base, let project, !project.isEmpty else { return nil }
      let path = raw.hasPrefix("/") ? String(raw.dropFirst()) : raw
      return upload(project: project, path: path).map { ($0, base.appendingPathComponent(path)) }
    }
    if raw.hasPrefix("/-/project/") {
      guard let base, var host = URLComponents(url: base, resolvingAgainstBaseURL: false) else { return nil }
      host.path = raw
      let parts = raw.split(separator: "/", maxSplits: 3).map(String.init)  // -, project, id, rest
      guard parts.count == 4 else { return nil }
      return upload(project: parts[2], path: parts[3]).map { ($0, host.url) }
    }
    guard let url = URL(string: raw), let scheme = url.scheme?.lowercased(), scheme == "http" || scheme == "https"
    else { return nil }
    if let host = base?.host, url.host == host, let range = url.path.range(of: "/uploads/") {
      var owner = String(url.path[..<range.lowerBound]).trimmingCharacters(in: CharacterSet(charactersIn: "/"))
      if owner.hasPrefix("-/project/") { owner.removeFirst("-/project/".count) }
      if owner.hasSuffix("/-") { owner.removeLast(2) }
      if !owner.isEmpty, let source = upload(project: owner, path: "uploads/" + url.path[range.upperBound...]) {
        return (source, url)
      }
    }
    return (.web(url), url)
  }

  private static func upload(project: String, path: String) -> ImageSource? {
    // uploads/<secret>/<file>
    let parts = path.split(separator: "/", maxSplits: 2).map(String.init)
    guard parts.count == 3, parts[0] == "uploads", !parts[1].isEmpty, !parts[2].isEmpty else { return nil }
    // Kept decoded; encoded once when it's fetched.
    return .upload(project: project, secret: parts[1], file: parts[2].removingPercentEncoding ?? parts[2])
  }

  var fileName: String {
    switch self {
    case .upload(_, _, let file): file
    case .web(let url): url.lastPathComponent.isEmpty ? "image" : url.lastPathComponent
    }
  }

  var key: String {
    switch self {
    case .upload(let project, let secret, let file): "upload|\(project)|\(secret)|\(file)"
    case .web(let url): "web|\(url.absoluteString)"
    }
  }
}

/// A decoded image, already scaled down to what a window can show.
struct LoadedImage: @unchecked Sendable {
  let image: CGImage
  /// The size it's meant to be shown at, in points (a Retina screenshot is half its pixels).
  let size: CGSize
  /// The original bytes on disk, for opening in Preview.
  let file: URL?
}

/// Fetches, caches and decodes images. Uploads never change (the secret is in the path), so their
/// bytes are kept on disk and a merge request opens with its screenshots straight away next time.
actor ImageLoader {
  static let shared = ImageLoader()

  /// Set by the benchmark, which replays a recording without the network.
  nonisolated(unsafe) static var offline = false

  /// Decoded images by `ImageSource.key`, bounded by their pixel bytes. NSCache is thread-safe.
  nonisolated(unsafe) private static let memory: NSCache<NSString, Box> = {
    let cache = NSCache<NSString, Box>()
    cache.totalCostLimit = 256 << 20
    return cache
  }()

  private final class Box {
    let value: LoadedImage
    init(_ value: LoadedImage) { self.value = value }
  }

  /// Without waiting: what's already decoded, so a redrawn view doesn't flash a placeholder.
  nonisolated static func cached(_ source: ImageSource) -> LoadedImage? {
    memory.object(forKey: source.key as NSString)?.value
  }

  private var running: [String: Task<LoadedImage?, Never>] = [:]
  // Each upload is a glab process; a description with twenty screenshots shouldn't start twenty.
  private var active = 0
  private var waiting: [CheckedContinuation<Void, Never>] = []
  private let limit = 4

  func image(_ source: ImageSource) async -> LoadedImage? {
    if let hit = Self.cached(source) { return hit }
    if let task = running[source.key] { return await task.value }
    let task = Task { await self.fetch(source) }
    running[source.key] = task
    let result = await task.value
    running[source.key] = nil
    return result
  }

  /// The original file on disk, fetching it if needed (videos aren't decoded, only opened).
  func file(_ source: ImageSource) async -> URL? {
    let file = Self.diskURL(source)
    if FileManager.default.fileExists(atPath: file.path) { return file }
    guard let data = await download(source) else { return nil }
    return (try? data.write(to: file, options: .atomic)) == nil ? nil : file
  }

  private func fetch(_ source: ImageSource) async -> LoadedImage? {
    let file = Self.diskURL(source)
    var data = try? Data(contentsOf: file)
    if data == nil {
      data = await download(source)
      if let data, case .upload = source { try? data.write(to: file, options: .atomic) }
    }
    guard let data else { return nil }
    let stored: URL? = FileManager.default.fileExists(atPath: file.path) ? file : nil
    let decoded = await Task.detached(priority: .userInitiated) { Self.decode(data, file: stored) }.value
    if let decoded {
      Self.memory.setObject(Box(decoded), forKey: source.key as NSString, cost: decoded.image.bytesPerRow * decoded.image.height)
    }
    return decoded
  }

  private func download(_ source: ImageSource) async -> Data? {
    guard !Self.offline else { return nil }
    await acquire()
    defer { release() }
    switch source {
    case .upload(let project, let secret, let file):
      let id = project.allSatisfy(\.isNumber)
        ? project : (project.addingPercentEncoding(withAllowedCharacters: .alphanumerics) ?? project)
      let name = file.addingPercentEncoding(withAllowedCharacters: .urlPathAllowed.subtracting(CharacterSet(charactersIn: "/"))) ?? file
      let data = try? await Glab.api(["projects/\(id)/uploads/\(secret)/\(name)"])
      // A missing upload comes back as a JSON error rather than a failure.
      guard let data, !data.isEmpty, data.first != UInt8(ascii: "{") else { return nil }
      return data
    case .web(let url):
      var request = URLRequest(url: url, timeoutInterval: 30)
      request.setValue("Mrow", forHTTPHeaderField: "User-Agent")
      guard let (data, response) = try? await URLSession.shared.data(for: request),
        (response as? HTTPURLResponse).map({ (200..<300).contains($0.statusCode) }) ?? true
      else { return nil }
      return data
    }
  }

  private func acquire() async {
    if active < limit {
      active += 1
      return
    }
    await withCheckedContinuation { waiting.append($0) }
  }

  private func release() {
    if waiting.isEmpty { active -= 1 } else { waiting.removeFirst().resume() }
  }

  /// Raster images are thumbnailed to at most 2400 pixels a side, so a 6000-pixel screenshot
  /// doesn't sit in memory at full size; SVGs (badges) go through NSImage, which draws them.
  private static func decode(_ data: Data, file: URL?) -> LoadedImage? {
    if let source = CGImageSourceCreateWithData(data as CFData, nil), CGImageSourceGetCount(source) > 0 {
      let properties = CGImageSourceCopyPropertiesAtIndex(source, 0, nil) as? [CFString: Any] ?? [:]
      let width = properties[kCGImagePropertyPixelWidth] as? CGFloat ?? 0
      let height = properties[kCGImagePropertyPixelHeight] as? CGFloat ?? 0
      let dpi = properties[kCGImagePropertyDPIWidth] as? CGFloat ?? 72
      let options: [CFString: Any] = [
        kCGImageSourceCreateThumbnailFromImageAlways: true,
        kCGImageSourceCreateThumbnailWithTransform: true,
        kCGImageSourceThumbnailMaxPixelSize: 2400,
        kCGImageSourceShouldCacheImmediately: true,
      ]
      if width > 0, height > 0, let image = CGImageSourceCreateThumbnailAtIndex(source, 0, options as CFDictionary) {
        // Mac screenshots say 144 dpi: they're meant to be shown at half their pixels.
        let scale = dpi > 100 ? 72 / dpi : 1
        return LoadedImage(image: image, size: CGSize(width: width * scale, height: height * scale), file: file)
      }
    }
    guard let image = NSImage(data: data), image.size.width > 0 else { return nil }
    var rect = CGRect(origin: .zero, size: image.size)
    guard let cg = image.cgImage(forProposedRect: &rect, context: nil, hints: [.ctm: AffineTransform(scale: 2)])
    else { return nil }
    return LoadedImage(image: cg, size: image.size, file: file)
  }

  private static let folder: URL = {
    let caches = FileManager.default.urls(for: .cachesDirectory, in: .userDomainMask)[0]
    let folder = caches.appendingPathComponent("Mrow/images", isDirectory: true)
    try? FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
    return folder
  }()

  /// Named after the original file (so Preview shows its name), prefixed by a hash of the source.
  private static func diskURL(_ source: ImageSource) -> URL {
    let hash = SHA256.hash(data: Data(source.key.utf8)).prefix(8).map { String(format: "%02x", $0) }.joined()
    let name = source.fileName.replacingOccurrences(of: "/", with: "_")
    return folder.appendingPathComponent("\(hash)-\(name)")
  }
}
