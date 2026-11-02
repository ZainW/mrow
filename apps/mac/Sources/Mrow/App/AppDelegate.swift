import AppKit
import SwiftUI
import UserNotifications

@MainActor final class AppDelegate: NSObject, NSApplicationDelegate, NSWindowDelegate,
  UNUserNotificationCenterDelegate
{
  private let store = ReviewStore()
  private let windowState = WindowState()
  private var window: NSWindow!

  func applicationDidFinishLaunching(_ notification: Notification) {
    Theme.registerFonts()
    NSApp.appearance = Preferences.shared.appearance.appearance
    NSApp.mainMenu = makeMainMenu()
    makeWindow()
    if Bundle.main.bundleIdentifier != nil {
      let center = UNUserNotificationCenter.current()
      center.delegate = self
      center.requestAuthorization(options: [.alert, .sound]) { _, _ in }
    }
    let arguments = CommandLine.arguments
    if arguments.contains("--selftest") {
      SelfTest.run()
    } else if let index = arguments.firstIndex(of: "--bench-record"), index + 1 < arguments.count {
      Task { await Bench.record(to: arguments[index + 1]) }
    } else if let index = arguments.firstIndex(of: "--bench"), index + 1 < arguments.count {
      store.offline = true
      ImageLoader.offline = true
      Task { await Bench.run(fixtures: arguments[index + 1], label: arguments.dropFirst(index + 2).first ?? "run") }
    } else if let index = arguments.firstIndex(of: "--ask"), index + 1 < arguments.count {
      Task {
        await Snapshot.ask(
          store: store, to: arguments[index + 1], project: arguments.dropFirst(index + 2).first,
          question: arguments.dropFirst(index + 3).first ?? "What could break, and where should I look first?")
      }
    } else if let index = arguments.firstIndex(of: "--queue"), index + 1 < arguments.count {
      Task { await QueueDump.run(arguments[index + 1]) }
    } else if let index = arguments.firstIndex(of: "--snapshot"), index + 1 < arguments.count {
      Task { await Snapshot.run(window: window, store: store, state: windowState, to: CommandLine.arguments[index + 1]) }
    } else {
      window.makeKeyAndOrderFront(nil)
      NSApp.activate()
    }
  }

  func applicationShouldHandleReopen(_ sender: NSApplication, hasVisibleWindows flag: Bool) -> Bool {
    if !flag { window.makeKeyAndOrderFront(nil) }
    return true
  }

  // Closing the window keeps the app (and its badge) around, like Mail.
  func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool { false }

  func applicationWillTerminate(_ notification: Notification) { ACPClient.shared.stop() }

  private func makeWindow() {
    // Configured like Mdow's document window: no title, transparent title bar, and an empty
    // toolbar so the window buttons sit centred in the 40pt header.
    window = NSWindow(
      contentRect: NSRect(x: 0, y: 0, width: 1240, height: 820),
      styleMask: [.titled, .closable, .miniaturizable, .resizable, .fullSizeContentView],
      backing: .buffered, defer: false)
    window.minSize = NSSize(width: 760, height: 460)
    window.tabbingMode = .disallowed
    window.titlebarAppearsTransparent = true
    window.titleVisibility = .hidden
    window.titlebarSeparatorStyle = .none
    window.backgroundColor = Palette.chrome
    window.toolbar = NSToolbar(identifier: "MrowHeader")
    window.toolbarStyle = .unifiedCompact
    window.title = "Mrow"
    window.isReleasedWhenClosed = false
    window.delegate = self
    let host = NSHostingController(
      rootView: RootView()
        .environmentObject(store)
        .environmentObject(Preferences.shared)
        .environmentObject(windowState))
    // The header is drawn by RootView, so there's no title bar area to keep clear. Without
    // this, macOS 26 slides scroll views up under the toolbar, over the header's buttons.
    host.safeAreaRegions = []
    window.contentViewController = host
    window.setContentSize(NSSize(width: 1240, height: 820))
    window.center()
    window.setFrameAutosaveName("MrowWindow")
  }

  // MARK: Notifications

  nonisolated func userNotificationCenter(
    _ center: UNUserNotificationCenter, willPresent notification: UNNotification
  ) async -> UNNotificationPresentationOptions {
    [.banner, .sound]
  }

  nonisolated func userNotificationCenter(
    _ center: UNUserNotificationCenter, didReceive response: UNNotificationResponse
  ) async {
    let id = response.notification.request.content.userInfo["id"] as? String
    await MainActor.run {
      store.setMode(.toReview)
      if let id { store.selectedID = id }
      window.makeKeyAndOrderFront(nil)
      NSApp.activate()
    }
  }

  // MARK: Menu actions

  @objc func refresh(_ sender: Any?) { Task { await store.refresh() } }
  @objc func showSettings(_ sender: Any?) {
    window.makeKeyAndOrderFront(nil)
    windowState.showingSettings = true
  }
  @objc func toggleSidebar(_ sender: Any?) { withAnimation(Motion.panel) { windowState.sidebarVisible.toggle() } }
  @objc func toggleAsk(_ sender: Any?) {
    guard store.selected != nil else { return NSSound.beep() }
    windowState.focusOrToggleAsk()
  }
  @objc func newChat(_ sender: Any?) {
    guard let item = store.selected else { return NSSound.beep() }
    AskStore.shared.newChat(AskStore.shared.chat(for: item), item: item)
    if !windowState.askVisible { withAnimation(Motion.panel) { windowState.askVisible = true } }
    windowState.askFocusPending = true
  }
  @objc func chooseModel(_ sender: Any?) {
    guard store.selected != nil else { return NSSound.beep() }
    if !windowState.askVisible { withAnimation(Motion.panel) { windowState.askVisible = true } }
    // After the panel is in the window, so the popover has a button to hang from.
    DispatchQueue.main.async { self.windowState.modelPickerOpen = true }
  }
  @objc func focusFilter(_ sender: Any?) { windowState.focusFilter += 1 }
  @objc func showMode(_ sender: NSMenuItem) {
    store.setMode(QueueMode(rawValue: sender.tag) ?? .toReview)
  }
  @objc func showTab(_ sender: NSMenuItem) {
    windowState.tab = DetailTab(rawValue: sender.tag) ?? .overview
  }
  @objc func nextItem(_ sender: Any?) { store.select(offset: 1) }
  @objc func previousItem(_ sender: Any?) { store.select(offset: -1) }
  @objc func showAccount(_ sender: NSMenuItem) {
    let forges = store.forges
    guard forges.indices.contains(sender.tag) else { return NSSound.beep() }
    store.switchForge(forges[sender.tag])
  }
  @objc func openInGitLab(_ sender: Any?) {
    if let item = store.selected { NSWorkspace.shared.open(item.webURL) }
  }
  @objc func copyLink(_ sender: Any?) {
    guard let item = store.selected else { return }
    NSPasteboard.general.clearContents()
    NSPasteboard.general.setString(item.webURL.absoluteString, forType: .string)
    store.show("Copied link")
  }
  @objc func approve(_ sender: Any?) {
    guard let item = store.selected, item.isOpen else { return }
    Task { await store.approve(item) }
  }
  @objc func markAllRead(_ sender: Any?) { store.markAllSeen() }
  @objc func keepInTab(_ sender: Any?) {
    if let item = store.selected { store.openTab(item) }
  }
  /// ⌘W closes the merge request's tab; with nothing open, the window.
  @objc func closeTab(_ sender: Any?) {
    if let id = store.selectedID { store.closeTab(id) } else { window.performClose(nil) }
  }
  @objc func closeOtherTabs(_ sender: Any?) {
    if let id = store.selectedID { store.closeOtherTabs(than: id) }
  }
  @objc func nextTab(_ sender: Any?) { store.selectTab(offset: 1) }
  @objc func previousTab(_ sender: Any?) { store.selectTab(offset: -1) }
  @objc func showMainWindow(_ sender: Any?) { window.makeKeyAndOrderFront(nil) }

  private func makeMainMenu() -> NSMenu {
    let main = NSMenu()

    let app = NSMenu()
    app.addItem(withTitle: "About Mrow", action: #selector(NSApplication.orderFrontStandardAboutPanel(_:)), keyEquivalent: "")
    app.addItem(.separator())
    app.addItem(item("Settings…", #selector(showSettings(_:)), ","))
    app.addItem(.separator())
    app.addItem(withTitle: "Hide Mrow", action: #selector(NSApplication.hide(_:)), keyEquivalent: "h")
    let hideOthers = app.addItem(
      withTitle: "Hide Others", action: #selector(NSApplication.hideOtherApplications(_:)), keyEquivalent: "h")
    hideOthers.keyEquivalentModifierMask = [.command, .option]
    app.addItem(withTitle: "Show All", action: #selector(NSApplication.unhideAllApplications(_:)), keyEquivalent: "")
    app.addItem(.separator())
    app.addItem(withTitle: "Quit Mrow", action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q")
    main.addItem(submenu(app, title: "Mrow"))

    // Text fields need these for ⌘C, ⌘V and friends to work.
    let edit = NSMenu(title: "Edit")
    edit.addItem(withTitle: "Undo", action: Selector(("undo:")), keyEquivalent: "z")
    edit.addItem(withTitle: "Redo", action: Selector(("redo:")), keyEquivalent: "Z")
    edit.addItem(.separator())
    edit.addItem(withTitle: "Cut", action: #selector(NSText.cut(_:)), keyEquivalent: "x")
    edit.addItem(withTitle: "Copy", action: #selector(NSText.copy(_:)), keyEquivalent: "c")
    edit.addItem(withTitle: "Paste", action: #selector(NSText.paste(_:)), keyEquivalent: "v")
    edit.addItem(withTitle: "Select All", action: #selector(NSText.selectAll(_:)), keyEquivalent: "a")
    edit.addItem(.separator())
    edit.addItem(item("Filter…", #selector(focusFilter(_:)), "k"))
    edit.addItem(item("Find…", #selector(focusFilter(_:)), "f"))
    main.addItem(submenu(edit, title: "Edit"))

    let view = NSMenu(title: "View")
    for mode in QueueMode.allCases {
      let entry = item(mode.title, #selector(showMode(_:)), "\(mode.rawValue + 1)")
      entry.tag = mode.rawValue
      view.addItem(entry)
    }
    view.addItem(.separator())
    // The account switcher's accounts, in its order: ⌃1, ⌃2.
    for index in Forge.allCases.indices {
      let entry = item("Account \(index + 1)", #selector(showAccount(_:)), "\(index + 1)", .control)
      entry.tag = index
      view.addItem(entry)
    }
    view.addItem(.separator())
    for tab in DetailTab.allCases {
      let entry = item(tab.title, #selector(showTab(_:)), "\(tab.rawValue + 1)", [.command, .option])
      entry.tag = tab.rawValue
      view.addItem(entry)
    }
    view.addItem(.separator())
    view.addItem(item("Toggle Sidebar", #selector(toggleSidebar(_:)), "s", [.command, .control]))
    view.addItem(item("Ask Cursor", #selector(toggleAsk(_:)), "l"))
    view.addItem(item("New Chat", #selector(newChat(_:)), "l", [.command, .shift]))
    view.addItem(item("Choose Model…", #selector(chooseModel(_:)), "l", [.command, .option]))
    view.addItem(item("Refresh", #selector(refresh(_:)), "r"))
    main.addItem(submenu(view, title: "View"))

    let review = NSMenu(title: "Review")
    review.addItem(item("Next Review", #selector(nextItem(_:)), "]", [.command, .option]))
    review.addItem(item("Previous Review", #selector(previousItem(_:)), "[", [.command, .option]))
    review.addItem(.separator())
    review.addItem(item("Open in Browser", #selector(openInGitLab(_:)), "o"))
    review.addItem(item("Copy Link", #selector(copyLink(_:)), "c", [.command, .shift]))
    review.addItem(item("Approve", #selector(approve(_:)), "a", [.command, .shift]))
    review.addItem(.separator())
    review.addItem(item("Mark All as Read", #selector(markAllRead(_:)), ""))
    main.addItem(submenu(review, title: "Review"))

    let windowMenu = NSMenu(title: "Window")
    windowMenu.addItem(withTitle: "Minimize", action: #selector(NSWindow.performMiniaturize(_:)), keyEquivalent: "m")
    windowMenu.addItem(withTitle: "Zoom", action: #selector(NSWindow.performZoom(_:)), keyEquivalent: "")
    let closeWindow = windowMenu.addItem(
      withTitle: "Close Window", action: #selector(NSWindow.performClose(_:)), keyEquivalent: "W")
    closeWindow.keyEquivalentModifierMask = [.command, .shift]
    windowMenu.addItem(.separator())
    windowMenu.addItem(item("Keep Open in Tab", #selector(keepInTab(_:)), "t"))
    windowMenu.addItem(item("Close Tab", #selector(closeTab(_:)), "w"))
    windowMenu.addItem(item("Close Other Tabs", #selector(closeOtherTabs(_:)), "w", [.command, .option]))
    windowMenu.addItem(item("Show Next Tab", #selector(nextTab(_:)), "]", [.command, .shift]))
    windowMenu.addItem(item("Show Previous Tab", #selector(previousTab(_:)), "[", [.command, .shift]))
    // ⌃Tab and ⌃⇧Tab too, as in every browser.
    for (key, action, modifiers) in [
      ("\t", #selector(nextTab(_:)), NSEvent.ModifierFlags.control),
      ("\t", #selector(previousTab(_:)), [.control, .shift]),
      ("\u{19}", #selector(previousTab(_:)), [.control, .shift]),  // shift-tab arrives as backtab
    ] {
      let hidden = item("", action, key, modifiers)
      hidden.isHidden = true
      hidden.allowsKeyEquivalentWhenHidden = true
      windowMenu.addItem(hidden)
    }
    windowMenu.addItem(.separator())
    windowMenu.addItem(item("Mrow", #selector(showMainWindow(_:)), "0"))
    main.addItem(submenu(windowMenu, title: "Window"))
    NSApp.windowsMenu = windowMenu

    return main
  }

  private func item(
    _ title: String, _ action: Selector, _ key: String, _ modifiers: NSEvent.ModifierFlags = .command
  ) -> NSMenuItem {
    let item = NSMenuItem(title: title, action: action, keyEquivalent: key)
    item.keyEquivalentModifierMask = modifiers
    item.target = self
    return item
  }

  private func submenu(_ menu: NSMenu, title: String) -> NSMenuItem {
    let item = NSMenuItem(title: title, action: nil, keyEquivalent: "")
    menu.title = title
    item.submenu = menu
    return item
  }
}
