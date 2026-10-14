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
    /// The invitation the open conversation carries, for the card above
    /// its page.
    private(set) var invitation: MailInvitation?
    /// The conversation `invitation` belongs to.
    fileprivate var invitationKey: ThreadKey?
    /// What gpg or gpgsm found in the open conversation's protected
    /// messages, oldest first.
    private(set) var protectionMarks: [ProtectionMark] = []
    fileprivate var protectionKey: ThreadKey?
    /// The card offering to translate the open conversation's message.
    private(set) var translationCard: TranslationOffer?
    fileprivate var translationKey: ThreadKey?
    /// Set while the model translates.
    private(set) var translating = false
    /// A translation the Mac's own translator has to do, while it does.
    var appleJob: AppleTranslationJob?
    /// What the sender menu offers for the open conversation.
    private(set) var senderMenu: SenderMenu?
    fileprivate var senderMenuKey: ThreadKey?
    /// The smart mailbox open in its editor.
    var smartEditing: SmartEdit?
    /// View Source's message, while its sheet is up.
    var source: MessageSource?
    /// Delete Forever's question, while it is up.
    var erasing: EraseRequest?
    /// The account sheet the sidebar's account menu opened.
    var accountSheet: AccountSheet?
    /// The contact card a click on a sender opened.
    var contactCard: ContactCard?
    /// The offer to leave the open conversation's mailing list.
    private(set) var listOffer: ListOffer?
    fileprivate var listOfferKey: ThreadKey?
    /// Conversations whose pictures on the web the person chose to load.
    private var remoteAllowed: Set<ThreadKey> = []
    /// Messages of the open conversation the person opened or closed.
    private var toggled: [String] = []
    private(set) var problem: String?
    /// The problem is a newer Iris's mail, which updating this copy solves.
    private(set) var problemWantsUpdate = false
    /// Why this window does not sync, when it does not: another copy of
    /// Iris holds the store.
    private(set) var notSyncing: String?

    var selectedMailbox: String? {
        didSet {
            guard selectedMailbox != oldValue else { return }
            // Picking a mailbox ends a search and closes its bar, as in
            // the GTK app.
            if searching != nil { searchText = "" }
            searchOpen = false
            category = openingCategory
            loadList()
        }
    }
    /// Whether the search bar shows over the list. Closing it ends the
    /// search.
    var searchOpen = false {
        didSet { if !searchOpen, oldValue { searchText = "" } }
    }
    /// What the search field holds. A pause in the typing runs it.
    var searchText = "" {
        didSet { if searchText != oldValue { typed() } }
    }
    /// The search on screen, in place of the mailbox; nil shows the mailbox.
    private(set) var searching: String? {
        didSet { if searching != oldValue { loadList() } }
    }
    /// Whether the search on screen goes to the servers too, or only to
    /// the mail kept on this Mac.
    private var searchServers = false
    /// Searches to offer for what the field holds.
    private(set) var suggestions: [SearchSuggestion] = []
    private var pause: Task<Void, Never>?
    var category: String? {
        didSet { if category != oldValue { loadList() } }
    }
    /// The conversations picked in the list; one of them opens.
    var selection: Set<ThreadKey> = [] {
        didSet {
            if openThread != Self.single(oldValue) {
                toggled = []
                openHoldsDraft = false
                // Another conversation closes the find bar and drops its
                // query; closing it draws the page, so it is drawn once.
                if findShown { findShown = false } else { loadConversation() }
                if let open = openThread { notifier?.seen(account: open.account, thread: open.thread) }
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
    @ObservationIgnored private var palette: [AccountColorChoice] = []
    @ObservationIgnored private var notifier: Notifier?

    /// The window's undo manager, so ⌘Z and Edit > Undo reverse a mail
    /// action the way they reverse anything in a Mac app.
    weak var undoManager: UndoManager?

    /// The colors pages are drawn in, which the window sets from its
    /// appearance and the system accent.
    var theme = PageTheme(dark: false, accent: "#007aff", accentText: "#0060df") {
        didSet { if theme != oldValue { loadConversation() } }
    }

    private(set) var mail: Mail?
    private var store: Store?

    /// Preferences as they stand, read again after each change. Shared
    /// with the GTK app's settings file.
    /// Whether the store holds no account yet: the window shows the
    /// welcome page then.
    private(set) var noAccounts = false
    /// Asks for the Add Account sheet.
    var addAccount: AddAccountRequest?
    private(set) var prefs: Preferences?
    private(set) var choices: PreferenceChoices?

    /// Where Iris keeps the mail store, or the store `IRIS_STORE` names,
    /// such as a demo run's throwaway one.
    nonisolated static var storePath: String {
        if let named = ProcessInfo.processInfo.environment["IRIS_STORE"], !named.isEmpty {
            return named
        }
        return FileManager.default.homeDirectoryForCurrentUser
            .appending(path: "Library/Application Support/iris/mailrs.db")
            .path(percentEncoded: false)
    }

    /// Set once the store is being opened. Every main window asks for it
    /// as it appears, and a second core would find the first one holding
    /// the sync lock and take its place without syncing.
    private var opened = false

    func open() {
        guard !opened else { return }
        opened = true
        Task.detached {
            do {
                // `IRIS_DEMO=1` makes the store `IRIS_STORE` names and fills
                // it with the sample accounts, for screenshots.
                let demo = ProcessInfo.processInfo.environment["IRIS_DEMO"] == "1"
                let mail = demo
                    ? try Mail.openDemo(path: MailModel.storePath)
                    : try Mail.open(path: MailModel.storePath)
                let store = try Store.open(path: MailModel.storePath)
                let sidebar = try mail.sidebar()
                let badge = mail.dockBadge()
                // `IRIS_OPEN=<account id>:<thread id>` opens one conversation
                // at start, as the screenshots do.
                let asked = ProcessInfo.processInfo.environment["IRIS_OPEN"]?
                    .split(separator: ":", maxSplits: 1).map(String.init)
                await MainActor.run {
                    self.mail = mail
                    self.store = store
                    self.readPrefs()
                    self.startSync()
                    self.sidebar = sidebar
                    NSApp.dockTile.badgeLabel = badge
                    self.noAccounts = mail.hasNoAccounts()
                    self.selectedMailbox = sidebar.first { $0.kind == "mailbox" }?.key
                    if let asked, asked.count == 2, let account = Int64(asked[0]) {
                        self.selection = [ThreadKey(account: account, thread: asked[1])]
                    }
                    // A demo run starts from a script, not the Dock, so it
                    // comes forward itself; a screenshot of a window in the
                    // background shows it dimmed.
                    if demo { NSApp.activate() }
                }
            } catch {
                await MainActor.run { self.show(problem: error) }
            }
        }
    }

    /// Starts the sync engine, and reads the sidebar and the list again a
    /// moment after each burst of news from it.
    private func startSync() {
        guard let mail else { return }
        let notifier = Notifier(model: self)
        notifier.start()
        self.notifier = notifier
        let listener = Listener { [weak self] in
            Task { @MainActor in self?.heardChange() }
        } news: { [weak self] notices in
            Task { @MainActor in self?.announce(notices) }
        }
        // Taking the store can wait a few seconds for a copy that is still
        // quitting, so it runs off the main thread.
        Task.detached {
            let started = Result { try mail.startSync(listener: listener) }
            await MainActor.run {
                switch started {
                case .success: self.notSyncing = nil
                case .failure(let error): self.notSyncing = "\(error)"
                }
            }
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

    /// Reads the sidebar, the shown list and the Dock's badge again.
    func refresh() {
        guard let mail else { return }
        Task.detached {
            let badge = mail.dockBadge()
            await MainActor.run { NSApp.dockTile.badgeLabel = badge }
            if let sidebar = try? mail.sidebar() {
                await MainActor.run {
                    self.sidebar = sidebar
                    self.loadList()
                    if self.space == .calendar { self.loadCalendar() }
                }
            }
        }
    }

    // MARK: Notifications

    private func announce(_ notices: [MailNotice]) {
        guard let mail else { return }
        notifier?.show(notices, buttons: mail.noticeButtons())
    }

    /// A click on a notification: Iris comes forward on the conversation,
    /// in its account's inbox.
    func openFromNotice(account: Int64, thread: String) {
        NSApp.activate()
        openInbox(of: account)
        selection = [ThreadKey(account: account, thread: thread)]
    }

    /// A button on a notification. Reply opens the composer; the others
    /// change the mail, with Undo, unless it moved on since.
    func noticeChose(account: Int64, thread: String, message: String, button: String) {
        guard let mail else { return }
        Task.detached {
            let answer = Result { try mail.noticeChose(accountId: account, threadId: thread, messageId: message, button: button) }
            await MainActor.run {
                switch answer {
                case .success(.reply):
                    self.openFromNotice(account: account, thread: thread)
                    self.composeAsked = ComposeRequest(kind: .reply, account: account, thread: thread)
                case .success(.done(let done)): self.finished(done, undoable: true)
                case .success(.nothing): break
                case .failure(let error): self.toast = Toast(words: "\(error)", undo: false)
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
        // Delete in the Trash erases for good, so it asks first.
        if action == .trash, let question = try? mail.eraseQuestion(threads: threads, from: from) {
            erasing = EraseRequest(threads: threads, question: question, clears: keys == nil)
            return
        }
        if keys == nil, Self.leavesList(action) { selection = [] }
        Task.detached {
            let done = mail.act(threads: threads, action: action, from: from)
            await MainActor.run { self.finished(done, undoable: true) }
        }
    }

    /// Erases what Delete Forever's question asked about, on a yes.
    func erase(_ request: EraseRequest) {
        guard let mail else { return }
        if request.clears { selection = [] }
        let threads = request.threads
        Task.detached {
            let done = mail.erase(threads: threads)
            await MainActor.run { self.finished(done, undoable: false) }
        }
    }

    // MARK: Calendar state (see CalendarView.swift)

    /// Which space the window shows, as the GTK window remembers it.
    var space: Space = Space(rawValue: UserDefaults.standard.string(forKey: "space") ?? "") ?? .mail {
        didSet {
            UserDefaults.standard.set(space.rawValue, forKey: "space")
            if space == .calendar { loadCalendar() }
        }
    }
    /// The day the calendar shows, or the week or month around it.
    var calendarDay = Date()
    /// Day, Week or Month, as the GTK calendar remembers it.
    var calendarKind: CalendarKind = CalendarKind(rawValue: UserDefaults.standard.string(forKey: "calendarKind") ?? "") ?? .week {
        didSet {
            UserDefaults.standard.set(calendarKind.rawValue, forKey: "calendarKind")
            loadCalendar()
        }
    }
    private(set) var monthView: MonthView?
    private(set) var weekView: WeekView?
    private(set) var calendarAccounts: [CalendarAccount] = []
    /// The event a double click opened, read only.
    var openedEvent: EventDetails?
    /// The editor open on an event of the person's own, or a new one.
    var editing: EditorRequest?
    /// A drag let go on an event that needs a question before it is
    /// written.
    var dropping: DropRequest?
    /// Counts the calendar's reads, so a card held where a drag left it
    /// goes back to the grid's own place once the grid is read again.
    private(set) var calendarLoads = 0
    /// How many lines of events a week row holds at the window's height.
    var monthRows: UInt32 = 3 {
        didSet { if monthRows != oldValue { loadCalendar() } }
    }

    /// Reads the calendars and the month shown again.
    func loadCalendar() {
        guard let mail else { return }
        let parts = Calendar.current.dateComponents([.year, .month], from: calendarDay)
        let (year, month, rows) = (Int32(parts.year ?? 2026), UInt32(parts.month ?? 1), monthRows)
        // The locale's own first weekday, counted from Monday as the core
        // counts: Foundation counts from Sunday, as 1.
        let first = UInt32((Calendar.current.firstWeekday + 5) % 7)
        let kind = calendarKind
        let day = Self.dayKey(calendarDay)
        Task.detached {
            let accounts = (try? mail.calendarAccounts()) ?? []
            switch kind {
            case .month:
                let view = try? mail.monthView(year: year, month: month, localeFirst: first, rows: rows)
                await MainActor.run {
                    self.calendarAccounts = accounts
                    if let view { self.monthView = view }
                    self.calendarLoads += 1
                }
            case .week, .day:
                let view = try? mail.weekView(date: day, days: kind == .day ? 1 : 7, localeFirst: first)
                await MainActor.run {
                    self.calendarAccounts = accounts
                    if let view { self.weekView = view }
                    self.calendarLoads += 1
                }
            }
        }
    }

    /// The open conversation ends in a draft: the window offers Edit
    /// Draft in place of replying.
    private(set) var openHoldsDraft = false

    // MARK: Find in the conversation

    /// Whether the find bar shows over the conversation. While it does,
    /// every message is open, so it finds what a closed one holds; closing
    /// it closes them again.
    var findShown = false {
        didSet { if findShown != oldValue { loadConversation() } }
    }

    /// Asks the open page to find; the page view answers it.
    let finder = PageFinder()

    // MARK: Labels and folders

    /// Whether the label picker over the conversation is open; L opens it.
    var filingShown = false

    private func refs(_ keys: Set<ThreadKey>?) -> [ThreadRef] {
        (keys ?? selection).map { ThreadRef(accountId: $0.account, threadId: $0.thread) }
    }

    /// The label picker for `keys`, or the conversations picked.
    func filingMenu(for keys: Set<ThreadKey>? = nil) -> FilingMenu? {
        try? mail?.filingMenu(threads: refs(keys))
    }

    /// Puts the row's label on, takes it off, or moves the mail to the
    /// row's folder, with Undo.
    func file(_ row: FilingRow, on keys: Set<ThreadKey>? = nil) {
        guard let mail else { return }
        let threads = refs(keys)
        guard !threads.isEmpty else { return }
        let from = selectedMailbox
        Task.detached {
            let done = mail.file(threads: threads, id: row.id, name: row.name, applied: row.applied, from: from)
            await MainActor.run { self.finished(done, undoable: true) }
        }
    }

    /// Makes a label called `name` on the picked mail's account, then puts
    /// the mail under it.
    func createLabel(_ name: String, on keys: Set<ThreadKey>? = nil) {
        guard let mail, let account = (keys ?? selection).first?.account else { return }
        let name = name.trimmingCharacters(in: .whitespaces)
        guard !name.isEmpty else { return }
        Task.detached {
            do {
                let row = try mail.createLabel(accountId: account, name: name)
                await MainActor.run { self.file(row, on: keys) }
            } catch {
                await MainActor.run { self.toast = Toast(words: "\(error)", undo: false) }
            }
        }
    }

    /// Mail dragged from the list onto the sidebar mailbox `to`.
    func drop(_ keys: [ThreadKey], on to: String) {
        guard let mail, !keys.isEmpty else { return }
        let threads = keys.map { ThreadRef(accountId: $0.account, threadId: $0.thread) }
        let from = selectedMailbox
        Task.detached {
            let done = Result { try mail.dropOn(threads: threads, from: from, to: to) }
            await MainActor.run {
                switch done {
                case .success(let done): self.finished(done, undoable: true)
                case .failure(let error): self.toast = Toast(words: "\(error)", undo: false)
                }
            }
        }
    }

    // MARK: The label menu

    func labelLook(_ key: String) -> LabelLook? { try? mail?.labelLook(key: key) }

    func renameLabel(_ key: String, to name: String) {
        changeLabels { try $0.renameLabel(key: key, name: name); return nil }
    }

    func deleteLabel(_ key: String) {
        if selectedMailbox == key { selectedMailbox = sidebar.first { $0.kind == "mailbox" }?.key }
        changeLabels { try $0.deleteLabel(key: key) }
    }

    func colorLabel(_ key: String, index: Int) {
        changeLabels { try $0.colorLabel(key: key, index: UInt32(index)); return nil }
    }

    func moveLabel(_ key: String, step: Int32) {
        changeLabels { try $0.moveLabel(key: key, step: step); return nil }
    }

    func newLabel(account: Int64, name: String) {
        changeLabels { try $0.newLabel(accountId: account, name: name); return nil }
    }

    /// Runs one label change off the main thread, since it waits on the
    /// server, then reads the sidebar again and says what it did or why
    /// it could not.
    private func changeLabels(_ change: @escaping @Sendable (Mail) throws -> String?) {
        guard let mail else { return }
        Task.detached {
            let done = Result { try change(mail) }
            await MainActor.run {
                switch done {
                case .success(let words): if let words { self.toast = Toast(words: words, undo: false) }
                case .failure(let error): self.toast = Toast(words: "\(error)", undo: false)
                }
                self.refresh()
            }
        }
    }

    /// The flag every one of `keys` wears, as its color, when they share
    /// one; the flag menu ticks it.
    func sharedFlag(of keys: Set<ThreadKey>? = nil) -> String? {
        let picked = keys ?? selection
        let flags = Set((listing?.rows ?? []).filter { picked.contains($0.key) }.map(\.flag))
        guard flags.count == 1, let flag = flags.first else { return nil }
        return flag
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
    /// `account` and `thread` name the conversation when the page is not
    /// the main window's, such as a conversation in its own window.
    func followLink(_ url: URL, account: Int64? = nil, thread: String? = nil) {
        let text = url.absoluteString
        if let id = text.removingPrefix("mailrs:toggle/") {
            toggleMessage(id)
        } else if let (kind, id) = Self.answerLink(text) {
            // A message's own Reply, Reply All or Forward answers it.
            guard let account = account ?? openThread?.account, let thread = thread ?? openThread?.thread else { return }
            composeAsked = ComposeRequest(kind: kind, account: account, thread: thread, message: id)
        } else if let rest = text.removingPrefix("mailrs:preview/"), let (id, index) = Self.fileLink(rest) {
            openAttachment(message: id, index: index, account: account)
        } else if let rest = text.removingPrefix("mailrs:attachment/"), let (id, index) = Self.fileLink(rest) {
            saveAttachment(message: id, index: index, account: account)
        } else if let id = text.removingPrefix("mailrs:attachments/") {
            saveAllAttachments(message: id.removingPercentEncoding ?? id, account: account)
        } else if let address = text.removingPrefix("mailrs:contact/") ?? text.removingPrefix("mailto:") {
            let address = address.components(separatedBy: "?")[0].removingPercentEncoding ?? address
            if text.hasPrefix("mailto:") {
                composeAsked = ComposeRequest(kind: .new, to: address)
            } else {
                showContact(address)
            }
        }
    }

    /// What a message header's answer link asks for, and of which message.
    private static func answerLink(_ text: String) -> (ComposeRequest.Kind, String)? {
        let kinds: [(String, ComposeRequest.Kind)] = [
            ("mailrs:reply/", .reply), ("mailrs:reply-all/", .replyAll), ("mailrs:forward/", .forward),
        ]
        for (prefix, kind) in kinds {
            if let id = text.removingPrefix(prefix) { return (kind, id.removingPercentEncoding ?? id) }
        }
        return nil
    }

    /// `<message>/<index>` from an attachment's link.
    private static func fileLink(_ rest: String) -> (String, UInt32)? {
        let parts = rest.split(separator: "/", maxSplits: 1).map(String.init)
        guard parts.count == 2, let index = UInt32(parts[1]) else { return nil }
        return (parts[0].removingPercentEncoding ?? parts[0], index)
    }

    /// Fetches an attachment of the open conversation off the main thread.
    private func withAttachment(message: String, index: UInt32, account: Int64? = nil, then: @escaping @MainActor (FilePart) -> Void) {
        guard let mail, let account = account ?? openThread?.account else { return }
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
    func openAttachment(message: String, index: UInt32, account: Int64? = nil) {
        withAttachment(message: message, index: index, account: account) { part in
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
    func saveAttachment(message: String, index: UInt32, account: Int64? = nil) {
        withAttachment(message: message, index: index, account: account) { part in
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
    func saveAllAttachments(message: String, account: Int64? = nil) {
        guard let mail, let account = account ?? openThread?.account else { return }
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

    // MARK: A conversation in its own window

    /// A conversation to open in a window of its own; the window answers.
    var windowAsked: ThreadKey?

    /// Opens `key` in its own window, as a double click on its row does
    /// in the GTK app.
    func openInWindow(_ key: ThreadKey? = nil) {
        guard let key = key ?? openThread else {
            toast = Toast(words: tr("Open a conversation first"), undo: false)
            return
        }
        windowAsked = key
    }

    /// The page of `key` for its own window, with `toggled` messages
    /// opened or closed, and marks it read as opening it here does.
    func windowPage(_ key: ThreadKey, toggled: [String]) async -> ConversationPage? {
        guard let store else { return nil }
        markReadOnOpen(key)
        let theme = theme
        let remote = prefs?.remoteImages == "always" || remoteAllowed.contains(key)
        let page = try? await Task.detached {
            try store.conversationPage(accountId: key.account, threadId: key.thread, theme: theme, allowRemote: remote, toggled: toggled, openAll: false)
        }.value
        // Bodies not fetched yet come from the server; the window asks
        // again once they are in.
        if let page, page.bodiesMissing, let mail {
            let came = await Task.detached {
                (try? mail.fetchBodies(thread: ThreadRef(accountId: key.account, threadId: key.thread))) ?? 0
            }.value
            if came > 0 { return await windowPage(key, toggled: toggled) }
        }
        return page
    }

    /// The subject a window for `key` is titled with.
    func windowTitle(_ key: ThreadKey) -> String {
        let subject = listing?.rows.first { $0.key == key }?.subject ?? ""
        return subject.isEmpty ? tr("Conversation") : subject
    }

    /// Loads the pictures on the web in the open conversation.
    func loadImages() {
        guard let key = openThread else { return }
        remoteAllowed.insert(key)
        loadConversation()
    }

    /// Lets images load from now on in mail from `email`, or from anyone
    /// at its domain, and loads them in the open conversation at once, even
    /// where another sender in it is not on the list.
    func alwaysLoadImages(from email: String, wholeDomain: Bool) {
        guard let mail, let key = openThread else { return }
        do {
            let words = try mail.allowImageSender(email: email, wholeDomain: wholeDomain)
            remoteAllowed.insert(key)
            loadConversation()
            toast = Toast(words: words, undo: false)
        } catch {
            toast = Toast(words: "\(error)", undo: false)
        }
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
                return try mail.replyDraft(accountId: account, threadId: thread, messageId: request.message, mode: mode)
            case .editDraft:
                guard let account = request.account, let thread = request.thread else {
                    return try mail.newDraft(accountId: account)
                }
                return try mail.editDraft(accountId: account, threadId: thread)
            default:
                var draft = try mail.newDraft(accountId: account)
                if let to = request.to { draft.to = to }
                return draft
            }
        }.value
    }

    /// Saves a composer's draft on the server.
    func saveDraft(_ draft: ComposeDraft) async throws -> ComposeDraft {
        guard let mail else { return draft }
        let kept = try await Task.detached { try mail.saveDraft(composed: draft) }.value
        // Drafts shows it now, not at the next sync's news.
        refresh()
        return kept
    }

    /// Deletes the server's copy of a discarded draft, when there is one.
    /// Lets a message go, with the draft saved of it, and says which came
    /// of it: discarded, or why the saved copy stays in Drafts.
    func discardDraft(_ draft: ComposeDraft) {
        guard let mail else { return }
        Task.detached {
            let said: String
            do {
                try mail.deleteDraft(composed: draft)
                said = tr("Message discarded")
            } catch {
                said = "\(error)"
            }
            await MainActor.run { self.say(said) }
        }
    }

    /// People a recipient field could mean.
    func suggest(_ field: String, account: Int64) async -> [Suggestion] {
        guard let mail else { return [] }
        return await Task.detached { mail.suggest(field: field, accountId: account) }.value
    }

    /// Puts up the notice that the mail could not be read, offering
    /// Check for Updates when a newer Iris has updated the mail.
    private func show(problem error: Error) {
        problem = "\(error)"
        if case CoreError.NewerStore = error { problemWantsUpdate = true }
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
                var offer: SaveOffer?
                do {
                    said = try mail.send(composed: draft)
                    offer = mail.saveOffer(accountId: draft.accountId, to: draft.to, cc: draft.cc, bcc: draft.bcc)
                } catch {
                    said = "\(error)"
                    await MainActor.run {
                        // Not sent: the message comes back to be fixed.
                        self.kept[key] = draft
                        self.composeAsked = ComposeRequest(kind: .restore, restore: key)
                    }
                }
                await MainActor.run { [offer] in
                    if let offer {
                        self.toast = Toast(words: said + "\n" + offer.title, undo: false, offer: offer)
                    } else {
                        self.toast = Toast(words: said, undo: false)
                    }
                    self.refresh()
                }
            }
        }
    }

    /// Save on the toast after sending: the new recipients go to the
    /// account's contacts.
    func saveOffered(_ offer: SaveOffer) {
        guard let mail else { return }
        toast = nil
        Task.detached {
            let done = Result { try mail.saveOffered(offer: offer) }
            await MainActor.run {
                switch done {
                case .success(let words): self.say(words)
                case .failure(let error): self.say("\(error)")
                }
            }
        }
    }

    /// The person closed the question: those people are not asked about
    /// again.
    func declineOffered(_ offer: SaveOffer) {
        guard let mail else { return }
        Task.detached { mail.declineOffered(offer: offer) }
    }

    /// Keeps the message to go out at `at`. A message the core refuses
    /// comes back in its composer, as one Send refuses does.
    func sendLater(_ draft: ComposeDraft, at: Int64) {
        guard let mail else { return }
        Task.detached {
            do {
                let said = try mail.sendLater(composed: draft, at: at)
                await MainActor.run {
                    self.toast = Toast(words: said, undo: false)
                    self.refresh()
                }
            } catch {
                await MainActor.run {
                    let key = UUID()
                    self.kept[key] = draft
                    self.composeAsked = ComposeRequest(kind: .restore, restore: key)
                    self.toast = Toast(words: "\(error)", undo: false)
                }
            }
        }
    }

    /// The times Send Later and Remind Me offer, read as the menu opens.
    func sendLaterTimes() -> [TimeChoice] { mail?.sendLaterTimes() ?? [] }
    func remindTimes() -> [TimeChoice] { mail?.remindTimes() ?? [] }

    /// Whether the list shows Send Later or Remind Me, where Delete
    /// cancels the sending or the reminder.
    var inSendLater: Bool { selectedMailbox == "Scheduled" }
    var inReminders: Bool { selectedMailbox == "Reminders" }
    /// A Trash, unified or one account's, where Delete erases for good.
    var inTrash: Bool { selectedMailbox?.contains("folder: Trash") ?? false }

    /// What Delete does in the list on screen, in words.
    var deleteWords: String {
        inSendLater ? tr("Cancel Send") : inReminders ? tr("Cancel Reminder") : inTrash ? tr("Delete Forever") : tr("Delete")
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
        if let query = searching {
            // The mail on this Mac answers first, then the servers, as in
            // the GTK app. When the servers find nothing, as when the
            // accounts are not connected, the stored rows stay.
            let servers = searchServers
            Task.detached {
                let stored = try? mail.search(query: query, scopeKey: key, servers: false)
                if let stored {
                    await MainActor.run {
                        if self.searching == query { self.listing = stored }
                    }
                }
                guard servers else { return }
                do {
                    let found = try mail.search(query: query, scopeKey: key, servers: true)
                    await MainActor.run {
                        guard self.searching == query else { return }
                        if found.rows.isEmpty, let stored, !stored.rows.isEmpty { return }
                        self.listing = found
                    }
                } catch {
                    await MainActor.run {
                        if self.searching == query, stored == nil { self.toast = Toast(words: "\(error)", undo: false) }
                    }
                }
            }
            return
        }
        let category = category
        Task.detached {
            do {
                let listing = try mail.list(key: key, category: category)
                await MainActor.run {
                    // The person may have picked another mailbox, or
                    // started a search, meanwhile.
                    if self.selectedMailbox == key, self.category == category, self.searching == nil {
                        self.listing = listing
                    }
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
            invitation = nil
            listOffer = nil
            translationCard = nil
            protectionMarks = []
            return
        }
        if invitationKey != key { invitation = nil }
        loadInvitation()
        if listOfferKey != key { listOffer = nil }
        loadListOffer()
        if translationKey != key { translationCard = nil }
        loadTranslationCard()
        if protectionKey != key { protectionMarks = [] }
        loadProtection()
        if senderMenuKey != key { senderMenu = nil }
        loadSenderMenu()
        markReadOnOpen(key)
        let theme = theme
        let remote = prefs?.remoteImages == "always" || remoteAllowed.contains(key)
        let toggled = toggled
        let openAll = findShown
        Task.detached {
            do {
                let page = try store.conversationPage(accountId: key.account, threadId: key.thread, theme: theme, allowRemote: remote, toggled: toggled, openAll: openAll)
                let draft = await self.mail?.holdsDraft(accountId: key.account, threadId: key.thread) ?? false
                await MainActor.run {
                    if self.openThread == key {
                        self.page = page
                        self.openHoldsDraft = draft
                    }
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
                await MainActor.run { self.show(problem: error) }
            }
        }
    }
}

// MARK: Invitations

extension MailModel {
    /// Reads the open conversation's invitation again, for its card.
    func loadInvitation() {
        guard let mail, let key = openThread else {
            invitation = nil
            return
        }
        Task.detached {
            let found = try? mail.invitation(accountId: key.account, threadId: key.thread)
            await MainActor.run {
                if self.openThread == key {
                    self.invitation = found ?? nil
                    self.invitationKey = key
                }
            }
        }
    }

    /// Switches to the calendar on the day of the event at `spot`, with
    /// its card open.
    func showSpot(_ spot: EventSpot) {
        guard let mail else { return }
        calendarDay = Date(timeIntervalSince1970: TimeInterval(spot.start) / 1000)
        space = .calendar
        loadCalendar()
        Task.detached {
            let details = try? mail.eventAt(spot: spot)
            await MainActor.run {
                if let details = details ?? nil {
                    self.openEvent(details)
                } else {
                    self.say(tr("The event is no longer on the calendar"))
                }
            }
        }
    }
}

// MARK: The sender menu

extension MailModel {
    func loadSenderMenu() {
        guard let mail, let key = openThread else {
            senderMenu = nil
            return
        }
        Task.detached {
            let found = try? mail.senderMenu(accountId: key.account, threadId: key.thread)
            await MainActor.run {
                if self.openThread == key {
                    self.senderMenu = found ?? nil
                    self.senderMenuKey = key
                }
            }
        }
    }

    /// Sorts the open conversation's sender into the category `key` names.
    func categorizeSender(_ key: String) {
        senderAction { mail, open in try mail.categorizeSender(accountId: open.account, threadId: open.thread, key: key) }
    }

    /// Blocks the open conversation's sender, once the person said yes.
    func blockSender() {
        senderAction { mail, open in try mail.blockSender(accountId: open.account, threadId: open.thread) }
    }

    private func senderAction(_ run: @escaping @Sendable (Mail, ThreadKey) throws -> String) {
        guard let mail, let open = openThread else { return }
        Task.detached {
            let done = Result { try run(mail, open) }
            await MainActor.run {
                switch done {
                case .success(let words): self.say(words)
                case .failure(let error): self.say("\(error)")
                }
                self.refresh()
            }
        }
    }
}

// MARK: Signed and encrypted mail

extension MailModel {
    /// Opens the open conversation's protected messages with gpg or
    /// gpgsm, then draws the page again with what they decrypted. A
    /// message is opened once a run; later reads answer from memory.
    func loadProtection() {
        guard let mail, let key = openThread else {
            protectionMarks = []
            return
        }
        Task.detached {
            let marks = (try? mail.protectionMarks(accountId: key.account, threadId: key.thread)) ?? []
            await MainActor.run {
                guard self.openThread == key else { return }
                let fresh = self.protectionMarks.map(\.messageId) != marks.map(\.messageId)
                self.protectionMarks = marks
                self.protectionKey = key
                // A message just opened may have arrived encrypted, so the
                // page is drawn again from what the engine opened.
                if fresh, !marks.isEmpty { self.loadConversation() }
            }
        }
    }
}

// MARK: Translation

extension MailModel {
    /// Reads whether the open conversation's message reads as another
    /// language, or has a translation already.
    func loadTranslationCard() {
        guard let mail, let key = openThread else {
            translationCard = nil
            return
        }
        let interface = appLocale.identifier
        Task.detached {
            let found = try? mail.translationCard(accountId: key.account, threadId: key.thread, interface: interface)
            await MainActor.run {
                if self.openThread == key, !self.translating {
                    self.translationCard = found ?? nil
                    self.translationKey = key
                }
            }
        }
    }

    /// The card's button: translates the message, or turns over the
    /// translation it has, and draws the page again.
    func translateOpen() {
        guard let mail, let key = openThread, !translating else { return }
        let interface = appLocale.identifier
        let onDevice = translationCard?.onDevice ?? false
        translating = true
        if let card = translationCard, card.button == tr("Translate") {
            translationCard = TranslationOffer(title: card.title, detail: tr("Translating…"), button: card.button, problem: false, onDevice: card.onDevice)
        }
        Task.detached {
            // A translation already made turns over without asking anyone.
            if let turned = try? mail.turnTranslation(accountId: key.account, threadId: key.thread) {
                await MainActor.run { self.translated(.success(turned), for: key) }
                return
            }
            if onDevice {
                let job = Result { try mail.translationJob(accountId: key.account, threadId: key.thread, interface: interface) }
                await MainActor.run {
                    switch job {
                    case .success(let job): self.appleJob = AppleTranslationJob(key: key, job: job)
                    case .failure(let error): self.translated(.failure(error), for: key)
                    }
                }
                return
            }
            let done = Result { try mail.translateMessage(accountId: key.account, threadId: key.thread, interface: interface) }
            await MainActor.run { self.translated(done, for: key) }
        }
    }

    /// The Mac's translator answered `job` with `pieces`, one for each of
    /// its pieces, `nil` for one it could not translate.
    func appleTranslated(_ job: AppleTranslationJob, pieces: [String?]) {
        guard let mail else { return }
        appleJob = nil
        let interface = appLocale.identifier
        let key = job.key
        Task.detached {
            let done = Result { try mail.applyTranslation(accountId: key.account, threadId: key.thread, interface: interface, translated: pieces) }
            await MainActor.run { self.translated(done, for: key) }
        }
    }

    /// The Mac's translator could not translate `job`.
    func appleFailed(_ job: AppleTranslationJob, _ error: Error) {
        appleJob = nil
        translated(.failure(CoreError.Store(tr("This Mac cannot translate this message: {reason}")
            .replacingOccurrences(of: "{reason}", with: error.localizedDescription))), for: job.key)
    }

    /// Puts what a translation came to on the card, and draws the page
    /// again with it.
    private func translated(_ done: Result<TranslationOffer, Error>, for key: ThreadKey) {
        translating = false
        guard openThread == key else { return }
        switch done {
        case .success(let card):
            translationCard = card
            loadConversation()
        case .failure(let error):
            let reason = "\(error)"
            if let card = translationCard {
                translationCard = TranslationOffer(title: card.title, detail: reason, button: card.button, problem: true, onDevice: card.onDevice)
            }
            say(reason)
        }
    }
}

/// What the Mac's own translator is asked to do for the open conversation.
struct AppleTranslationJob: Equatable {
    let id = UUID()
    let key: ThreadKey
    let job: TranslationJob
}

// MARK: Contacts

extension MailModel {
    /// Opens the card for the person at `address`.
    func showContact(_ address: String) {
        guard let mail else { return }
        Task.detached {
            let found = Result { try mail.contactCard(address: address, name: "") }
            await MainActor.run {
                switch found {
                case .success(let card): self.contactCard = card
                case .failure(let error): self.say("\(error)")
                }
            }
        }
    }

    /// Adds `email` to the VIPs, or takes it off, and says which.
    func toggleVip(email: String, name: String) {
        guard let mail else { return }
        do {
            say(try mail.toggleVip(email: email, name: name))
            refresh()
        } catch {
            say("\(error)")
        }
    }

    /// Searches every account for mail from `address`.
    func searchFrom(_ address: String) {
        space = .mail
        searchOpen = true
        searchText = "from:\(address)"
        searchNow()
    }
}

// MARK: Mailing lists

extension MailModel {
    /// Reads whether the open conversation's list can be left.
    func loadListOffer() {
        guard let mail, let key = openThread else {
            listOffer = nil
            return
        }
        Task.detached {
            let found = try? mail.listOffer(accountId: key.account, threadId: key.thread)
            await MainActor.run {
                if self.openThread == key {
                    self.listOffer = found ?? nil
                    self.listOfferKey = key
                }
            }
        }
    }

    /// Leaves the open conversation's list, once the person said yes,
    /// and says how it went. A page the list needs finished opens in the
    /// browser.
    func unsubscribe() {
        guard let mail, let key = openThread else { return }
        Task.detached {
            let done = Result { try mail.unsubscribe(accountId: key.account, threadId: key.thread) }
            await MainActor.run {
                switch done {
                case .success(let done):
                    self.say(done.words)
                    if !done.openUrl.isEmpty, let url = URL(string: done.openUrl) {
                        NSWorkspace.shared.open(url)
                    }
                    if done.left, self.openThread == key { self.listOffer = nil }
                case .failure(let error):
                    self.say("\(error)")
                }
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
        refreshTheme()
    }

    /// Draws pages in the app's own appearance rather than the Mac's, so a
    /// light Style on a dark Mac gives a light page. SwiftUI's color scheme
    /// can still read the system's when the conversation first appears,
    /// before Style has been applied.
    func refreshTheme() {
        let dark = NSApp.effectiveAppearance.bestMatch(from: [.aqua, .darkAqua]) == .darkAqua
        theme = PageTheme.current(dark: dark)
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

    /// Opens what a link in the assistant's answer names: a conversation in
    /// the mail, an event on the calendar, a page in the browser, or says
    /// why there is nothing to open.
    func openAssistantLink(_ url: URL) {
        guard let mail else { return }
        let text = url.absoluteString
        Task.detached {
            let goal = mail.assistantLink(url: text)
            await MainActor.run {
                switch goal {
                case .conversation(let account, let thread):
                    self.space = .mail
                    self.show(account: account, thread: thread)
                case .event(let spot):
                    self.showSpot(spot)
                case .web(let link):
                    if let link = URL(string: link) { NSWorkspace.shared.open(link) }
                case .nowhere(let words):
                    self.say(words)
                }
            }
        }
    }
}

/// What a toast says, and whether it offers Undo.
struct Toast: Equatable {
    let id = UUID()
    let words: String
    let undo: Bool
    /// Set while the toast stands for a message waiting to go.
    var undoSend: UUID? = nil
    /// The question after sending, with Save, about the recipients the
    /// address book lacks.
    var offer: SaveOffer? = nil
}

/// A conversation, by the account it belongs to and its id.
struct ThreadKey: Hashable, Sendable, Codable {
    let account: Int64
    let thread: String
}

/// Hears the Rust core's news on its own threads and hands it on.
final class Listener: MailListener, @unchecked Sendable {
    private let heard: @Sendable () -> Void
    private let news: @Sendable ([MailNotice]) -> Void

    init(_ heard: @escaping @Sendable () -> Void, news: @escaping @Sendable ([MailNotice]) -> Void) {
        self.heard = heard
        self.news = news
    }

    func changed(what: String) {
        heard()
    }

    func newMail(notices: [MailNotice]) {
        news(notices)
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

// MARK: Search

extension MailModel {
    /// The field changed. Clearing it puts the mailbox back; otherwise
    /// the search runs once the typing pauses. The mail on this Mac
    /// answers from the first letter; the servers hear it from three on,
    /// since fewer match nearly everything and each costs a request. The
    /// pause and the floor are the GTK app's
    /// (`mailrs_appcore::search::typing`).
    fileprivate func typed() {
        pause?.cancel()
        let text = searchText.trimmingCharacters(in: .whitespacesAndNewlines)
        suggest(searchText)
        guard !text.isEmpty else {
            searching = nil
            return
        }
        pause = Task {
            try? await Task.sleep(for: .milliseconds(300))
            guard !Task.isCancelled else { return }
            run(text, servers: text.count >= 3)
        }
    }

    /// Hides the suggestions until the text changes again.
    func dismissSuggestions() {
        suggestions = []
    }

    /// Return in the field: search the servers too, at once, however
    /// short the text.
    func searchNow() {
        pause?.cancel()
        let text = searchText.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty else {
            searching = nil
            return
        }
        run(text, servers: true)
    }

    private func run(_ text: String, servers: Bool) {
        searchServers = servers
        if searching == text { loadList() } else { searching = text }
    }

    /// Reads the suggestions for `text` off the main thread: the first
    /// time, the store's people are read.
    private func suggest(_ text: String) {
        guard let mail else { return }
        Task.detached {
            let found = mail.searchSuggestions(text: text)
            await MainActor.run {
                if self.searchText == text { self.suggestions = found }
            }
        }
    }
}

// MARK: Accounts

extension MailModel {
    /// A fresh walk through Add Account.
    func accountSetup() -> AccountSetup? { mail?.accountSetup() }

    /// A new account is running: read the sidebar and the welcome page's
    /// question again.
    func accountAdded() {
        readAccounts()
    }

    /// Shows the inbox of the account `id` once the sidebar lists it.
    func openInbox(of id: Int64) {
        guard let mail else { return }
        Task.detached {
            let sidebar = (try? mail.sidebar()) ?? []
            await MainActor.run {
                self.sidebar = sidebar
                self.noAccounts = false
                if let inbox = sidebar.first(where: { $0.accountId == id && $0.icon == "inbox" }) {
                    self.selectedMailbox = inbox.key
                }
            }
        }
    }

    func removeAccountQuestion(_ id: Int64) -> RemoveQuestion? {
        try? mail?.removeAccountQuestion(accountId: id)
    }

    /// Removes the account, off the main thread, and says how it went.
    func removeAccount(_ question: RemoveQuestion, id: Int64) {
        guard let mail else { return }
        if openThread?.account == id { selection = [] }
        Task.detached {
            do {
                try mail.removeAccount(accountId: id)
                await MainActor.run {
                    self.toast = Toast(words: tr("Removed {account}").replacingOccurrences(of: "{account}", with: question.email), undo: false)
                    self.readAccounts()
                }
            } catch {
                await MainActor.run {
                    self.toast = Toast(words: tr("Could not remove {account}: {reason}")
                        .replacingOccurrences(of: "{account}", with: question.email)
                        .replacingOccurrences(of: "{reason}", with: "\(error)"), undo: false)
                }
            }
        }
    }

    // MARK: The account menu

    /// The account menu's state for one account.
    func accountLook(_ id: Int64) -> AccountLook? { try? mail?.accountLook(accountId: id) }

    /// The colours the account menu offers, read once.
    var accountPalette: [AccountColorChoice] {
        if palette.isEmpty, let mail { palette = mail.accountPalette() }
        return palette
    }

    func renameAccount(_ id: Int64, to name: String) {
        changeAccount { try $0.renameAccount(accountId: id, name: name) }
    }

    func colorAccount(_ id: Int64, index: Int) {
        changeAccount { try $0.colorAccount(accountId: id, index: UInt32(index)) }
    }

    func uncolorAccount(_ id: Int64) {
        changeAccount { try $0.uncolorAccount(accountId: id) }
    }

    func moveAccount(_ id: Int64, step: Int32) {
        changeAccount { try $0.moveAccount(accountId: id, step: step) }
    }

    /// Runs one account menu change, then reads the sidebar and the list
    /// again at once for the new name, colour or place.
    private func changeAccount(_ change: (Mail) throws -> Void) {
        guard let mail else { return }
        do {
            try change(mail)
            refresh()
        } catch {
            toast = Toast(words: "\(error)", undo: false)
        }
    }

    /// An account signed in again: says so, and reads the sidebar for its
    /// state.
    func signedInAgain(_ email: String) {
        toast = Toast(words: tr("{account} is signed in again.").replacingOccurrences(of: "{account}", with: email), undo: false)
        refresh()
    }

    /// Reads whether the store has accounts, and the sidebar, and moves
    /// off a mailbox that left it.
    func readAccounts() {
        guard let mail else { return }
        Task.detached {
            let none = mail.hasNoAccounts()
            let sidebar = (try? mail.sidebar()) ?? []
            await MainActor.run {
                self.noAccounts = none
                self.sidebar = sidebar
                if self.selectedMailbox == nil || !sidebar.contains(where: { $0.key == self.selectedMailbox }) {
                    self.selectedMailbox = sidebar.first { $0.kind == "mailbox" }?.key
                }
            }
        }
    }
}

/// Delete Forever asked about `threads`, with the question's words.
struct EraseRequest: Identifiable {
    let id = UUID()
    let threads: [ThreadRef]
    let question: EraseQuestion
    /// Whether the selection goes once the mail does.
    let clears: Bool
}
