import AppKit
import SwiftUI

/// Hide My Email, as the GTK dialog has it: the plus addresses made so
/// far, each with Copy, on or off and Delete, and a form for a new one.
struct HideMyEmailView: View {
    @Environment(MailModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    /// The account new addresses belong to unless another is picked.
    let account: Int64
    @State private var page: HiddenPage?
    @State private var problem: String?
    @State private var making = false
    @State private var chosen = ""
    @State private var note = ""
    @State private var made: String?
    @State private var deleting: String?
    @State private var working = false

    var body: some View {
        VStack(spacing: 0) {
            if let made {
                created(made)
            } else if making, let page {
                form(page)
            } else if let page {
                list(page)
            } else if let problem {
                ContentUnavailableView(problem, systemImage: "exclamationmark.triangle")
            } else {
                ProgressView().frame(maxWidth: .infinity, maxHeight: .infinity)
            }
            Divider()
            HStack {
                if working { ProgressView().controlSize(.small) }
                Spacer()
                if making && made == nil {
                    Button(tr("Cancel"), role: .cancel) { making = false }
                    Button(tr("Create")) { create() }
                        .keyboardShortcut(.defaultAction)
                        .disabled(chosen.isEmpty || working)
                } else if made != nil {
                    Button(tr("Done")) { made = nil; making = false; load() }
                        .keyboardShortcut(.defaultAction)
                } else {
                    Button(tr("Create Address"), systemImage: "plus") { making = true }
                        .disabled(page?.accounts.isEmpty ?? true)
                    Button(tr("Close")) { dismiss() }
                        .keyboardShortcut(.cancelAction)
                }
            }
            .padding(14)
        }
        .frame(width: 520, height: 520)
        .navigationTitle(tr("Hide My Email"))
        .task { load() }
        .alert(tr("Delete Address?"), isPresented: Binding(get: { deleting != nil }, set: { if !$0 { deleting = nil } }),
               presenting: deleting) { address in
            Button(tr("Cancel"), role: .cancel) {}
            Button(tr("Delete"), role: .destructive) { run { try $0.deleteHiddenAddress(address: address) } }
        } message: { address in
            Text(model.mail?.hiddenDeleteWords(address: address) ?? "")
        }
    }

    private func list(_ page: HiddenPage) -> some View {
        Form {
            Section {
                if page.rows.isEmpty {
                    VStack(alignment: .leading) {
                        Text(tr("No addresses yet"))
                        Text(tr("Create one with the + button.")).foregroundStyle(.secondary)
                    }
                }
                ForEach(page.rows, id: \.address) { row in
                    HStack {
                        VStack(alignment: .leading, spacing: 2) {
                            Text(row.address).textSelection(.enabled)
                            Text(row.details).font(.callout).foregroundStyle(.secondary)
                        }
                        Spacer()
                        Button(tr("Copy Address"), systemImage: "doc.on.doc") { copy(row.address) }
                            .labelStyle(.iconOnly).buttonStyle(.borderless)
                        Toggle(tr("Receive Mail"), isOn: Binding(get: { row.active }, set: { on in
                            run { try $0.setHiddenAddressActive(address: row.address, active: on) }
                        }))
                        .labelsHidden()
                        .help(tr("Receive Mail"))
                        Button(tr("Delete Address"), systemImage: "trash") { deleting = row.address }
                            .labelStyle(.iconOnly).buttonStyle(.borderless)
                    }
                }
            } footer: {
                Text(page.about).foregroundStyle(.secondary)
            }
        }
        .formStyle(.grouped)
        .disabled(working)
    }

    private func form(_ page: HiddenPage) -> some View {
        Form {
            Section {
                Picker(tr("Account"), selection: $chosen) {
                    ForEach(page.accounts, id: \.self) { Text($0).tag($0) }
                }
                TextField(tr("Where Did You Use It?"), text: $note)
            } footer: {
                Text(page.newAbout).foregroundStyle(.secondary)
            }
        }
        .formStyle(.grouped)
        .onAppear {
            if chosen.isEmpty {
                let preferred = model.sidebar.first { $0.accountId == account && $0.kind == "account" }?.title
                chosen = page.accounts.first { $0 == preferred } ?? page.accounts.first ?? ""
            }
        }
    }

    private func created(_ address: String) -> some View {
        VStack(spacing: 12) {
            Text(tr("Your New Address")).font(.headline)
            Text(address).font(.title3.monospaced()).textSelection(.enabled)
            Button(tr("Copy")) { copy(address) }
            Text(tr("It is on your clipboard. Mail sent to it gets the Hide My Email label in Gmail."))
                .multilineTextAlignment(.center)
                .foregroundStyle(.secondary)
        }
        .padding(24)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    private func copy(_ address: String) {
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(address, forType: .string)
        model.say(tr("Address copied"))
    }

    private func load() {
        guard let mail = model.mail else { return }
        Task.detached {
            let read = Result { try mail.hiddenAddresses() }
            await MainActor.run {
                switch read {
                case .success(let found):
                    page = found
                    if found.rows.isEmpty, !found.accounts.isEmpty { making = true }
                case .failure(let error): problem = "\(error)"
                }
            }
        }
    }

    private func create() {
        guard let mail = model.mail else { return }
        let (email, where_) = (chosen, note)
        working = true
        Task.detached {
            let done = Result { try mail.createHiddenAddress(email: email, note: where_) }
            await MainActor.run {
                working = false
                switch done {
                case .success(let address):
                    NSPasteboard.general.clearContents()
                    NSPasteboard.general.setString(address, forType: .string)
                    note = ""
                    made = address
                case .failure(let error): model.say("\(error)")
                }
            }
        }
    }

    /// Runs one change off the main thread, says how it went, and reads
    /// the list again.
    private func run(_ change: @escaping @Sendable (Mail) throws -> String) {
        guard let mail = model.mail else { return }
        working = true
        Task.detached {
            let done = Result { try change(mail) }
            await MainActor.run {
                working = false
                switch done {
                case .success(let words): model.say(words)
                case .failure(let error): model.say("\(error)")
                }
                load()
            }
        }
    }
}
