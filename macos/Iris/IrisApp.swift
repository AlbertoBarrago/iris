import SwiftUI

/// Iris's SwiftUI front end. This first slice reads the mail the GTK app
/// syncs: accounts, an inbox, and one conversation at a time.
@main
struct IrisApp: App {
    @State private var model = MailModel()

    var body: some Scene {
        WindowGroup("Iris") {
            ContentView()
                .environment(model)
                .task { model.open() }
        }
        .defaultSize(width: 1100, height: 720)
    }
}
