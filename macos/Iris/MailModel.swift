import AppKit
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
        didSet { if selectedMailbox != oldValue { category = openingCategory; loadList() } }
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

    fileprivate var mail: Mail?
    private var store: Store?

    /// Preferences as they stand, read again after each change. Shared
    /// with the GTK app's settings file.
    private(set) var prefs: Preferences?
    private(set) var choices: PreferenceChoices?

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
                    self.readPrefs()
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

    func checkAccount(_ id: Int64) {
        mail?.checkAccount(accountId: id)
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
        } else if let rest = text.removingPrefix("mailrs:preview/"), let (id, index) = Self.fileLink(rest) {
            openAttachment(message: id, index: index)
        } else if let rest = text.removingPrefix("mailrs:attachment/"), let (id, index) = Self.fileLink(rest) {
            saveAttachment(message: id, index: index)
        } else if let id = text.removingPrefix("mailrs:attachments/") {
            saveAllAttachments(message: id.removingPercentEncoding ?? id)
        } else if let address = text.removingPrefix("mailrs:contact/") ?? text.removingPrefix("mailto:") {
            ContactMenu.show(for: address.components(separatedBy: "?")[0].removingPercentEncoding ?? address)
        }
    }

    /// `<message>/<index>` from an attachment's link.
    private static func fileLink(_ rest: String) -> (String, UInt32)? {
        let parts = rest.split(separator: "/", maxSplits: 1).map(String.init)
        guard parts.count == 2, let index = UInt32(parts[1]) else { return nil }
        return (parts[0].removingPercentEncoding ?? parts[0], index)
    }

    /// Fetches an attachment of the open conversation off the main thread.
    private func withAttachment(message: String, index: UInt32, then: @escaping @MainActor (FilePart) -> Void) {
        guard let mail, let account = openThread?.account else { return }
        Task.detached {
            do {
                let part = try mail.attachment(accountId: account, messageId: message, index: index)
                await MainActor.run { then(part) }
            } catch {
                await MainActor.run { self.toast = Toast(words: "\(error)", undo: false) }
            }
        }
    }

    /// Opens an attachment with the app the Mac picks for its kind, from a
    /// copy in the temporary folder.
    func openAttachment(message: String, index: UInt32) {
        withAttachment(message: message, index: index) { part in
            let folder = FileManager.default.temporaryDirectory.appending(path: "Iris Attachments", directoryHint: .isDirectory)
            try? FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
            let file = folder.appending(path: Self.safeName(part.filename))
            do {
                try Data(part.data).write(to: file)
                NSWorkspace.shared.open(file)
            } catch {
                self.toast = Toast(words: "\(error)", undo: false)
            }
        }
    }

    /// Saves an attachment to Downloads, as the GTK app's download button does.
    func saveAttachment(message: String, index: UInt32) {
        withAttachment(message: message, index: index) { part in
            let downloads = FileManager.default.urls(for: .downloadsDirectory, in: .userDomainMask)[0]
            let file = Self.unused(downloads.appending(path: Self.safeName(part.filename)))
            do {
                try Data(part.data).write(to: file)
                self.toast = Toast(words: tr("Saved {file} to Downloads").replacingOccurrences(of: "{file}", with: file.lastPathComponent), undo: false)
            } catch {
                self.toast = Toast(words: tr("Could not save {file}: {reason}")
                    .replacingOccurrences(of: "{file}", with: part.filename)
                    .replacingOccurrences(of: "{reason}", with: error.localizedDescription), undo: false)
            }
        }
    }

    /// Saves every attachment of a message into a folder the person picks.
    func saveAllAttachments(message: String) {
        guard let mail, let account = openThread?.account else { return }
        let panel = NSOpenPanel()
        panel.canChooseDirectories = true
        panel.canChooseFiles = false
        panel.canCreateDirectories = true
        panel.prompt = tr("Save")
        panel.message = tr("Choose a Folder")
        guard panel.runModal() == .OK, let folder = panel.url else { return }
        Task.detached {
            do {
                let listed = try mail.attachments(accountId: account, messageId: message)
                for item in listed {
                    let part = try mail.attachment(accountId: account, messageId: message, index: item.index)
                    try Data(part.data).write(to: Self.unused(folder.appending(path: Self.safeName(part.filename))))
                }
                await MainActor.run {
                    self.toast = Toast(words: tr("Saved {file}").replacingOccurrences(of: "{file}", with: folder.lastPathComponent), undo: false)
                }
            } catch {
                await MainActor.run { self.toast = Toast(words: "\(error)", undo: false) }
            }
        }
    }

    /// A file name that cannot climb out of the folder it is saved in.
    nonisolated private static func safeName(_ name: String) -> String {
        let cleaned = name.replacingOccurrences(of: "/", with: "-").replacingOccurrences(of: ":", with: "-")
        return cleaned.isEmpty || cleaned.hasPrefix(".") ? "attachment" + cleaned : cleaned
    }

    /// `file`, or `file 2`, `file 3` and on, whichever is free.
    nonisolated private static func unused(_ file: URL) -> URL {
        var candidate = file
        var number = 2
        let stem = file.deletingPathExtension().lastPathComponent
        let ext = file.pathExtension
        while FileManager.default.fileExists(atPath: candidate.path(percentEncoded: false)) {
            let name = ext.isEmpty ? "\(stem) \(number)" : "\(stem) \(number).\(ext)"
            candidate = file.deletingLastPathComponent().appending(path: name)
            number += 1
        }
        return candidate
    }

    /// The handler the page's pictures come through, made once the store
    /// is open.
    var pictures: PictureScheme { PictureScheme(mail: mail) }

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

    // MARK: Writing mail

    /// Messages Undo Send brought back, until their composer takes them.
    private var kept: [UUID: ComposeDraft] = [:]
    /// The message waiting out its Undo Send delay, with its key.
    private var sending: (UUID, ComposeDraft)?

    /// Every address the accounts send as.
    func senders() async throws -> [Sender] {
        guard let mail else { return [] }
        return try await Task.detached { try mail.senders() }.value
    }

    /// The draft a composer opens on, made by the core.
    func startDraft(_ request: ComposeRequest) async throws -> ComposeDraft {
        guard let mail else { throw CoreError.Store(tr("Add an account to write mail.")) }
        let account = request.account ?? openThread?.account
        return try await Task.detached { () throws -> ComposeDraft in
            switch request.kind {
            case .reply, .replyAll, .forward:
                guard let account = request.account, let thread = request.thread else {
                    return try mail.newDraft(accountId: account)
                }
                let mode: ReplyMode = request.kind == .reply ? .reply : request.kind == .replyAll ? .replyAll : .forward
                return try mail.replyDraft(accountId: account, threadId: thread, mode: mode)
            default:
                return try mail.newDraft(accountId: account)
            }
        }.value
    }

    /// Saves a composer's draft on the server.
    func saveDraft(_ draft: ComposeDraft) async throws -> ComposeDraft {
        guard let mail else { return draft }
        return try await Task.detached { try mail.saveDraft(composed: draft) }.value
    }

    /// Deletes the server's copy of a discarded draft, when there is one.
    func discardDraft(_ draft: ComposeDraft) {
        guard let mail else { return }
        Task.detached { mail.deleteDraft(composed: draft) }
    }

    /// People a recipient field could mean.
    func suggest(_ field: String, account: Int64) async -> [Suggestion] {
        guard let mail else { return [] }
        return await Task.detached { mail.suggest(field: field, accountId: account) }.value
    }

    /// Shows `words` in the toast corner.
    func say(_ words: String) {
        toast = Toast(words: words, undo: false)
    }

    /// The message Undo Send kept under `key`, once.
    func takeKept(_ key: UUID) -> ComposeDraft? {
        kept.removeValue(forKey: key)
    }

    /// Asks the window to open a composer; ContentView carries it out,
    /// since only a view can open a window.
    var composeAsked: ComposeRequest?

    /// Sends `draft` after the Undo Send delay, saying so in a toast with
    /// Undo, which brings the message back in a composer instead.
    func send(_ draft: ComposeDraft) {
        let key = UUID()
        sending = (key, draft)
        toast = Toast(words: tr("Sending…"), undo: (prefs?.undoSeconds ?? 5) > 0, undoSend: key)
        Task { @MainActor in
            try? await Task.sleep(for: .seconds(Int(self.prefs?.undoSeconds ?? 5)))
            guard let (waiting, draft) = sending, waiting == key, let mail else { return }
            sending = nil
            Task.detached {
                let said: String
                do {
                    said = try mail.send(composed: draft)
                } catch {
                    said = "\(error)"
                    await MainActor.run {
                        // Not sent: the message comes back to be fixed.
                        self.kept[key] = draft
                        self.composeAsked = ComposeRequest(kind: .restore, restore: key)
                    }
                }
                await MainActor.run {
                    self.toast = Toast(words: said, undo: false)
                    self.refresh()
                }
            }
        }
    }

    /// Stops a message waiting to go and brings it back in its composer.
    func undoSend(_ key: UUID) {
        guard let (waiting, draft) = sending, waiting == key else { return }
        sending = nil
        kept[key] = draft
        toast = nil
        composeAsked = ComposeRequest(kind: .restore, restore: key)
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

    fileprivate func loadList() {
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
                // A mailbox that left the sidebar, such as a flag color
                // with nothing left under it, gives way to the first one
                // rather than an alert.
                await MainActor.run {
                    if self.selectedMailbox == key {
                        self.selectedMailbox = self.sidebar.first { $0.kind == "mailbox" }?.key
                    }
                }
            }
        }
    }

    fileprivate func loadConversation() {
        guard let store, let key = openThread else {
            page = nil
            return
        }
        markReadOnOpen(key)
        let theme = theme
        let remote = prefs?.remoteImages == "always" || remoteAllowed.contains(key)
        let toggled = toggled
        Task.detached {
            do {
                let page = try store.conversationPage(accountId: key.account, threadId: key.thread, theme: theme, allowRemote: remote, toggled: toggled)
                await MainActor.run {
                    if self.openThread == key { self.page = page }
                }
                // Bodies not fetched yet come from the server, and the page
                // is drawn again once they are in.
                if page.bodiesMissing, let mail = await self.mail {
                    let came = (try? mail.fetchBodies(thread: ThreadRef(accountId: key.account, threadId: key.thread))) ?? 0
                    if came > 0 {
                        await MainActor.run {
                            if self.openThread == key { self.loadConversation() }
                        }
                    }
                }
            } catch {
                await MainActor.run { self.problem = "\(error)" }
            }
        }
    }
}

