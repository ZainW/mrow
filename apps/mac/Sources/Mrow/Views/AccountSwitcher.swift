import SwiftUI

/// The sidebar's account switcher, like a team picker: the account on show with a chevron,
/// opening a list of every account with who I am there and what's waiting. ⌃1 and ⌃2 switch
/// straight away.
struct AccountSwitcher: View {
  @EnvironmentObject private var store: ReviewStore
  var showSettings: () -> Void

  @State private var open = false
  @State private var hovering = false

  var body: some View {
    Button {
      open.toggle()
    } label: {
      HStack(spacing: 8) {
        ForgeMark(forge: store.forge, size: 20)
        VStack(alignment: .leading, spacing: 0) {
          Text(store.forge.name)
            .font(.mrowUI(13, .semibold))
            .foregroundStyle(Color.mrowText)
          if let subtitle {
            Text(subtitle)
              .font(.mrowUI(11))
              .foregroundStyle(store.error != nil ? Color.mrowRed : Color.mrowTertiary)
              .lineLimit(1)
          }
        }
        Image(systemName: "chevron.up.chevron.down")
          .font(.system(size: 9, weight: .semibold))
          .foregroundStyle(Color.mrowTertiary)
        Spacer(minLength: 0)
        // The other accounts' waiting reviews, so nothing hides behind the switch.
        if otherWaiting > 0 {
          Text("\(otherWaiting)")
            .font(.mrowUI(10.5, .semibold))
            .foregroundStyle(Color.mrowAccent)
            .padding(.horizontal, 6)
            .frame(height: 17)
            .background(Color.mrowAccentSoft, in: Capsule())
            .help("\(otherWaiting) waiting in your other account")
        }
      }
      .padding(.horizontal, 8)
      .frame(height: 38)
      .background(
        hovering || open ? Color.mrowHover : .clear, in: RoundedRectangle(cornerRadius: 8, style: .continuous)
      )
      .contentShape(Rectangle())
    }
    .buttonStyle(.plain)
    .onHover { hovering = $0 }
    .help("Switch account  ⌃1 ⌃2")
    .accessibilityLabel("Account: \(store.forge.name)")
    .popover(isPresented: $open, arrowEdge: .bottom) {
      AccountList(showSettings: {
        open = false
        showSettings()
      }) { open = false }
      .environmentObject(store)
    }
  }

  private var subtitle: String? {
    if store.error != nil { return "Offline" }
    guard let me = store.me else { return nil }
    let host = store.forge.cli.host
    return "@\(me.username)" + (host.isEmpty ? "" : " · \(host)")
  }

  private var otherWaiting: Int {
    store.forges.filter { $0 != store.forge }.reduce(0) { $0 + store.waitingCount($1) }
  }
}

/// The switcher's popover: one row per account, then a way to add or change them.
private struct AccountList: View {
  @EnvironmentObject private var store: ReviewStore
  var showSettings: () -> Void
  var done: () -> Void

  var body: some View {
    VStack(alignment: .leading, spacing: 2) {
      Text("ACCOUNTS")
        .font(.mrowUI(10.5, .semibold))
        .tracking(0.6)
        .foregroundStyle(Color.mrowTertiary)
        .padding(.horizontal, 10)
        .padding(.top, 4)
        .padding(.bottom, 4)
      ForEach(Array(store.forges.enumerated()), id: \.element) { index, forge in
        AccountRow(forge: forge, shortcut: "⌃\(index + 1)", isOn: forge == store.forge) {
          store.switchForge(forge)
          done()
        }
      }
      Rectangle().fill(Color.mrowLine).frame(height: 1).padding(.vertical, 4)
      ChromeButton(
        symbol: "slider.horizontal.3",
        title: Forge.allCases.allSatisfy(store.forges.contains) ? "Account Settings…" : "Add an Account…",
        style: .text, symbolSize: 12
      ) {
        showSettings()
      }
    }
    .padding(6)
    .frame(width: 280)
    .background(Color.mrowElevated)
    .background(TakesKey())
  }
}

private struct AccountRow: View {
  @EnvironmentObject private var store: ReviewStore
  var forge: Forge
  var shortcut: String
  var isOn: Bool
  var action: () -> Void

  @State private var hovering = false

  var body: some View {
    Button(action: action) {
      HStack(spacing: 10) {
        ForgeMark(forge: forge, size: 26)
        VStack(alignment: .leading, spacing: 1) {
          Text(forge.name).font(.mrowUI(13, .medium)).foregroundStyle(Color.mrowText)
          Text(detail)
            .font(.mrowUI(11))
            .foregroundStyle(account?.error != nil ? Color.mrowRed : Color.mrowTertiary)
            .lineLimit(1)
            .truncationMode(.tail)
        }
        Spacer(minLength: 6)
        let waiting = store.waitingCount(forge)
        if waiting > 0 {
          Text("\(waiting)")
            .font(.mrowUI(11, .semibold))
            .foregroundStyle(Color.mrowAccent)
            .padding(.horizontal, 6)
            .frame(height: 18)
            .background(Color.mrowAccentSoft, in: Capsule())
        }
        Text(shortcut).font(.mrowMono(10.5)).foregroundStyle(Color.mrowTertiary)
        Image(systemName: "checkmark")
          .font(.system(size: 10, weight: .bold))
          .foregroundStyle(Color.mrowAccent)
          .opacity(isOn ? 1 : 0)
      }
      .padding(.horizontal, 8)
      .frame(height: 44)
      .background(
        hovering ? Color.mrowHover : .clear, in: RoundedRectangle(cornerRadius: 7, style: .continuous)
      )
      .contentShape(Rectangle())
    }
    .buttonStyle(.plain)
    .onHover { hovering = $0 }
    .help(account?.error ?? "")
  }

  private var account: Account? { store.accounts[forge] }

  private var detail: String {
    if let error = account?.error { return error }
    guard let me = account?.me else { return "Signing in through \(forge.cli.name)…" }
    let host = forge.cli.host
    return "@\(me.username) · \(host.isEmpty ? forge.defaultHost : host)"
  }
}

/// A forge's small square mark: GitLab's orange, GitHub's ink, with a monogram.
struct ForgeMark: View {
  var forge: Forge
  var size: CGFloat = 20

  var body: some View {
    RoundedRectangle(cornerRadius: size * 0.28, style: .continuous)
      .fill(fill)
      .frame(width: size, height: size)
      .overlay {
        Text(monogram)
          .font(.mrowUI(size * 0.42, .bold))
          .foregroundStyle(ink)
          .tracking(-0.2)
      }
      .accessibilityHidden(true)
  }

  private var monogram: String {
    switch forge {
    case .gitlab: "GL"
    case .github: "GH"
    }
  }

  private var fill: Color {
    switch forge {
    case .gitlab: Color(red: 0.886, green: 0.263, blue: 0.161)
    case .github: .mrowText
    }
  }

  private var ink: Color {
    switch forge {
    case .gitlab: .white
    case .github: .mrowPaper
    }
  }
}
