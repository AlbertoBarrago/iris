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
            if let listing = model.listing, !listing.categories.isEmpty {
                CategoryBar(tabs: listing.categories, selected: $model.category)
                    .padding(.horizontal, 12)
                    .padding(.vertical, 8)
            }
            // Rows are known by their conversation, the same key the
            // selection holds, so a click selects the row under it.
            List(model.listing?.rows ?? [], id: \.key, selection: $model.selection) { row in
                let key = row.key
                ThreadRowView(row: row)
                    .swipeActions(edge: .leading) {
                        Button(row.unread ? tr("Mark as Read") : tr("Mark as Unread"), systemImage: row.unread ? "envelope.open" : "envelope.badge") {
                            model.perform(row.unread ? .markRead : .markUnread, on: [key])
                        }
                        .tint(.blue)
                    }
                    .swipeActions(edge: .trailing) {
                        Button(tr("Delete"), systemImage: "trash", role: .destructive) { model.perform(.trash, on: [key]) }
                        Button(tr("Archive"), systemImage: "archivebox") { model.perform(.archive, on: [key]) }
                            .tint(.purple)
                    }
            }
            .listStyle(.inset)
            .contextMenu(forSelectionType: ThreadKey.self) { keys in
                MailActionsMenu(keys: keys)
            }
            .onDeleteCommand { model.perform(.trash) }
            .onExitCommand { model.selection = [] }
            // The GTK app's one-key shortcuts while the list has the keys.
            .onKeyPress(characters: ["e", "u", "M"]) { press in
                switch press.characters {
                case "e": model.perform(.archive)
                case "u": model.toggleRead()
                case "M": model.perform(.mute)
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
                Button(tr("Search"), systemImage: "magnifyingglass") {}
                Button(tr("New Message"), systemImage: "square.and.pencil") {
                    model.composeAsked = ComposeRequest(kind: .new)
                }
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

    var body: some View {
        HStack(alignment: .top, spacing: 10) {
            Circle()
                .fill(row.unread ? Color.accentColor : .clear)
                .frame(width: 8, height: 8)
                .padding(.top, 16)
            ZStack {
                Circle().fill(Color(hex: row.avatarColor) ?? .gray)
                Text(row.initials).font(.callout.weight(.semibold)).foregroundStyle(.white)
            }
            .frame(width: 38, height: 38)
            VStack(alignment: .leading, spacing: 2) {
                HStack(spacing: 5) {
                    Circle().fill(Color(hex: row.accountColor) ?? .accentColor).frame(width: 7, height: 7)
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
                        Text("\(row.messageCount)")
                            .font(.caption.weight(.semibold))
                            .padding(.horizontal, 6)
                            .padding(.vertical, 1)
                            .background(Capsule().fill(Color.secondary.opacity(0.2)))
                    }
                }
                Text(row.snippet).font(.callout).foregroundStyle(.secondary).lineLimit(2)
            }
        }
        .padding(.vertical, 6)
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
        Button(tr("Delete"), systemImage: "trash") { model.perform(.trash, on: keys) }
        Button(tr("Junk"), systemImage: "xmark.bin") { model.perform(.junk, on: keys) }
        Divider()
        Button(tr("Mark as Read"), systemImage: "envelope.open") { model.perform(.markRead, on: keys) }
        Button(tr("Mark as Unread"), systemImage: "envelope.badge") { model.perform(.markUnread, on: keys) }
        Divider()
        FlagMenu(keys: keys)
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
        Menu(tr("Flag"), systemImage: "flag") {
            ForEach(Self.colors, id: \.0) { key, name, hex in
                Button {
                    model.perform(.flag(color: key), on: keys)
                } label: {
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
            Button {
                model.dismissToast()
            } label: {
                Image(systemName: "xmark")
            }
            .buttonStyle(.borderless)
        }
        .padding(.horizontal, 18)
        .padding(.vertical, 10)
        .background(.regularMaterial, in: Capsule())
        .shadow(radius: 8, y: 2)
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
}