// MARK: Preferences

extension MailModel {
    /// Reads Preferences again and applies what shows at once: the
    /// appearance, and the page, which Text Size and Remote Images change.
    func readPrefs() {
        guard let mail else { return }
        let before = prefs
        prefs = mail.preferences()
        if choices == nil { choices = mail.preferenceChoices() }
        applyAppearance()
        guard let before, let prefs else { return }
        if before.remoteImages != prefs.remoteImages { loadConversation() }
        if before.threading != prefs.threading || before.inboxCategories != prefs.inboxCategories
            || before.suggestFollowUps != prefs.suggestFollowUps {
            if !prefs.inboxCategories { category = nil }
            loadList()
        }
    }

    /// Changes one preference by its name in `Preferences`.
    func setPreference(_ name: String, _ value: String) {
        guard let mail else { return }
        do {
            try mail.setPreference(name: name, value: value)
        } catch {
            toast = Toast(words: "\(error)", undo: false)
        }
        readPrefs()
    }

    func setPreference(_ name: String, _ on: Bool) {
        setPreference(name, on ? "true" : "false")
    }

    func signatures() -> [SignatureSetting] {
        (try? mail?.signatures()) ?? []
    }

    /// Keeps a signature, Markdown or formatted, saying so when it cannot.
    func keepSignature(email: String, markdown: String? = nil, formatted: String? = nil) {
        guard let mail else { return }
        do {
            if let markdown { try mail.setSignature(email: email, text: markdown) }
            if let formatted { try mail.setFormattedSignature(email: email, html: formatted) }
        } catch {
            toast = Toast(words: "\(error)", undo: false)
        }
    }

