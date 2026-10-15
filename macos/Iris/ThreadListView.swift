import AppKit
import SwiftUI

/// The conversations of the mailbox picked: its title and unread count,
/// the category tabs over an inbox, and one row per conversation.
struct ThreadListView: View {
    @Environment(MailModel.self) private var model

    var body: some View {
        @Bindable var model = model
        VStack(spacing: 0) {
            if let reason = model.notSyncing {
                Label(reason, systemImage: "pause.circle")
                    .font(.callout)
                    .padding(10)
                    .frame(maxWidth: .infinity)
                    .background(.yellow.opacity(0.15))
            }
            if model.searchOpen {
                MailSearchField()
                    .padding(.horizontal, 12)
                    .padding(.vertical, 8)
                Divider()
            }
            if let listing = model.listing, !listing.categories.isEmpty {
                CategoryBar(tabs: listing.categories, selected: $model.category)
                    .padding(.horizontal, 12)
                    .padding(.vertical, 8)
            }
            // Rows are known by their conversation, the same key the
            // selection holds, so a click selects the row under it.
            List(model.listing?.rows ?? [], id: \.key, selection: $model.selection) { row in
                let key = row.key
                VStack(alignment: .leading, spacing: 0) {
                    ThreadRowView(row: row, expanded: model.expandedThreads.contains(key)) {
                        if !model.expandedThreads.insert(key).inserted { model.expandedThreads.remove(key) }
                    }
                    if model.expandedThreads.contains(key) {
                        ConversationSummaryView(row: row)
                    }
                }
                    // A drag from a picked row carries the whole selection,
                    // as Mail's list does; from any other row, that row.
                    .draggable(DraggedThreads.encode(model.selection.contains(key) ? Array(model.selection) : [key]))
                    .swipeActions(edge: .leading) {
                        Button(row.unread ? tr("Mark as Read") : tr("Mark as Unread"), systemImage: row.unread ? "envelope.open" : "envelope.badge") {
                            model.perform(row.unread ? .markRead : .markUnread, on: [key])
                        }
                        .tint(.blue)
                    }
                    .swipeActions(edge: .trailing) {
                        Button(model.deleteWords, systemImage: "trash", role: .destructive) { model.perform(.trash, on: [key]) }
                        Button(tr("Archive"), systemImage: "archivebox") { model.perform(.archive, on: [key]) }
                            .tint(.purple)
                    }
            }
            .listStyle(.inset)
            .contextMenu(forSelectionType: ThreadKey.self) { keys in
                MailActionsMenu(keys: keys)
            } primaryAction: { keys in
                // A double click or Return opens the conversation in a
                // window of its own, as in the GTK app.
                for key in keys { model.openInWindow(key) }
            }
            // Handle backward Delete as well as the system's forward Delete.
            .onKeyPress(characters: CharacterSet(charactersIn: "\u{7f}\u{8}\u{f728}"), phases: .down) { press in
                guard press.modifiers.intersection([.command, .control, .option, .shift]).isEmpty,
                      !model.selection.isEmpty else { return .ignored }
                model.perform(.trash)
                return .handled
            }
            .onExitCommand { model.selection = [] }
            // The GTK app's one-key shortcuts while the list has the keys.
            .onKeyPress(characters: ["e", "u", "M", "l"]) { press in
                switch press.characters {
                case "e": model.perform(.archive)
                case "u": model.toggleRead()
                case "M": model.perform(.mute)
                case "l" where !model.selection.isEmpty: model.filingShown = true
                default: return .ignored
                }
                return .handled
            }
            .overlay {
                if let listing = model.listing, listing.rows.isEmpty {
                    ContentUnavailableView(listing.empty, systemImage: "tray")
                }
            }
        }
        .navigationTitle(model.listing?.title ?? "")
        .navigationSubtitle(model.listing?.subtitle ?? "")
        .toolbar {
            ToolbarItemGroup {
                Button(tr("Check for Mail"), systemImage: "arrow.clockwise") { model.checkNow() }
                    .help(tr("Check for Mail"))
                // Mail's message filter, for unread mail alone.
                Toggle(isOn: $model.unreadOnly) {
                    Label(
                        tr("Show Only Unread"),
                        systemImage: model.unreadOnly
                            ? "line.3.horizontal.decrease.circle.fill" : "line.3.horizontal.decrease.circle"
                    )
                }
                .help(tr("Show Only Unread"))
                .disabled(model.listing?.filtersUnread != true)
                // As in the GTK app: the magnifier opens a search bar over
                // the list. ⌘F opens it too, from the Edit menu, unless the
                // conversation has the keys.
                Toggle(isOn: $model.searchOpen) {
                    Label(tr("Search"), systemImage: "magnifyingglass")
                }
                .help(tr("Search mail"))
                Button(tr("New Message"), systemImage: "square.and.pencil") {
                    model.composeAsked = ComposeRequest(kind: .new)
                }
                .help(tr("New Message"))
            }
        }
    }
}

