import SwiftUI

/// Settings, as a sheet over the window in the palette's elevated colour.
struct SettingsView: View {
  @EnvironmentObject private var store: ReviewStore
  @EnvironmentObject private var prefs: Preferences
  @Environment(\.dismiss) private var dismiss

  var body: some View {
    VStack(alignment: .leading, spacing: 0) {
      HStack {
        Text("Settings").font(.mrowSerif(22, .semibold)).foregroundStyle(Color.mrowText)
        Spacer()
        ChromeButton(symbol: "xmark", title: "Close", symbolSize: 12) { dismiss() }
          .keyboardShortcut(.cancelAction)
      }
      .padding(.bottom, 18)

      account(.gitlab, enabled: $prefs.gitlabEnabled, host: $prefs.hostname, path: $prefs.glabPath)
      account(.github, enabled: $prefs.githubEnabled, host: $prefs.githubHost, path: $prefs.ghPath)

      section("Queue") {
        row("Refresh every") {
          Picker("", selection: $prefs.refreshMinutes) {
            Text("Never").tag(0)
            Text("Minute").tag(1)
            Text("5 minutes").tag(5)
            Text("15 minutes").tag(15)
            Text("30 minutes").tag(30)
          }
          .labelsHidden()
          .fixedSize()
        }
        row("Done looks back") {
          Stepper("\(prefs.doneDays) days", value: $prefs.doneDays, in: 1...90)
            .font(.mrowUI(12.5))
        }
        row("Sort") {
          Picker("", selection: $prefs.oldestFirst) {
            Text("Newest request first").tag(false)
            Text("Waiting longest first").tag(true)
          }
          .labelsHidden()
          .fixedSize()
        }
        row("") { Toggle("Show drafts in To review", isOn: $prefs.showDrafts) }
        row("") { Toggle("Notify me about new review requests", isOn: $prefs.notify) }
      }

      section("Appearance") {
        row("Theme") {
          Picker("", selection: $prefs.appearance) {
            ForEach(AppearanceChoice.allCases) { Text($0.displayName).tag($0) }
          }
          .pickerStyle(.segmented)
          .labelsHidden()
          .fixedSize()
        }
      }
    }
    .font(.mrowUI(12.5))
    .toggleStyle(.checkbox)
    .padding(24)
    .frame(width: 480)
    .background(Color.mrowElevated)
  }

  /// One forge's account: on or off, who I am there, and where its CLI and host are.
  private func account(
    _ forge: Forge, enabled: Binding<Bool>, host: Binding<String>, path: Binding<String>
  ) -> some View {
    section(forge.name) {
      row("") {
        Toggle("Show \(forge.name) reviews", isOn: enabled)
      }
      if enabled.wrappedValue {
        row("Signed in as") {
          Text(signedIn(forge))
            .font(.mrowUI(12.5))
            .foregroundStyle(store.accounts[forge]?.error != nil ? Color.mrowRed : Color.mrowSecondary)
            .lineLimit(2)
            .fixedSize(horizontal: false, vertical: true)
        }
        row("Host") { field(forge.defaultHost, text: host) }
        row("\(forge.cli.name) path") { field(detected(forge), text: path) }
      }
    }
  }

  private func signedIn(_ forge: Forge) -> String {
    let account = store.accounts[forge]
    if let me = account?.me { return "\(me.name) (@\(me.username))" }
    if let error = account?.error { return error }
    return "Not yet. Run `\(forge.cli.loginCommand)` in a terminal."
  }

  private func detected(_ forge: Forge) -> String {
    (try? forge.cli.executable()) ?? "/opt/homebrew/bin/\(forge.cli.name)"
  }

  private func section(_ title: String, @ViewBuilder content: () -> some View) -> some View {
    VStack(alignment: .leading, spacing: 10) {
      Text(title.uppercased())
        .font(.mrowUI(10.5, .semibold))
        .tracking(0.6)
        .foregroundStyle(Color.mrowTertiary)
      content()
    }
    .padding(.vertical, 14)
    .overlay(alignment: .top) { Rectangle().fill(Color.mrowLine).frame(height: 1) }
  }

  private func row(_ label: String, @ViewBuilder content: () -> some View) -> some View {
    HStack(alignment: .firstTextBaseline, spacing: 12) {
      Text(label)
        .foregroundStyle(Color.mrowSecondary)
        .frame(width: 120, alignment: .trailing)
      content()
      Spacer(minLength: 0)
    }
  }

  private func field(_ placeholder: String, text: Binding<String>) -> some View {
    TextField("", text: text, prompt: Text(placeholder).foregroundStyle(Color.mrowTertiary))
      .textFieldStyle(.plain)
      .font(.mrowMono(12))
      .foregroundStyle(Color.mrowText)
      .padding(.horizontal, 9)
      .frame(height: 28)
      .background(Color.mrowHover, in: RoundedRectangle(cornerRadius: 7, style: .continuous))
  }
}
