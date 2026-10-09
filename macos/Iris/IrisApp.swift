import SwiftUI

/// Iris's SwiftUI front end. This first slice reads the mail the GTK app
/// syncs: accounts, an inbox, and one conversation at a time.
@main
struct IrisApp: App {
    @State private var model = MailModel()

    init() {
        startLogging()
        // The words come from the same catalogs the GTK app reads, in the
        // language macOS puts first.
        let locale = Bundle.main.resourceURL?.appending(path: "locale").path(percentEncoded: false) ?? ""
        let language = Locale.preferredLanguages.first?.replacingOccurrences(of: "-", with: "_") ?? ""
        bindLanguage(localeDir: locale, language: language)
    }

    var body: some Scene {
        WindowGroup("Iris") {
            ContentView()
                .environment(model)
                .task { model.open() }
        }
        .defaultSize(width: 1100, height: 720)
        .commands {
            CommandGroup(replacing: .newItem) {
                Button(tr("New Message")) { model.composeAsked = ComposeRequest(kind: .new) }
                    .keyboardShortcut("n")
            }
        }
        WindowGroup(id: "compose", for: ComposeRequest.self) { $request in
            if let request {
                ComposeView(request: request).environment(model)
            }
        }
        .defaultSize(width: 720, height: 620)
    }
}

/// `text` in the interface's language, from the GTK app's catalogs.
func tr(_ text: String) -> String {
    translate(text: text)
}