/// Tutti, Principale, Aggiornamenti, Promozioni, Social: pills that wrap,
/// each with its unread count.
struct CategoryBar: View {
    let tabs: [CategoryTab]
    @Binding var selected: String?

    var body: some View {
        FlowLayout(spacing: 8) {
            ForEach(tabs, id: \.key) { tab in
                let isOn = (selected ?? "all") == tab.key
                Button {
                    selected = tab.key == "all" ? nil : tab.key
                } label: {
                    HStack(spacing: 6) {
                        Text(tab.name).fontWeight(.semibold)
                        if tab.unread > 0 {
                            Text("\(tab.unread)")
                                .font(.caption2.weight(.bold))
                                .padding(.horizontal, 5)
                                .padding(.vertical, 1)
                                .background(Capsule().fill(isOn ? Color.white.opacity(0.3) : Color.accentColor))
                                .foregroundStyle(.white)
                        }
                    }
                    .padding(.horizontal, 14)
                    .padding(.vertical, 7)
                    .background(Capsule().fill(isOn ? Color.accentColor : Color.secondary.opacity(0.15)))
                    .foregroundStyle(isOn ? .white : .primary)
                }
                .buttonStyle(.plain)
            }
        }
    }
}

/// One conversation: who wrote last, when, the subject and the first words.
struct ThreadRowView: View {
    let row: ListRow
    var expanded = false
    var toggle: () -> Void = {}

    var body: some View {
        HStack(alignment: .top, spacing: 10) {
            Circle()
                .fill(row.unread ? Color.accentColor : .clear)
                .frame(width: 8, height: 8)
                .padding(.top, 16)
            ZStack {
                if let photo = ContactPhotos.image(row.photo) {
                    Image(nsImage: photo).resizable().scaledToFill()
                } else {
                    Circle().fill(Color(hex: row.avatarColor) ?? .gray)
                    Text(row.initials).font(.callout.weight(.semibold)).foregroundStyle(.white)
                }
            }
            .frame(width: 38, height: 38)
            .clipShape(Circle())
            VStack(alignment: .leading, spacing: 2) {
                HStack(spacing: 5) {
                    if let color = Color(hex: row.accountColor) {
                        Circle().fill(color).frame(width: 7, height: 7)
                    }
                    Text(row.sender).fontWeight(row.unread ? .bold : .semibold).lineLimit(1)
                    if row.sender != row.senderEmail, !row.senderEmail.isEmpty {
                        Text(row.senderEmail).foregroundStyle(.secondary).lineLimit(1).truncationMode(.tail)
                    }
                    Spacer(minLength: 4)
                    if row.hasAttachments {
                        Image(systemName: "paperclip").foregroundStyle(.secondary)
                    }
                    if let flag = row.flag {
                        FlagImage.colored(flag)
                    }
                    Text(row.date)
                        .font(.callout.monospacedDigit())
                        .foregroundStyle(row.unread ? Color.accentColor : .secondary)
                        .fontWeight(row.unread ? .semibold : .regular)
                }
                HStack {
                    Text(row.subject).fontWeight(row.unread ? .semibold : .regular).lineLimit(1)
                    Spacer(minLength: 4)
                    if row.messageCount > 1 {
                        Button(action: toggle) {
                            HStack(spacing: 3) {
                                Image(systemName: expanded ? "chevron.down" : "chevron.right")
                                    .font(.system(size: 8, weight: .bold))
                                Text("\(row.messageCount)").font(.caption.weight(.semibold))
                            }
                            .padding(.horizontal, 6)
                            .padding(.vertical, 2)
                            .background(Capsule().fill(Color.secondary.opacity(0.2)))
                        }
                        .buttonStyle(.borderless)
                        .help(expanded ? tr("Hide messages") : tr("Show messages"))
                        .accessibilityLabel(expanded ? tr("Hide messages") : tr("Show messages"))
                        .accessibilityValue(expanded ? tr("Expanded") : tr("Collapsed"))
                    }
                }
                Text(row.snippet).font(.callout).foregroundStyle(.secondary).lineLimit(2)
            }
        }
        .padding(.vertical, 6)
    }
}

