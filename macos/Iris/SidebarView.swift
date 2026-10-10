import AppKit
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
    @State private var removing: RemoveQuestion?
    @State private var renaming = false
    @State private var newName = ""
    @State private var renamingEmail = ""
    @State private var makingLabel = false
    @State private var newLabelName = ""
    /// The account menu's words, read when the menu is built.
    private var look: AccountLook? { item.accountId.flatMap { model.accountLook($0) } }

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
        .alert(removing?.title ?? "", isPresented: Binding(get: { removing != nil }, set: { if !$0 { removing = nil } })) {
            Button(tr("Cancel"), role: .cancel) {}
            Button(tr("Remove"), role: .destructive) {
                if let question = removing, let id = item.accountId { model.removeAccount(question, id: id) }
            }
        } message: {
            Text(removing?.body ?? "")
        }
        .alert(look?.newLabelHeading ?? tr("New Label"), isPresented: $makingLabel) {
            TextField(look?.newLabelPrompt ?? "", text: $newLabelName)
            Button(tr("Cancel"), role: .cancel) {}
            Button(tr("Create")) {
                if let id = item.accountId { model.newLabel(account: id, name: newLabelName) }
            }
            .keyboardShortcut(.defaultAction)
        }
        .alert(tr("Name This Account"), isPresented: $renaming) {
            TextField(renamingEmail, text: $newName)
            Button(tr("Cancel"), role: .cancel) {}
            Button(tr("Save")) {
                if let id = item.accountId { model.renameAccount(id, to: newName) }
            }
            .keyboardShortcut(.defaultAction)
        } message: {
            Text(tr("The sidebar shows the name instead of the address. Leave it empty for the address."))
        }
    }

    /// Where the account sits among the others in the sidebar.
    private var place: (first: Bool, last: Bool) {
        let accounts = model.sidebar.filter { $0.kind == "account" }
        return (accounts.first?.key == item.key, accounts.last?.key == item.key)
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
            }
            .disabled(true)
            Divider()
            Button(look?.newLabelItem ?? tr("New Label…")) {
                newLabelName = ""
                makingLabel = true
            }
            Button(tr("Rename…")) {
                let look = item.accountId.flatMap { model.accountLook($0) }
                newName = look?.name ?? ""
                renamingEmail = look?.email ?? item.title
                renaming = true
            }
            Menu(tr("Color")) {
                ForEach(Array(model.accountPalette.enumerated()), id: \.offset) { index, choice in
                    Button {
                        if let id = item.accountId { model.colorAccount(id, index: index) }
                    } label: {
                        Label {
                            Text(choice.name)
                        } icon: {
                            Image(nsImage: Self.swatch(choice.hex, chosen: choice.hex == item.accountColor))
                        }
                    }
                }
            }
            Button(tr("Move Up")) {
                if let id = item.accountId { model.moveAccount(id, step: -1) }
            }
            .disabled(place.first)
            Button(tr("Move Down")) {
                if let id = item.accountId { model.moveAccount(id, step: 1) }
            }
            .disabled(place.last)
            Divider()
            Button(tr("Sign In Again…")) {
                if let id = item.accountId { model.addAccount = AddAccountRequest(again: id) }
            }
            Divider()
            Button(tr("Remove Account…")) {
                if let id = item.accountId { removing = model.removeAccountQuestion(id) }
            }
    }

    /// A colour's dot for the menu, which draws a picture but no SwiftUI
    /// shape. The chosen colour gets a ring.
    private static func swatch(_ hex: String, chosen: Bool) -> NSImage {
        let image = NSImage(size: NSSize(width: 14, height: 14), flipped: false) { rect in
            let color = Color(hex: hex).map(NSColor.init) ?? .gray
            color.setFill()
            NSBezierPath(ovalIn: rect.insetBy(dx: chosen ? 3 : 1.5, dy: chosen ? 3 : 1.5)).fill()
            if chosen {
                color.setStroke()
                let ring = NSBezierPath(ovalIn: rect.insetBy(dx: 0.75, dy: 0.75))
                ring.lineWidth = 1.2
                ring.stroke()
            }
            return true
        }
        return image
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
    @Environment(MailModel.self) private var model
    let item: SidebarItem
    @State private var targeted = false

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
        // Mail dragged from the list lands here; the core decides what a
        // drop on this mailbox does, or says why it cannot.
        .dropDestination(for: String.self) { texts, _ in
            let keys = texts.flatMap(DraggedThreads.decode)
            guard !keys.isEmpty else { return false }
            model.drop(keys, on: item.key)
            return true
        } isTargeted: { targeted = $0 }
        .background(targeted ? Color.accentColor.opacity(0.25) : .clear, in: RoundedRectangle(cornerRadius: 6))
        .modifier(LabelMenu(item: item))
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
