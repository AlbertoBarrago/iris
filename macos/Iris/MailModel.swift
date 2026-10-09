import Foundation
import Observation

/// What the window shows, read from the Rust core off the main thread.
@MainActor
@Observable
final class MailModel {
    private(set) var sidebar: [SidebarItem] = []
    private(set) var listing: MailboxListing?
    /// The open conversation as one page, drawn by the Rust core the way
    /// the GTK app draws it.
    private(set) var page: String?
    private(set) var problem: String?

    var selectedMailbox: String? {
        didSet { if selectedMailbox != oldValue { category = nil; loadList() } }
    }
    var category: String? {
        didSet { if category != oldValue { loadList() } }
    }
    var selectedThread: ThreadKey? {
        didSet { if selectedThread != oldValue { loadConversation() } }
    }

    /// The colors pages are drawn in, which the window sets from its
    /// appearance and the system accent.
    var theme = PageTheme(dark: false, accent: "#007aff", accentText: "#0060df") {
        didSet { if theme != oldValue { loadConversation() } }
    }

    private var mail: Mail?
    private var store: Store?

    /// Where the GTK app keeps the mail store, or the store `IRIS_STORE`
    /// names, such as a demo run's, for comparing the two front ends on
    /// the same sample mail.
    nonisolated static var storePath: String {
        if let named = ProcessInfo.processInfo.environment["IRIS_STORE"], !named.isEmpty {
            return named
        }
        return FileManager.default.homeDirectoryForCurrentUser
            .appending(path: "Library/Application Support/iris/mailrs.db")
            .path(percentEncoded: false)
    }

    func open() {
        Task.detached {
            do {
                let mail = try Mail.open(path: MailModel.storePath)
                let store = try Store.open(path: MailModel.storePath)
                let sidebar = try mail.sidebar()
                // `IRIS_OPEN=<account id>:<thread id>` opens one conversation
                // at start, for comparing pages with the GTK app's demo.
                let asked = ProcessInfo.processInfo.environment["IRIS_OPEN"]?
                    .split(separator: ":", maxSplits: 1).map(String.init)
                await MainActor.run {
                    self.mail = mail
                    self.store = store
                    self.sidebar = sidebar
                    self.selectedMailbox = sidebar.first { $0.kind == "mailbox" }?.key
                    if let asked, asked.count == 2, let account = Int64(asked[0]) {
                        self.selectedThread = ThreadKey(account: account, thread: asked[1])
                    }
                }
            } catch {
                await MainActor.run { self.problem = "\(error)" }
            }
        }
    }

    private func loadList() {
        guard let mail, let key = selectedMailbox else { return }
        let category = category
        Task.detached {
            do {
                let listing = try mail.list(key: key, category: category)
                await MainActor.run {
                    // The person may have picked another mailbox meanwhile.
                    if self.selectedMailbox == key, self.category == category { self.listing = listing }
                }
            } catch {
                await MainActor.run { self.problem = "\(error)" }
            }
        }
    }

    private func loadConversation() {
        guard let store, let key = selectedThread else {
            page = nil
            return
        }
        let theme = theme
        Task.detached {
            do {
                let page = try store.conversationPage(accountId: key.account, threadId: key.thread, theme: theme)
                await MainActor.run {
                    if self.selectedThread == key { self.page = page }
                }
            } catch {
                await MainActor.run { self.problem = "\(error)" }
            }
        }
    }
}

/// A conversation, by the account it belongs to and its id.
struct ThreadKey: Hashable, Sendable {
    let account: Int64
    let thread: String
}