/// The metadata already on disk, loaded only while the conversation is
/// expanded. Its buttons open mail; the count button only reveals rows.
struct ConversationSummaryView: View {
    @Environment(MailModel.self) private var model
    let row: ListRow
    @State private var summaries: [MessageSummary] = []
    @State private var loading = true
    @State private var failure: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            if loading && summaries.isEmpty {
                ProgressView(tr("Loading messages")).controlSize(.small).padding(8)
            }
            ForEach(summaries, id: \.id) { message in
                Button { model.openMessage(message.id, in: row.key) } label: {
                    HStack(alignment: .top, spacing: 8) {
                        Circle().fill(message.unread ? Color.accentColor : .clear)
                            .frame(width: 5, height: 5).padding(.top, 5)
                            .accessibilityHidden(true)
                        VStack(alignment: .leading, spacing: 3) {
                            HStack {
                                Text(message.sender)
                                    .fontWeight(message.unread ? .semibold : .regular)
                                    .lineLimit(1)
                                Spacer(minLength: 8)
                                Text(message.date).foregroundStyle(.secondary)
                            }
                            Text(message.snippet).foregroundStyle(.secondary).lineLimit(1)
                        }
                    }
                    .font(.callout)
                    .foregroundStyle(.primary)
                    .padding(.vertical, 7)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .contentShape(Rectangle())
                }
                .buttonStyle(.borderless)
                .accessibilityElement(children: .combine)
                Divider()
            }
            if !loading, summaries.isEmpty, failure == nil {
                Text(tr("No message previews are stored on this Mac yet."))
                    .font(.callout).foregroundStyle(.secondary).padding(.vertical, 8)
            }
            if let failure { Text(failure).font(.callout).foregroundStyle(.secondary).padding(.vertical, 8) }
        }
        .padding(.leading, 54)
        .padding(.bottom, 6)
        .task(id: row) {
            loading = true
            failure = nil
            do {
                let loaded = try await model.summaries(for: row.key)
                try Task.checkCancellation()
                summaries = loaded
                loading = false
            } catch {
                guard !Task.isCancelled else { return }
                failure = "\(error)"
                loading = false
            }
        }
    }
}

