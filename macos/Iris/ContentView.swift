import SwiftUI

/// The mail window: the sidebar, the list of the mailbox picked, and the
/// open conversation, laid out as the GTK app lays them out.
struct ContentView: View {
    @Environment(MailModel.self) private var model
    @Environment(\.undoManager) private var undoManager

    var body: some View {
        NavigationSplitView {
            SidebarView()
                .navigationSplitViewColumnWidth(min: 220, ideal: 260, max: 340)
        } content: {
            ThreadListView()
                .navigationSplitViewColumnWidth(min: 340, ideal: 440, max: 560)
        } detail: {
            ConversationView()
        }
        .onAppear { model.undoManager = undoManager }
        .onChange(of: undoManager) { model.undoManager = undoManager }
        .alert("Iris could not read the mail", isPresented: .constant(model.problem != nil)) {
            Button("OK") {}
        } message: {
            Text(model.problem ?? "")
        }
    }
}
