import SwiftUI

/// Iris's SwiftUI front end. This first slice reads the mail the GTK app
/// syncs: accounts, an inbox, and one conversation at a time.
struct IrisApp: App {
    @State private var model = MailModel()
    @State private var assistant = AssistantModel()
    @Environment(\.openWindow) private var openWindow
    @Environment(\.openURL) private var openURL

    init() {
        startLogging()
        // The words come from the same catalogs the GTK app reads, in the
        // language picked in Settings, or else the one macOS puts first.
        let locale = Bundle.main.resourceURL?.appending(path: "locale").path(percentEncoded: false) ?? ""
        let chosen = chosenLanguage()
        let language = chosen.isEmpty
            ? Locale.preferredLanguages.first?.replacingOccurrences(of: "-", with: "_") ?? ""
            : chosen
        bindLanguage(localeDir: locale, language: language)
    }

    var body: some Scene {
        WindowGroup("Iris") {
            ContentView()
                .environment(model)
                .environment(assistant)
                .task {
                    assistant.mail = model
                    model.open()
                }
                // A `mailto:` link clicked on a website or in another app.
                .onOpenURL { url in
                    if url.scheme?.lowercased() == "mailto" { model.compose(mailto: url) }
                }
                // The window already open takes the link, rather than
                // SwiftUI opening a second main window for each one.
                .handlesExternalEvents(preferring: ["mailto"], allowing: ["*"])
        }
        .defaultSize(width: 1100, height: 720)
        .commands {
            CommandGroup(replacing: .appInfo) {
                Button(aboutWords().title) { openWindow(id: "about") }
                // Always listed, so it is where a Mac app keeps it; a build
                // without the signing key, such as one made in Xcode, has
                // nothing to check against and leaves it off.
                Button(tr("Check for Updates…")) { Updates.shared.checkNow() }
                    .disabled(!Updates.shared.available)
            }
            CommandGroup(replacing: .help) {
                Button(tr("Keyboard shortcuts")) { openWindow(id: "shortcuts") }
                    .keyboardShortcut("/", modifiers: [.command, .shift])
                Divider()
                Button(aboutWords().reportTitle) {
                    if let url = URL(string: aboutWords().reportUrl) { openURL(url) }
                }
            }
            CommandGroup(replacing: .newItem) {
                Button(tr("New Message")) { model.composeAsked = ComposeRequest(kind: .new) }
                    .keyboardShortcut("n")
                Divider()
                Button(tr("Add Account…")) { model.addAccount = AddAccountRequest() }
            }
            CommandGroup(replacing: .printItem) {
                Button(tr("Print…")) { PagePrinter.print(model.finder.view) }
                    .keyboardShortcut("p")
                    .disabled(model.openThread == nil)
            }
            CommandGroup(after: .textEditing) {
                Button(tr("Search")) { Finding.find(model) }
                    .keyboardShortcut("f")
            }
            CommandGroup(after: .newItem) {
                Button(tr("Open in New Window")) { model.openInWindow() }
                    .keyboardShortcut("o")
                Button(tr("New Smart Mailbox…")) {
                    model.smartEditing = SmartEdit(form: SmartForm(id: "", name: "", account: "", matchAll: true, conditions: []))
                }
                Divider()
                Button(tr("Export…")) { model.exportSelection() }
                    .disabled(model.selection.isEmpty)
            }
            CommandGroup(after: .toolbar) {
                // ⌘L, as Mail's message filter.
                Toggle(tr("Show Only Unread"), isOn: $model.unreadOnly)
                    .keyboardShortcut("l")
                    .disabled(model.listing?.filtersUnread != true)
                Button(tr("View Source")) { model.viewSource() }
                    .keyboardShortcut("u", modifiers: [.command, .option])
                    .disabled(model.openThread == nil)
            }
            CommandGroup(after: .sidebar) {
                // The GTK app's Alt+1 and Alt+2.
                Button(tr("Mail")) { model.space = .mail }
                    .keyboardShortcut("1", modifiers: .option)
                Button(tr("Calendar")) { model.space = .calendar }
                    .keyboardShortcut("2", modifiers: .option)
                Divider()
                Button(tr("Assistant")) { assistant.shown.toggle() }
                    .keyboardShortcut("j")
            }
        }
        WindowGroup(id: "compose", for: ComposeRequest.self) { $request in
            if let request {
                // A window SwiftUI hands a new request starts afresh rather
                // than keep the draft it held.
                ComposeView(request: request).id(request).environment(model)
            }
        }
        .defaultSize(width: 720, height: 620)
        // A conversation in a window of its own, as a double click on its
        // row opens it in the GTK app.
        WindowGroup(id: "conversation", for: ThreadKey.self) { $key in
            if let key {
                ConversationWindow(key: key).environment(model)
            }
        }
        .defaultSize(width: 820, height: 760)
        Window(tr("Keyboard shortcuts"), id: "shortcuts") {
            ShortcutsView()
        }
        .windowResizability(.contentSize)
        Window(aboutWords().title, id: "about") {
            AboutView()
        }
        .windowResizability(.contentSize)
        Settings {
            SettingsView().environment(model).environment(assistant)
        }
        .windowResizability(.contentMinSize)
    }
}

/// `text` in the interface's language, from the GTK app's catalogs.
func tr(_ text: String) -> String {
    translate(text: text)
}

/// `one` or `many` in the interface's language, as `count` asks.
func trPlural(_ one: String, _ many: String, _ count: Int) -> String {
    translatePlural(one: one, many: many, count: UInt32(clamping: count))
}

/// The locale the interface speaks: the language picked in Settings, or
/// the system's.
var appLocale: Locale {
    let chosen = chosenLanguage()
    return chosen.isEmpty ? .current : Locale(identifier: chosen)
}

/// The core's errors as the person reads them: their words alone, not
/// the case they travel in, so a notice says "Another copy of Iris is
/// syncing your mail" rather than `Store("…")`.
extension CoreError: CustomStringConvertible {
    public var description: String {
        switch self {
        case .Store(let words), .NewerStore(let words): words
        }
    }
}