/// Lays its children out left to right, wrapping onto new lines.
struct FlowLayout: Layout {
    var spacing: CGFloat = 8

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        let width = proposal.width ?? .infinity
        var x: CGFloat = 0, y: CGFloat = 0, line: CGFloat = 0, widest: CGFloat = 0
        for view in subviews {
            let size = view.sizeThatFits(.unspecified)
            if x > 0, x + size.width > width {
                x = 0
                y += line + spacing
                line = 0
            }
            x += size.width + spacing
            widest = max(widest, x - spacing)
            line = max(line, size.height)
        }
        return CGSize(width: min(widest, width), height: y + line)
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        var x = bounds.minX, y = bounds.minY, line: CGFloat = 0
        for view in subviews {
            let size = view.sizeThatFits(.unspecified)
            if x > bounds.minX, x + size.width > bounds.maxX {
                x = bounds.minX
                y += line + spacing
                line = 0
            }
            view.place(at: CGPoint(x: x, y: y), proposal: ProposedViewSize(size))
            x += size.width + spacing
            line = max(line, size.height)
        }
    }
}

/// The actions a right click offers on the conversations under it.
struct MailActionsMenu: View {
    @Environment(MailModel.self) private var model
    let keys: Set<ThreadKey>

    var body: some View {
        Button(tr("Archive"), systemImage: "archivebox") { model.perform(.archive, on: keys) }
        Button(model.deleteWords, systemImage: "trash") { model.perform(.trash, on: keys) }
        Button(tr("Junk"), systemImage: "xmark.bin") { model.perform(.junk, on: keys) }
        Divider()
        Button(tr("Mark as Read"), systemImage: "envelope.open") { model.perform(.markRead, on: keys) }
        Button(tr("Mark as Unread"), systemImage: "envelope.badge") { model.perform(.markUnread, on: keys) }
        Divider()
        FlagMenu(keys: keys)
        FilingSubmenu(keys: keys)
        RemindMenu(keys: keys)
        Button(tr("Mute"), systemImage: "speaker.slash") { model.perform(.mute, on: keys) }
    }
}

/// Flag with a color, or take the flag off.
struct FlagMenu: View {
    @Environment(MailModel.self) private var model
    var keys: Set<ThreadKey>? = nil

    static let colors: [(String, String, String)] = [
        ("red", tr("Red"), "#ff3b30"), ("orange", tr("Orange"), "#ff9500"), ("yellow", tr("Yellow"), "#ffcc00"),
        ("green", tr("Green"), "#34c759"), ("blue", tr("Blue"), "#007aff"), ("purple", tr("Purple"), "#af52de"),
        ("gray", tr("Gray"), "#8e8e93"),
    ]

    var body: some View {
        // The flag the mail already wears shows ticked.
        let worn = model.sharedFlag(of: keys)
        Menu(tr("Flag"), systemImage: worn == nil ? "flag" : "flag.fill") {
            ForEach(Self.colors, id: \.0) { key, name, hex in
                Toggle(isOn: Binding(get: { worn == hex }, set: { _ in model.perform(.flag(color: key), on: keys) })) {
                    Label { Text(name) } icon: { FlagImage.colored(hex) }
                }
            }
            Divider()
            Button(tr("Clear Flag")) { model.perform(.flag(color: nil), on: keys) }
        }
    }
}

/// What an action did, and Undo, floating in the window's top right
/// corner, where a Mac shows its notifications.
struct ToastView: View {
    @Environment(MailModel.self) private var model
    let toast: Toast

