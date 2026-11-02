import AppKit
import SwiftUI

/// Mdow's flat chrome button: an icon, optionally a title and key hint, on a soft rounded
/// background that appears on hover.
struct ChromeButton: View {
  enum Style {
    /// A square icon button.
    case icon
    /// Icon and title, with a hover background.
    case text
    /// A quiet, always-filled pill that reads like a search field.
    case field
  }

  var symbol: String?
  var title: String
  var style: Style = .icon
  var hint: String?
  var isOn = false
  var symbolSize: CGFloat = 14
  var action: () -> Void

  @State private var hovering = false
  @Environment(\.isEnabled) private var isEnabled

  var body: some View {
    Button(action: action) {
      HStack(spacing: 7) {
        if let symbol {
          Image(systemName: symbol)
            .font(.system(size: symbolSize, weight: .medium))
            .frame(width: style == .icon ? nil : symbolSize)
        }
        if style != .icon {
          Text(title).font(.mrowUI(12.5, style == .field ? .regular : .medium))
          if let hint {
            Spacer(minLength: 14)
            Text(hint).font(.mrowMono(11)).foregroundStyle(Color.mrowTertiary)
          }
        }
      }
      .foregroundStyle(foreground)
      .padding(.horizontal, style == .icon ? 0 : 10)
      .frame(width: style == .icon ? 28 : nil, height: 28)
      .background(background, in: RoundedRectangle(cornerRadius: 7, style: .continuous))
      .contentShape(Rectangle())
    }
    .buttonStyle(PressStyle())
    .onHover { hovering = $0 }
    .help(hint.map { "\(title)  \($0)" } ?? title)
    .accessibilityLabel(title)
  }

  private var foreground: Color {
    if !isEnabled { return .mrowTertiary }
    if style == .field { return .mrowSecondary }
    return hovering || isOn ? .mrowText : .mrowSecondary
  }

  private var background: Color {
    switch style {
    case .field: hovering ? .mrowSelected : .mrowHover
    case .icon, .text: isEnabled && (hovering || isOn) ? .mrowHover : .clear
    }
  }
}

/// A small icon button that copies text, showing a checkmark for a moment so you know it did.
struct CopyButton: View {
  var title = "Copy"
  var symbolSize: CGFloat = 11
  var text: () -> String

  @State private var copied = false
  @State private var hovering = false
  @Environment(\.isEnabled) private var isEnabled

  var body: some View {
    Button {
      NSPasteboard.general.clearContents()
      NSPasteboard.general.setString(text(), forType: .string)
      copied = true
      Task {
        try? await Task.sleep(for: .seconds(1.5))
        copied = false
      }
    } label: {
      Image(systemName: copied ? "checkmark" : "doc.on.doc")
        .font(.system(size: symbolSize, weight: .medium))
        .foregroundStyle(copied ? Color.mrowGreen : hovering ? Color.mrowText : Color.mrowTertiary)
        .frame(width: 22, height: 22)
        .background(
          isEnabled && hovering ? Color.mrowHover : .clear, in: RoundedRectangle(cornerRadius: 6, style: .continuous)
        )
        .contentShape(Rectangle())
    }
    .buttonStyle(.plain)
    .onHover { hovering = $0 }
    .help(copied ? "Copied" : title)
    .accessibilityLabel(title)
  }
}

/// Darkens a button a little while it's held down.
private struct PressStyle: ButtonStyle {
  func makeBody(configuration: Configuration) -> some View {
    configuration.label
      .overlay {
        if configuration.isPressed {
          RoundedRectangle(cornerRadius: 7, style: .continuous).fill(Color.mrowHover)
        }
      }
  }
}

/// A filled action button in the card, e.g. Approve.
struct ActionButton: View {
  var symbol: String?
  var title: String
  var prominent = false
  var action: () -> Void

  @State private var hovering = false
  @Environment(\.isEnabled) private var isEnabled

