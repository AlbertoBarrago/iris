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
            List(model.listing?.rows ?? [], id: \.self, selection: $model.selectedThread) { row in
                ThreadRowView(row: row)
                    .tag(ThreadKey(account: row.accountId, thread: row.threadId))
            }
            .listStyle(.inset)
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
                Button("Check for Mail", systemImage: "arrow.clockwise") { model.checkNow() }
                Button("Search", systemImage: "magnifyingglass") {}
                Button("New Message", systemImage: "square.and.pencil") {}
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
                        Image(systemName: "flag.fill").foregroundStyle(Color(hex: flag) ?? .red)
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