    var body: some View {
        HStack(spacing: 14) {
            Text(toast.words).fontWeight(.semibold)
            if toast.undo {
                Button(tr("Undo")) {
                    if let key = toast.undoSend { model.undoSend(key) } else { model.undo(); model.dismissToast() }
                }
                    .buttonStyle(.borderless)
                    .fontWeight(.semibold)
            }
            if let offer = toast.offer {
                Button(tr("Save")) { model.saveOffered(offer) }
                    .buttonStyle(.borderless)
                    .fontWeight(.semibold)
            }
            Button {
                // Closing the question counts as a no, as in the GTK app;
                // letting it time out does not.
                if let offer = toast.offer { model.declineOffered(offer) }
                model.dismissToast()
            } label: {
                Image(systemName: "xmark")
                    .font(.caption.weight(.bold))
                    .foregroundStyle(.secondary)
                    .frame(width: 20, height: 20)
                    .background(.quaternary, in: Circle())
            }
            .buttonStyle(.plain)
            .help(tr("Close"))
            .accessibilityLabel(tr("Close"))
        }
        .padding(.horizontal, 18)
        .padding(.vertical, 10)
        .background(.regularMaterial, in: Capsule())
        .shadow(radius: 8, y: 2)
        .onHover { hovering = $0 }
        // Each toast goes by itself after a while, longer when it offers
        // Undo, and stays while the pointer rests on it. Undo Send keeps
        // its own clock: the toast goes once the message does.
        .task(id: TimerKey(toast: toast.id, hovering: hovering)) {
            guard toast.undoSend == nil, !hovering else { return }
            let base = Int(model.prefs?.noticeSeconds ?? 5)
            try? await Task.sleep(for: .seconds(toast.offer != nil ? base * 2 : toast.undo ? base + 3 : base))
            if !Task.isCancelled, model.toast?.id == toast.id { model.dismissToast() }
        }
    }

    @State private var hovering = false

    private struct TimerKey: Equatable {
        let toast: UUID
        let hovering: Bool
    }
}

extension ListRow {
    /// The conversation this row stands for.
    var key: ThreadKey { ThreadKey(account: accountId, thread: threadId) }
}

/// A flag in its own color. A Mac menu draws an SF Symbol as a template,
/// in one color, whatever color the view asks for, so the flag is drawn
/// into an image that keeps its color.
enum FlagImage {
    static func colored(_ hex: String) -> Image {
        let color = NSColor(Color(hex: hex) ?? .red)
        let configuration = NSImage.SymbolConfiguration(paletteColors: [color])
        guard let symbol = NSImage(systemSymbolName: "flag.fill", accessibilityDescription: nil)?
            .withSymbolConfiguration(configuration)
        else {
            return Image(systemName: "flag.fill")
        }
        symbol.isTemplate = false
        return Image(nsImage: symbol)
    }

    /// A dot in its own color, for a label color in a menu.
    static func dot(_ hex: String) -> Image {
        let color = NSColor(Color(hex: hex) ?? .gray)
        let configuration = NSImage.SymbolConfiguration(paletteColors: [color])
        guard let symbol = NSImage(systemSymbolName: "circle.fill", accessibilityDescription: nil)?
            .withSymbolConfiguration(configuration)
        else {
            return Image(systemName: "circle.fill")
        }
        symbol.isTemplate = false
        return Image(nsImage: symbol)
    }
}

/// The search bar over the list: AppKit's own field, with its clear
/// button. Typing searches after a pause, Return at once; Escape in an
/// empty field closes the bar. It takes the keys as it opens. The core's
/// suggestions show under it while it has them.
struct MailSearchField: View {
    @Environment(MailModel.self) private var model

    var body: some View {
        @Bindable var model = model
        SearchFieldBox(
            text: $model.searchText,
            suggestions: model.suggestions,
            placeholder: tr("Search mail")
        ) {
            model.searchNow()
            model.dismissSuggestions()
        } pick: { suggestion in
            model.searchText = suggestion.query
            model.searchNow()
            model.dismissSuggestions()
        } dismiss: {
            model.dismissSuggestions()
        } cancel: {
            model.searchOpen = false
        }
    }
}

/// An `NSSearchField` bound to `text`, which takes the keys once it is
/// in a window. `submit` runs on Return, `cancel` on Escape when the
/// field is already empty. Suggestions show in a panel under the field
/// that never takes the keys, so typing always stays in the field; the
/// arrow keys move through them and Return picks the one marked.
private struct SearchFieldBox: NSViewRepresentable {
    @Binding var text: String
    let suggestions: [SearchSuggestion]
    let placeholder: String
    let submit: () -> Void
    let pick: (SearchSuggestion) -> Void
    let dismiss: () -> Void
    let cancel: () -> Void

