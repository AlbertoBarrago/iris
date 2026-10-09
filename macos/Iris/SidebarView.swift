import SwiftUI

/// The sidebar the Rust core lays out: headings, the mailboxes for every
/// account together, then each account with its own.
struct SidebarView: View {
    @Environment(MailModel.self) private var model

    var body: some View {
        @Bindable var model = model
        List(selection: $model.selectedMailbox) {
            ForEach(Array(model.sidebar.enumerated()), id: \.offset) { _, item in
                switch item.kind {
                case "heading":
                    Text(item.title.uppercased())
                        .font(.caption.weight(.semibold))
                        .foregroundStyle(.secondary)
                        .padding(.top, 10)
                        .selectionDisabled()
                case "account":
                    HStack(spacing: 6) {
                        Circle().fill(Color(hex: item.accountColor) ?? .accentColor).frame(width: 8, height: 8)
                        Text(item.title).lineLimit(1)
                    }
                    .padding(.top, 6)
                    .selectionDisabled()
                default:
                    MailboxRow(item: item).tag(item.key)
                }
            }
        }
        .listStyle(.sidebar)
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
        .padding(.leading, CGFloat(item.depth) * 14)
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
