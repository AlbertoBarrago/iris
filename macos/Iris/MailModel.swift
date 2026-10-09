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
    /// Why this window does not sync, when it does not: another copy of
    /// Iris holds the store.
    private(set) var notSyncing: String?

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
                    self.startSync()
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

    /// Starts the sync engine, and reads the sidebar and the list again a
    /// moment after each burst of news from it.
    private func startSync() {
        guard let mail else { return }
        let listener = Listener { [weak self] in
            Task { @MainActor in self?.heardChange() }
        }
        do {
            try mail.startSync(listener: listener)
            notSyncing = nil
        } catch {
            notSyncing = "\(error)"
        }
    }

    private var refreshing: Task<Void, Never>?

    /// Waits half a second for the rest of a burst, then reads again.
    private func heardChange() {
        refreshing?.cancel()
        refreshing = Task { @MainActor in
            try? await Task.sleep(for: .milliseconds(500))
            guard !Task.isCancelled else { return }
            refresh()
        }
    }

    /// Reads the sidebar and the shown list again.
    func refresh() {
        guard let mail else { return }
        Task.detached {
            if let sidebar = try? mail.sidebar() {
                await MainActor.run {
                    self.sidebar = sidebar
                    self.loadList()
                }
            }
        }
    }

    func checkNow() {
        mail?.checkNow()
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

/// Hears the Rust core's news on its own threads and hands it on.
final class Listener: MailListener, @unchecked Sendable {
    private let heard: @Sendable () -> Void

    init(_ heard: @escaping @Sendable () -> Void) {
        self.heard = heard
    }

    func changed(what: String) {
        heard()
    }
}
