import SwiftUI

/// The label picker, as the GTK app's Labels button opens it: the
/// account's labels with those already on the conversation ticked, or its
/// folders on an account that files in folders, and New Label at the end.
/// The core words it and decides what a row does (`Mail.filingMenu`).
struct FilingPicker: View {
    @Environment(MailModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    /// The conversations it files; nil for the selection.
    var keys: Set<ThreadKey>? = nil

    @State private var menu: FilingMenu?
    @State private var naming = false
    @State private var newName = ""

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            if let menu {
                Text(menu.title)
                    .font(.headline)
                    .padding(.horizontal, 14)
                    .padding(.top, 12)
                    .padding(.bottom, 6)
                if let said = menu.said {
                    Text(said)
                        .foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                        .padding(.horizontal, 14)
                        .padding(.vertical, 8)
                }
                if !menu.rows.isEmpty {
                    ScrollView {
                        VStack(alignment: .leading, spacing: 0) {
                            ForEach(menu.rows, id: \.id) { row in
                                FilingRowButton(row: row) {
                                    model.file(row, on: keys)
                                    dismiss()
                                }
                            }
                        }
                        .padding(.horizontal, 6)
                    }
                    .frame(maxHeight: 340)
                    .fixedSize(horizontal: false, vertical: true)
                }
                if !menu.newItem.isEmpty {
                    Divider().padding(.top, 6)
                    Button {
                        newName = ""
                        naming = true
                    } label: {
                        Label(menu.newItem, systemImage: "plus")
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .padding(.horizontal, 14)
                    .padding(.vertical, 9)
                }
            } else {
                ProgressView().padding(20)
            }
        }
        .frame(width: 260)
        .onAppear { menu = model.filingMenu(for: keys) }
        .alert(menu?.newHeading ?? "", isPresented: $naming) {
            TextField(menu?.newPrompt ?? "", text: $newName)
            Button(tr("Cancel"), role: .cancel) {}
            Button(tr("Create")) {
                model.createLabel(newName, on: keys)
                dismiss()
            }
            .keyboardShortcut(.defaultAction)
        }
    }
}

/// One row of the picker: a tick when the label is on the mail, then its
/// name, lit under the pointer as a menu row is.
private struct FilingRowButton: View {
    let row: FilingRow
    let choose: () -> Void
    @State private var hovering = false

    var body: some View {
        Button(action: choose) {
            HStack(spacing: 8) {
                Image(systemName: "checkmark")
                    .font(.callout.weight(.semibold))
                    .opacity(row.applied ? 1 : 0)
                    .frame(width: 14)
                Text(row.name).lineLimit(1)
                Spacer(minLength: 0)
            }
            .padding(.horizontal, 8)
            .padding(.vertical, 5)
            .background(hovering ? Color.accentColor.opacity(0.18) : .clear, in: RoundedRectangle(cornerRadius: 6))
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .onHover { hovering = $0 }
        .accessibilityLabel(row.described)
    }
}

/// The picker's rows as a submenu, for a right click on the list.
struct FilingSubmenu: View {
    @Environment(MailModel.self) private var model
    let keys: Set<ThreadKey>

    var body: some View {
        if let menu = model.filingMenu(for: keys) {
            Menu(menu.name, systemImage: menu.folders ? "folder" : "tag") {
                if let said = menu.said {
                    Text(said)
                }
                ForEach(menu.rows, id: \.id) { row in
                    Toggle(isOn: Binding(get: { row.applied }, set: { _ in model.file(row, on: keys) })) {
                        Text(row.name)
                    }
                    .accessibilityLabel(row.described)
                }
            }
        }
    }
}

/// What a dragged conversation carries: its account and thread, one
/// conversation per line, so a drag of the selection carries them all.
enum DraggedThreads {
    static let prefix = "iris-threads:"

    static func encode(_ keys: [ThreadKey]) -> String {
        prefix + keys.map { "\($0.account)\t\($0.thread)" }.joined(separator: "\n")
    }

    static func decode(_ text: String) -> [ThreadKey] {
        guard text.hasPrefix(prefix) else { return [] }
        return text.dropFirst(prefix.count).split(separator: "\n").compactMap { line in
            let parts = line.split(separator: "\t", maxSplits: 1)
            guard parts.count == 2, let account = Int64(parts[0]) else { return nil }
            return ThreadKey(account: account, thread: String(parts[1]))
        }
    }
}
