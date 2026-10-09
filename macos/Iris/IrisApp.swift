import SwiftUI

/// Iris's SwiftUI front end. This first slice reads the mail the GTK app
/// syncs: accounts, an inbox, and one conversation at a time.
@main
struct IrisApp: App {
    @State private var model = MailModel()

    init() {
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
    }
}