  var body: some View {
    Button(action: action) {
      HStack(spacing: 6) {
        if let symbol { Image(systemName: symbol).font(.system(size: 12, weight: .semibold)) }
        Text(title).font(.mrowUI(12.5, .semibold))
      }
      .padding(.horizontal, 12)
      .frame(height: 30)
      .foregroundStyle(prominent ? Color.mrowPaper : Color.mrowText)
      .background(fill, in: RoundedRectangle(cornerRadius: 8, style: .continuous))
      .overlay {
        if !prominent {
          RoundedRectangle(cornerRadius: 8, style: .continuous).strokeBorder(Color.mrowLine)
        }
      }
      .opacity(isEnabled ? 1 : 0.5)
      .contentShape(Rectangle())
    }
    .buttonStyle(.plain)
    .onHover { hovering = $0 }
  }

  private var fill: Color {
    if prominent { return hovering ? Color.mrowAccent.opacity(0.88) : .mrowAccent }
    return hovering ? .mrowHover : .mrowElevated
  }
}

/// A small rounded tag: branches, labels, states.
struct Chip: View {
  var text: String
  var symbol: String?
  var tint: Color = .mrowSecondary
  var fill: Color = .mrowHover
  var mono = false

  var body: some View {
    HStack(spacing: 5) {
      if let symbol { Image(systemName: symbol).font(.system(size: 10, weight: .semibold)) }
      Text(text).font(mono ? .mrowMono(11) : .mrowUI(11.5, .medium)).lineLimit(1)
    }
    .foregroundStyle(tint)
    .padding(.horizontal, 8)
    .frame(height: 22)
    .background(fill, in: RoundedRectangle(cornerRadius: 6, style: .continuous))
  }
}

/// A person's avatar from GitLab, or their initials while it loads.
struct Avatar: View {
  var person: Person
  var size: CGFloat = 20

  var body: some View {
    ZStack {
      Circle().fill(Color.mrowAccentSoft)
      Text(person.initials)
        .font(.mrowUI(size * 0.42, .semibold))
        .foregroundStyle(Color.mrowAccent)
      if let url = avatarURL {
        AsyncImage(url: url) { image in
          image.resizable().scaledToFill()
        } placeholder: {
          Color.clear
        }
        .clipShape(Circle())
      }
    }
    .frame(width: size, height: size)
    .overlay(Circle().strokeBorder(Color.mrowLine))
    .help(person.name)
  }

  private var avatarURL: URL? {
    guard let raw = person.avatarUrl, !raw.isEmpty else { return nil }
    if raw.hasPrefix("http") { return URL(string: raw) }
    let host = Preferences.shared.hostname.trimmingCharacters(in: .whitespaces)
    return URL(string: "https://\(host.isEmpty ? "gitlab.com" : host)\(raw)")
  }
}

/// Makes the window this sits in key once it's on screen. A popover opened from the keyboard
/// otherwise leaves its arrow keys and typing with the window underneath.
struct TakesKey: NSViewRepresentable {
  final class KeyView: NSView {
    override func viewDidMoveToWindow() {
      super.viewDidMoveToWindow()
      guard let window else { return }
      DispatchQueue.main.async { if !window.isKeyWindow { window.makeKey() } }
    }
  }

  func makeNSView(context: Context) -> NSView { KeyView() }
  func updateNSView(_ view: NSView, context: Context) {}
}

/// Lets the header drag the window and zoom it on double-click, like a title bar.
struct WindowDragArea: NSViewRepresentable {
  final class DragView: NSView {
    override var mouseDownCanMoveWindow: Bool { true }

    override func mouseDown(with event: NSEvent) {
      guard let window else { return }
      if event.clickCount == 2 {
        // Double-clicking a title bar does what the user chose in Desktop & Dock settings.
        switch UserDefaults.standard.string(forKey: "AppleActionOnDoubleClick") {
        case "Minimize": window.performMiniaturize(nil)
        case "None": break
        default: window.performZoom(nil)
        }
        return
      }
      window.performDrag(with: event)
    }
  }

