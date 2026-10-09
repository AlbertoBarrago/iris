import SwiftUI

/// The sidebar the Rust core lays out: the mailboxes for every account
/// together under their headings, then each account with its own. Every
/// heading and every account folds away, as a Mac sidebar's sections do,
/// and stays folded the next time.
struct SidebarView: View {
    @Environment(MailModel.self) private var model
    /// The keys of the sections and accounts folded away, one per line.
    @AppStorage("sidebar.folded") private var folded = ""

    var body: some View {
        @Bindable var model = model
        List(selection: $model.selectedMailbox) {
            ForEach(SidebarSection.group(model.sidebar)) { section in
                Section(isExpanded: open(section.key)) {
                    ForEach(section.rows, id: \.key) { item in
                        MailboxRow(item: item).tag(item.key)
                    }
                    ForEach(section.accounts) { account in
                        DisclosureGroup(isExpanded: open(account.line.key)) {
                            ForEach(account.rows, id: \.key) { item in
                                MailboxRow(item: item).tag(item.key)
                            }
                        } label: {
                            AccountLine(item: account.line, isOpen: open(account.line.key))
                        }
                    }
                } header: {
                    Text(section.title)
                }
            }
        }
        .listStyle(.sidebar)
    }

    /// Whether `key` is open, and a way to fold or unfold it.
    private func open(_ key: String) -> Binding<Bool> {
        Binding {
            !folded.split(separator: "\n").contains(Substring(key))
        } set: { isOpen in
            var keys = Set(folded.split(separator: "\n").map(String.init))
            if isOpen { keys.remove(key) } else { keys.insert(key) }
            folded = keys.sorted().joined(separator: "\n")
        }
    }
}

/// One heading of the sidebar with what sits under it.
struct SidebarSection: Identifiable {
    let key: String
    let title: String
    var rows: [SidebarItem] = []
    var accounts: [AccountGroup] = []
    var id: String { key }

    /// The core's flat list, read into headings, accounts and their rows.
    static func group(_ items: [SidebarItem]) -> [SidebarSection] {
        var sections: [SidebarSection] = []
        for item in items {
            switch item.kind {
            case "heading":
                sections.append(SidebarSection(key: "section-\(item.title)", title: item.title))
            case "account":
                sections[sections.count - 1].accounts.append(AccountGroup(line: item))
            default:
                guard !sections.isEmpty else { continue }
                if sections[sections.count - 1].accounts.isEmpty {
                    sections[sections.count - 1].rows.append(item)
                } else {
                    let last = sections[sections.count - 1].accounts.count - 1
                    sections[sections.count - 1].accounts[last].rows.append(item)
                }
            }
        }
        return sections
    }
}

/// An account's line and its own mailboxes.
struct AccountGroup: Identifiable {
    let line: SidebarItem
    var rows: [SidebarItem] = []
    var id: String { line.key }
}

/// An account's line: its color, its address, how its sync is doing, and
/// the account's own actions on a right click.
struct AccountLine: View {
    @Environment(MailModel.self) private var model
    let item: SidebarItem
    /// Whether the account's mailboxes show; a click on the line folds
    /// or unfolds them, and does nothing else.
    @Binding var isOpen: Bool
    @State private var hovering = false

    var body: some View {
        HStack(spacing: 6) {
            Circle().fill(Color(hex: item.accountColor) ?? .accentColor).frame(width: 8, height: 8)
            Text(item.title).lineLimit(1)
            Spacer(minLength: 4)
            if let (symbol, words) = state {
                Image(systemName: symbol).foregroundStyle(.secondary).help(words)
            }
            // The account's menu, offered under the pointer as the GTK
            // sidebar's options button was; a right click opens it too.
            Menu {
                menu
            } label: {
                Image(systemName: "ellipsis.circle")
            }
            .menuStyle(.borderlessButton)
            .menuIndicator(.hidden)
            .fixedSize()
            .opacity(hovering ? 1 : 0)
            .help(tr("Account options"))
        }
        .contentShape(Rectangle())
        .onTapGesture { isOpen.toggle() }
        .onHover { hovering = $0 }
        .help(item.title)
        .contextMenu { menu }
    }

    @ViewBuilder private var menu: some View {
            Button(tr("Check for Mail")) {
                if let id = item.accountId { model.checkAccount(id) }
            }
            Divider()
            // These come with Preferences in this front end.
            Group {
                Button(tr("Automatic Reply…")) {}
                Button(tr("Signature…")) {}
                Button(tr("Rules…")) {}
                Button(tr("Hide My Email…")) {}
                Divider()
                Button(tr("Rename…")) {}
                Button(tr("Move Up")) {}
                Button(tr("Move Down")) {}
                Divider()
                Button(tr("Sign In Again…")) {}
                Divider()
                Button(tr("Remove Account…")) {}
            }
            .disabled(true)
    }

    /// The mark beside an account whose sync needs attention.
    private var state: (String, String)? {
        switch item.state {
        case "needs_reauth": ("exclamationmark.triangle", tr("Sign in again to keep syncing"))
        case "offline": ("wifi.slash", tr("Offline"))
        case "backing_off", "waiting_for_keyring": ("clock.arrow.circlepath", item.state)
        case "bootstrapping": ("arrow.triangle.2.circlepath", item.state)
        default: nil
        }
    }
}

struct MailboxRow: View {
    let item: SidebarItem

    var body: some View {
        HStack {
            Label {
                Text(item.title).lineLimit(1)
            } icon: {
                Image(systemName: Self.symbol(item.icon))
                    .foregroundStyle(Color(hex: item.color) ?? .accentColor)
            }
            Spacer()
            if item.count > 0 {
                Text("\(item.count)")
                    .font(.callout.monospacedDigit())
                    .foregroundStyle(.secondary)
            }
        }
        // An account's group indents its rows already; only what nests
        // deeper, or a flag color under Flagged, steps in further.
        .padding(.leading, CGFloat(item.accountId == nil ? Int(item.depth) : max(Int(item.depth) - 1, 0)) * 14)
    }

    /// The SF Symbol for what a row's icon stands for.
    static func symbol(_ icon: String) -> String {
        switch icon {
        case "inbox": "tray"
        case "flag": "flag.fill"
        case "sent": "paperplane"
        case "drafts": "pencil"
        case "muted": "speaker.slash"
        case "outbox": "tray.and.arrow.up"
        case "send-later": "clock.arrow.circlepath"
        case "remind-me": "alarm"
        case "follow-up": "arrow.uturn.left.circle"
        case "archive": "archivebox"
        case "junk": "xmark.bin"
        case "trash": "trash"
        case "all-mail": "envelope.open"
        case "group": "folder"
        case "tag": "tag"
        default: "folder"
        }
    }
}

extension Color {
    /// A color from `#rrggbb`, or nil.
    init?(hex: String?) {
        guard var text = hex?.trimmingCharacters(in: .whitespaces), text.hasPrefix("#"), text.count == 7 else {
            return nil
        }
        text.removeFirst()
        guard let value = UInt32(text, radix: 16) else { return nil }
        self.init(
            red: Double((value >> 16) & 0xff) / 255,
            green: Double((value >> 8) & 0xff) / 255,
            blue: Double(value & 0xff) / 255
        )
    }
}
