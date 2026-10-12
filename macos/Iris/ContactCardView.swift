import AppKit
import SwiftUI

extension ContactCard: Identifiable {
    public var id: String { email }
}

/// What a click on a sender opens: the person as the address book knows
/// them, with a message to write, the VIP switch and all their mail, as
/// the GTK app's contact card.
struct ContactCardView: View {
    @Environment(MailModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let card: ContactCard
    @State private var vip: Bool

    init(card: ContactCard) {
        self.card = card
        _vip = State(initialValue: card.vip)
    }

    var body: some View {
        VStack(spacing: 14) {
            ContactPhoto(card: card)
            VStack(spacing: 2) {
                Text(card.name).font(.title2.weight(.semibold)).multilineTextAlignment(.center)
                if !card.organization.isEmpty {
                    Text(card.organization).foregroundStyle(.secondary)
                }
            }
            .textSelection(.enabled)
            Form {
                ForEach(Array(card.addresses.enumerated()), id: \.offset) { index, address in
                    Button {
                        write(to: address)
                    } label: {
                        LabeledContent(index == 0 ? tr("Email") : tr("Other email")) {
                            Text(address).foregroundStyle(Color.accentColor)
                        }
                    }
                    .buttonStyle(.plain)
                    .help(tr("New Message"))
                    .contextMenu {
                        Button(tr("Copy Address")) {
                            NSPasteboard.general.clearContents()
                            NSPasteboard.general.setString(address, forType: .string)
                        }
                    }
                }
                if !card.phone.isEmpty {
                    LabeledContent(tr("Phone")) { Text(card.phone).textSelection(.enabled) }
                }
            }
            .formStyle(.grouped)
            .scrollDisabled(true)
            .fixedSize(horizontal: false, vertical: true)
            HStack {
                Button(tr("New Message"), systemImage: "square.and.pencil") { write(to: card.email) }
                    .help(tr("New Message"))
                Button(vip ? tr("Remove from VIPs") : tr("Add to VIPs"), systemImage: vip ? "star.slash" : "star") {
                    model.toggleVip(email: card.email, name: card.name)
                    vip.toggle()
                }
                .help(vip ? tr("Remove from VIPs") : tr("Add to VIPs"))
                Button(tr("All Their Mail"), systemImage: "magnifyingglass") {
                    dismiss()
                    model.searchFrom(card.email)
                }
                .help(tr("All Their Mail"))
            }
            .controlSize(.large)
        }
        .padding(20)
        .frame(width: 420)
        .toolbar {
            ToolbarItem(placement: .cancellationAction) {
                Button(tr("Close")) { dismiss() }
            }
        }
        .navigationTitle(tr("Contact"))
    }

    private func write(to address: String) {
        dismiss()
        model.composeAsked = ComposeRequest(kind: .new, to: address)
    }
}

/// The contact's photo, or their initials on a color picked from the
/// address, as the list draws a sender.
private struct ContactPhoto: View {
    let card: ContactCard

    var body: some View {
        Group {
            if !card.photo.isEmpty, let picture = NSImage(contentsOfFile: card.photo) {
                Image(nsImage: picture).resizable().scaledToFill()
            } else {
                ZStack {
                    Circle().fill(Color(hue: hue, saturation: 0.45, brightness: 0.7))
                    Text(initials).font(.system(size: 34, weight: .semibold)).foregroundStyle(.white)
                }
            }
        }
        .frame(width: 96, height: 96)
        .clipShape(Circle())
        .accessibilityHidden(true)
    }

    private var initials: String {
        let words = card.name.split(whereSeparator: { $0.isWhitespace }).prefix(2)
        let letters = words.compactMap(\.first).map { String($0).uppercased() }.joined()
        return letters.isEmpty ? String(card.email.prefix(1)).uppercased() : letters
    }

    private var hue: Double {
        let sum = card.email.unicodeScalars.reduce(0) { ($0 &* 31 &+ Int($1.value)) & 0xffff }
        return Double(sum % 360) / 360
    }
}