    func makeNSView(context: Context) -> NSSearchField {
        let field = NSSearchField()
        field.placeholderString = placeholder
        field.sendsSearchStringImmediately = true
        field.delegate = context.coordinator
        field.target = context.coordinator
        field.action = #selector(Coordinator.changed(_:))
        context.coordinator.field = field
        DispatchQueue.main.async { field.window?.makeFirstResponder(field) }
        return field
    }

    func updateNSView(_ field: NSSearchField, context: Context) {
        let coordinator = context.coordinator
        coordinator.parent = self
        // While the person types, the model can lag the field by a key.
        // Writing that older text back would move the cursor to the end
        // and drop what was typed since, so only a change made elsewhere,
        // such as a picked suggestion, is written into the field.
        if field.stringValue != text, text != coordinator.reported {
            field.stringValue = text
            coordinator.reported = text
            field.currentEditor()?.selectedRange = NSRange(location: (text as NSString).length, length: 0)
        }
        coordinator.showSuggestions()
    }

    static func dismantleNSView(_ field: NSSearchField, coordinator: Coordinator) {
        coordinator.panel.close()
    }

    func makeCoordinator() -> Coordinator { Coordinator(self) }

    @MainActor
    final class Coordinator: NSObject, NSSearchFieldDelegate {
        var parent: SearchFieldBox
        weak var field: NSSearchField?
        /// The text the field last reported, which the model may not
        /// have caught up with yet.
        var reported = ""
        private var editing = false
        let panel = SearchSuggestionPanel()

        init(_ parent: SearchFieldBox) {
            self.parent = parent
            super.init()
            panel.picked = { [weak self] suggestion in self?.parent.pick(suggestion) }
        }

        /// Shows the suggestions under the field while it is being typed
        /// in and has some, and hides the panel otherwise.
        func showSuggestions() {
            guard let field else { return }
            if editing, !parent.suggestions.isEmpty {
                panel.show(parent.suggestions, under: field)
            } else {
                panel.close()
            }
        }

        @objc func changed(_ field: NSSearchField) {
            reported = field.stringValue
            if parent.text != field.stringValue { parent.text = field.stringValue }
        }

        func controlTextDidBeginEditing(_ note: Notification) {
            editing = true
            showSuggestions()
        }

        func controlTextDidEndEditing(_ note: Notification) {
            editing = false
            showSuggestions()
        }

        func control(_ control: NSControl, textView: NSTextView, doCommandBy selector: Selector) -> Bool {
            switch selector {
            case #selector(NSResponder.moveDown(_:)) where panel.isShowing:
                panel.mark(by: 1)
                return true
            case #selector(NSResponder.moveUp(_:)) where panel.isShowing:
                panel.mark(by: -1)
                return true
            case #selector(NSResponder.insertNewline(_:)):
                if let suggestion = panel.marked {
                    parent.pick(suggestion)
                } else {
                    parent.submit()
                }
                return true
            case #selector(NSResponder.cancelOperation(_:)) where panel.isShowing:
                parent.dismiss()
                return true
            case #selector(NSResponder.cancelOperation(_:)) where parent.text.isEmpty:
                parent.cancel()
                return true
            default:
                return false
            }
        }
    }
}

/// Contact photos read from disk once per run, so a long list does not
/// read the same file for every row it draws.
@MainActor
enum ContactPhotos {
    private static var read: [String: NSImage] = [:]
    private static var missing: Set<String> = []

    static func image(_ path: String) -> NSImage? {
        guard !path.isEmpty, !missing.contains(path) else { return nil }
        if let found = read[path] { return found }
        guard let image = NSImage(contentsOfFile: path) else {
            missing.insert(path)
            return nil
        }
        read[path] = image
        return image
    }
}