    func setWorkingHours(_ hours: Hours) {
        guard let mail else { return }
        do {
            try mail.setWorkingHours(hours: hours)
        } catch {
            toast = Toast(words: "\(error)", undo: false)
        }
        readPrefs()
    }

    func imageSenders() -> [ImageSender] {
        (try? mail?.imageSenders()) ?? []
    }

    func forgetImageSender(_ sender: String) {
        guard let mail else { return }
        do {
            try mail.forgetImageSender(sender: sender)
        } catch {
            toast = Toast(words: "\(error)", undo: false)
        }
    }

    func syncSettings() -> SyncSettings? {
        mail?.syncSettings()
    }

    /// Saves the sync settings; the engine starts again with them, off the
    /// main thread, since stopping it waits for the accounts.
    func setSyncSettings(_ settings: SyncSettings) {
        guard let mail else { return }
        Task.detached {
            do {
                try mail.setSyncSettings(settings: settings)
            } catch {
                await MainActor.run { self.toast = Toast(words: "\(error)", undo: false) }
            }
        }
    }

    /// The signature Gmail keeps for an account, asked off the main thread.
    func gmailSignature(account: Int64) async throws -> String? {
        guard let mail else { return nil }
        return try await Task.detached { try mail.gmailSignature(accountId: account) }.value
    }

