import SwiftUI

/// Accounts, the inbox of the one picked, and the open conversation.
struct ContentView: View {
    @Environment(MailModel.self) private var model

    var body: some View {
        @Bindable var model = model
        NavigationSplitView {
            List(model.accounts, id: \.id, selection: $model.selectedAccount) { account in
                Label(account.email, systemImage: "tray")
            }
            .navigationTitle("Iris")
        } content: {
            List(model.threads, id: \.id, selection: $model.selectedThread) { thread in
                ThreadRowView(thread: thread)
            }
            .overlay {
                if model.threads.isEmpty, model.selectedAccount != nil {
                    ContentUnavailableView("No Mail", systemImage: "tray")
                }
            }
        } detail: {
            ConversationView()
        }
        .alert("Iris could not read the mail", isPresented: .constant(model.problem != nil)) {
            Button("OK") {}
        } message: {
            Text(model.problem ?? "")
        }
    }
}

/// One conversation in the list: sender, date, subject and the first words.
struct ThreadRowView: View {
    let thread: ThreadRow

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            HStack {
                if thread.unread {
                    Circle().fill(.tint).frame(width: 8, height: 8)
                }
                Text(thread.from).font(.headline).lineLimit(1)
                Spacer()
                Text(Date(timeIntervalSince1970: Double(thread.date) / 1000), format: .dateTime.day().month().hour().minute())
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            Text(thread.subject).lineLimit(1)
            Text(thread.snippet).font(.caption).foregroundStyle(.secondary).lineLimit(2)
        }
        .padding(.vertical, 2)
    }
}