  func makeNSView(context: Context) -> DragView { DragView() }
  func updateNSView(_ nsView: DragView, context: Context) {}
}

extension View {
  /// macOS 26 extends scroll views up under the title bar and lays a glass "scroll edge" view
  /// over the header, which swallows clicks on the header's buttons. Mrow's header is its own
  /// flat strip, as in Mdow, so the effect goes.
  @ViewBuilder func withoutEdgeEffect() -> some View {
    if #available(macOS 26, *) {
      scrollEdgeEffectHidden(true, for: .all)
    } else {
      self
    }
  }
}

enum RelativeTime {
  /// "4m", "3h", "2d", "Oct 3": short enough for a sidebar row.
  static func short(_ date: Date, now: Date = Date()) -> String {
    let seconds = max(0, now.timeIntervalSince(date))
    if seconds < 60 { return "now" }
    if seconds < 3600 { return "\(Int(seconds / 60))m" }
    if seconds < 86_400 { return "\(Int(seconds / 3600))h" }
    if seconds < 86_400 * 14 { return "\(Int(seconds / 86_400))d" }
    return date.formatted(.dateTime.month(.abbreviated).day())
  }

  static func long(_ date: Date) -> String {
    let formatter = RelativeDateTimeFormatter()
    formatter.unitsStyle = .full
    return formatter.localizedString(for: date, relativeTo: Date())
  }
}

extension Color {
  /// GitLab's label colours come as "#RRGGBB".
  init?(hex: String) {
    var string = hex.trimmingCharacters(in: .whitespaces)
    if string.hasPrefix("#") { string.removeFirst() }
    guard string.count == 6, let value = UInt32(string, radix: 16) else { return nil }
    self.init(nsColor: Palette.rgb(value))
  }
}

/// A vertical stack that asks each child for its height once per width, with no limit on the
/// height, so text wraps to as many lines as it needs. VStack plus `.fixedSize` gets the same
/// look but measures every child several times, and that multiplies at each level of nesting.
struct Column: Layout {
  var spacing: CGFloat = 0

  struct Cache {
    var width: CGFloat?
    var sizes: [CGSize] = []
  }

  func makeCache(subviews: Subviews) -> Cache { Cache() }

  func updateCache(_ cache: inout Cache, subviews: Subviews) { cache = Cache() }

  func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout Cache) -> CGSize {
    let sizes = measure(width: proposal.width, subviews: subviews, cache: &cache)
    let height = sizes.reduce(0) { $0 + $1.height } + spacing * CGFloat(max(0, sizes.count - 1))
    return CGSize(width: proposal.width ?? sizes.map(\.width).max() ?? 0, height: height)
  }

  func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout Cache) {
    let sizes = measure(width: bounds.width, subviews: subviews, cache: &cache)
    var y = bounds.minY
    for (index, subview) in subviews.enumerated() {
      subview.place(
        at: CGPoint(x: bounds.minX, y: y), proposal: ProposedViewSize(width: bounds.width, height: sizes[index].height))
      y += sizes[index].height + spacing
    }
  }

  private func measure(width: CGFloat?, subviews: Subviews, cache: inout Cache) -> [CGSize] {
    if cache.width == width, cache.sizes.count == subviews.count { return cache.sizes }
    cache.width = width
    cache.sizes = subviews.map { $0.sizeThatFits(ProposedViewSize(width: width, height: nil)) }
    return cache.sizes
  }
}

/// A thin rounded bar, filled to `fraction`.
struct ProgressBar: View {
  var fraction: Double

  var body: some View {
    GeometryReader { geometry in
      Capsule()
        .fill(Color.mrowLine)
        .overlay(alignment: .leading) {
          Capsule()
            .fill(fraction >= 1 ? Color.mrowGreen : Color.mrowAccent)
            .frame(width: geometry.size.width * min(max(fraction, 0), 1))
        }
    }
    .animation(.easeOut(duration: 0.2), value: fraction)
  }
}