    /// The inbox category a mailbox opens on: Settings' choice, or the
    /// whole inbox when categories are off.
    var openingCategory: String? {
        guard let prefs, prefs.inboxCategories, prefs.defaultCategory != "all" else { return nil }
        return prefs.defaultCategory
    }

    /// Follows Style: the system's appearance, or always light or dark.
    private func applyAppearance() {
        switch prefs?.colorScheme {
        case "light": NSApp.appearance = NSAppearance(named: .aqua)
        case "dark": NSApp.appearance = NSAppearance(named: .darkAqua)
        default: NSApp.appearance = nil
        }
    }

    /// Marks a conversation read on opening, as Mark as Read says: at
    /// once, after two seconds while it stays open, or never.
    fileprivate func markReadOnOpen(_ key: ThreadKey) {
        guard let mail, listing?.rows.contains(where: { $0.key == key && $0.unread }) == true else { return }
        let thread = ThreadRef(accountId: key.account, threadId: key.thread)
        switch prefs?.markRead {
        case "manually":
            return
        case "after-delay":
            Task { @MainActor in
                try? await Task.sleep(for: .seconds(2))
                guard self.openThread == key else { return }
                Task.detached { mail.markRead(thread: thread) }
            }
        default:
            Task.detached { mail.markRead(thread: thread) }
        }
    }
}

// MARK: Assistant

extension MailModel {
    /// The assistant of this window, once the store is open.
    func makeAssistant(window: AssistantWindow) -> Assistant? {
        mail?.assistant(window: window)
    }

    /// What the window shows, which the assistant reads as "this
    /// conversation" and "the selected mail".
    var assistantScreen: AssistantScreen {
        let open = openThread
        let subject = open.flatMap { key in listing?.rows.first { $0.key == key }?.subject } ?? ""
        return AssistantScreen(
            mailbox: listing?.title ?? "",
            open: open.map { ThreadRef(accountId: $0.account, threadId: $0.thread) },
            openSubject: subject,
            selected: selection.map { ThreadRef(accountId: $0.account, threadId: $0.thread) }
        )
    }

    /// Opens a composer on a message someone else started, such as the
    /// assistant.
    func openComposer(_ draft: ComposeDraft) {
        let key = UUID()
        kept[key] = draft
        composeAsked = ComposeRequest(kind: .restore, restore: key)
    }

    /// Opens one conversation, wherever it is.
    func show(account: Int64, thread: String) {
        selection = [ThreadKey(account: account, thread: thread)]
    }
}

/// What a toast says, and whether it offers Undo.
struct Toast: Equatable {
    let id = UUID()
    let words: String
    let undo: Bool
    /// Set while the toast stands for a message waiting to go.
    var undoSend: UUID? = nil
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

// MARK: AI settings

extension MailModel {
    func aiPreferences() -> AiPreferences? { mail?.aiPreferences() }

    func aiChoices() -> AiChoices? { mail?.aiChoices() }

    /// Runs one AI settings change, and says what went wrong if it did.
    func changeAi(_ change: (Mail) throws -> Void) {
        guard let mail else { return }
        do {
            try change(mail)
        } catch {
            toast = Toast(words: "\(error)", undo: false)
        }
    }

    /// Keeps a key in the Keychain, off the main thread: macOS may stop to
    /// ask whether Iris may use it.
    func saveAiKey(_ name: String, _ value: String) async throws {
        guard let mail else { return }
        try await Task.detached { try mail.saveAiKey(name: name, value: value) }.value
    }

    func testAi(_ provider: String) async throws -> String {
        guard let mail else { return "" }
        return try await Task.detached { try mail.testAi(provider: provider) }.value
    }

    func listAiModels(_ provider: String) async throws -> ModelListing? {
        guard let mail else { return nil }
        return try await Task.detached { try mail.listAiModels(provider: provider) }.value
    }

    func findAi() async -> [FoundModel] {
        guard let mail else { return [] }
        return await Task.detached { mail.findAi() }.value
    }
}
