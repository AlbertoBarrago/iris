import Foundation
import Observation

/// What the window shows, read from the Rust core off the main thread.
@MainActor
@Observable
final class MailModel {
    private(set) var accounts: [AccountRow] = []
    private(set) var threads: [ThreadRow] = []
    private(set) var messages: [MessageItem] = []
    private(set) var problem: String?

    var selectedAccount: Int64? {
        didSet { if selectedAccount != oldValue { loadInbox() } }
    }
    var selectedThread: String? {
        didSet { if selectedThread != oldValue { loadConversation() } }
    }

    private var store: Store?

    /// Where the GTK app keeps the mail store.
    nonisolated static var storePath: String {
        FileManager.default.homeDirectoryForCurrentUser
            .appending(path: "Library/Application Support/iris/mailrs.db")
            .path(percentEncoded: false)
    }

    func open() {
        Task.detached {
            do {
                let store = try Store.open(path: MailModel.storePath)
                let accounts = try store.accounts()
                await MainActor.run {
                    self.store = store
                    self.accounts = accounts
                    self.selectedAccount = accounts.first?.id
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

    private func loadConversation() {
        guard let store, let account = selectedAccount, let thread = selectedThread else {
            messages = []
            return
        }
        Task.detached {
            do {
                let messages = try store.conversation(accountId: account, threadId: thread)
                await MainActor.run {
                    if self.selectedThread == thread { self.messages = messages }
                }
            } catch {
                await MainActor.run { self.problem = "\(error)" }
            }
        }
    }
}
