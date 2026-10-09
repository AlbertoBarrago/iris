import Foundation
import Observation

/// What the window shows, read from the Rust core off the main thread.
@MainActor
@Observable
final class MailModel {
    private(set) var accounts: [AccountRow] = []
    private(set) var threads: [ThreadRow] = []
    /// The open conversation as one page, drawn by the Rust core the way
    /// the GTK app draws it.
    private(set) var page: String?
    private(set) var problem: String?

    var selectedAccount: Int64? {
        didSet { if selectedAccount != oldValue { loadInbox() } }
    }
    var selectedThread: String? {
        didSet { if selectedThread != oldValue { loadConversation() } }
    }

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
                let store = try Store.open(path: MailModel.storePath)
                let accounts = try store.accounts()
                // `IRIS_OPEN=<account id>:<thread id>` opens one conversation
                // at start, for comparing pages with the GTK app's demo.
                let asked = ProcessInfo.processInfo.environment["IRIS_OPEN"]?
                    .split(separator: ":", maxSplits: 1).map(String.init)
                await MainActor.run {
                    self.store = store
                    self.accounts = accounts
                    if let asked, asked.count == 2, let account = Int64(asked[0]) {
                        self.selectedAccount = account
                        self.selectedThread = asked[1]
                    } else {
                        self.selectedAccount = accounts.first?.id
                    }
                }
            } catch {
                await MainActor.run { self.problem = "\(error)" }
            }
        }
    }

    private func loadInbox() {
        guard let store, let account = selectedAccount else { return }
        threads = []
        selectedThread = nil
        Task.detached {
            do {
                let threads = try store.inbox(accountId: account, offset: 0, limit: 200)
                await MainActor.run {
                    // The person may have picked another account meanwhile.
                    if self.selectedAccount == account { self.threads = threads }
                }
            } catch {
                await MainActor.run { self.problem = "\(error)" }
            }
        }
    }

    /// The colors pages are drawn in, which the window sets from its
    /// appearance and the system accent.
    var theme = PageTheme(dark: false, accent: "#007aff", accentText: "#0060df") {
        didSet { if theme != oldValue { loadConversation() } }
    }

    private func loadConversation() {
        guard let store, let account = selectedAccount, let thread = selectedThread else {
            page = nil
            return
        }
        let theme = theme
        Task.detached {
            do {
                let page = try store.conversationPage(accountId: account, threadId: thread, theme: theme)
                await MainActor.run {
                    if self.selectedThread == thread { self.page = page }
                }
            } catch {
                await MainActor.run { self.problem = "\(error)" }
            }
        }
    }
}
