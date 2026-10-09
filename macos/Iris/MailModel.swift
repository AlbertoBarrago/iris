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
    private(set) var page: ConversationPage?
    /// Conversations whose pictures on the web the person chose to load.
    private var remoteAllowed: Set<ThreadKey> = []
    /// Messages of the open conversation the person opened or closed.
    private var toggled: [String] = []
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
    /// The conversations picked in the list; one of them opens.
    var selection: Set<ThreadKey> = [] {
        didSet {
            if openThread != Self.single(oldValue) {
                toggled = []
                loadConversation()
            }
        }
    }

    /// The conversation on screen: the one picked, when only one is.
    var openThread: ThreadKey? { Self.single(selection) }

    private static func single(_ keys: Set<ThreadKey>) -> ThreadKey? {
        keys.count == 1 ? keys.first : nil
    }

    /// The toast over the list after an action, with Undo.
    private(set) var toast: Toast?

    /// The window's undo manager, so ⌘Z and Edit > Undo reverse a mail
    /// action the way they reverse anything in a Mac app.
    weak var undoManager: UndoManager?

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
                        self.selection = [ThreadKey(account: account, thread: asked[1])]
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

    /// Runs `action` on `keys`, or on the conversations picked, puts it on
    /// the window's undo stack and says what it did.
    func perform(_ action: Action, on keys: Set<ThreadKey>? = nil) {
        guard let mail else { return }
        let picked = keys ?? selection
        guard !picked.isEmpty else { return }
        let threads = picked.map { ThreadRef(accountId: $0.account, threadId: $0.thread) }
        let from = selectedMailbox
        if keys == nil, Self.leavesList(action) { selection = [] }
        Task.detached {
            let done = mail.act(threads: threads, action: action, from: from)
            await MainActor.run { self.finished(done, undoable: true) }
        }
    }

    /// Opens or closes one message of the open conversation, as a click on
    /// its header does.
    func toggleMessage(_ id: String) {
        if let at = toggled.firstIndex(of: id) {
            toggled.remove(at: at)
        } else {
            toggled.append(id)
        }
        loadConversation()
    }

    /// Whether every conversation picked is read already, which turns
    /// Mark as Read into Mark as Unread.
    var selectionIsRead: Bool {
        let rows = listing?.rows.filter { selection.contains($0.key) } ?? []
        return !rows.isEmpty && !rows.contains { $0.unread }
    }

    /// Follows a link of the page's own: a message's header opens or
    /// closes it, a sender's name or picture offers what can be done with
    /// the address.
    func followLink(_ url: URL) {
        let text = url.absoluteString
        if let id = text.removingPrefix("mailrs:toggle/") {
            toggleMessage(id)
        } else if let address = text.removingPrefix("mailrs:contact/") ?? text.removingPrefix("mailto:") {
            ContactMenu.show(for: address.components(separatedBy: "?")[0].removingPercentEncoding ?? address)
        }
    }

    /// Loads the pictures on the web in the open conversation.
    func loadImages() {
        guard let key = openThread else { return }
        remoteAllowed.insert(key)
        loadConversation()
    }

    /// Marks the conversations picked read, or unread when every one of
    /// them is read already.
    func toggleRead() {
        perform(selectionIsRead ? .markUnread : .markRead)
    }

    /// Reverses the newest mail action.
    func undo() {
        guard let mail else { return }
        Task.detached {
            let done = mail.undo()
            await MainActor.run {
                if let done { self.finished(done, undoable: false) }
            }
        }
    }

    private func finished(_ done: ActionDone, undoable: Bool) {
        if let failed = done.failed {
            toast = Toast(words: failed, undo: false)
        } else if !done.words.isEmpty {
            toast = Toast(words: done.words, undo: undoable)
            if undoable, let undoManager {
                undoManager.registerUndo(withTarget: self) { model in model.undo() }
                undoManager.setActionName(done.words)
            }
        }
        refresh()
        let shown = toast
        Task { @MainActor in
            try? await Task.sleep(for: .seconds(5))
            if self.toast == shown { self.toast = nil }
        }
    }

    func dismissToast() { toast = nil }

    /// Whether an action takes the mail out of the list on screen.
    private static func leavesList(_ action: Action) -> Bool {
        switch action {
        case .archive, .trash, .junk, .mute: true
        default: false
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
        guard let store, let key = openThread else {
            page = nil
            return
        }
        // Opening a conversation reads it, as in the GTK app; the list
        // hears of it from the engine.
        if let mail, listing?.rows.contains(where: { $0.key == key && $0.unread }) == true {
            Task.detached { mail.markRead(thread: ThreadRef(accountId: key.account, threadId: key.thread)) }
        }
        let theme = theme
        let remote = remoteAllowed.contains(key)
        let toggled = toggled
        Task.detached {
            do {
                let page = try store.conversationPage(accountId: key.account, threadId: key.thread, theme: theme, allowRemote: remote, toggled: toggled)
                await MainActor.run {
                    if self.openThread == key { self.page = page }
                }
            } catch {
                await MainActor.run { self.problem = "\(error)" }
            }
        }
    }
}

/// What a toast says, and whether it offers Undo.
struct Toast: Equatable {
    let id = UUID()
    let words: String
    let undo: Bool
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

extension String {
    /// What follows `prefix`, when the string starts with it.
    func removingPrefix(_ prefix: String) -> String? {
        hasPrefix(prefix) ? String(dropFirst(prefix.count)) : nil
    }
}
