import SwiftUI

/// Iris's SwiftUI front end. This first slice reads the mail the GTK app
/// syncs: accounts, an inbox, and one conversation at a time.
struct IrisApp: App {
    @State private var model = MailModel()
    @State private var assistant = AssistantModel()

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
        }
        .defaultSize(width: 1100, height: 720)
        .commands {
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
                ComposeView(request: request).environment(model)
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
