import SwiftUI

/// Each account keeps its personal contacts and directory choices together.
/// Permission buttons appear beside the enabled source that needs access.
struct ContactsSettings: View {
    @Environment(MailModel.self) private var model
    @State private var accounts: [ContactAccount] = []

    var body: some View {
        Form {
            if accounts.isEmpty {
                Section(tr("Contacts")) {
                    Text(tr("No accounts yet")).foregroundStyle(.secondary)
                }
            }
            ForEach(accounts, id: \.accountId) { account in
                Section {
                    switch account.kind {
                    case "switch", "withheld":
                        Toggle(tr("Personal contacts"), isOn: Binding(
                            get: { account.on },
                            set: { set(account, on: $0) }
                        ))
                        .accessibilityLabel(tr("Personal contacts") + ": " + account.email)
                        if account.on && account.kind == "withheld" {
                            Text(account.reason).font(.callout).foregroundStyle(.secondary)
                            Button(tr("Authorize contacts for {account}")
                                .replacingOccurrences(of: "{account}", with: account.email)) {
                                model.addAccount = AddAccountRequest(again: account.accountId)
                            }
                        }
                    default:
                        Text(account.reason).font(.callout).foregroundStyle(.secondary)
                    }
                    if account.directoryAvailable {
                        Toggle(tr("Search the company directory"), isOn: Binding(
                            get: { account.directoryOn },
                            set: { setDirectory(account, on: $0) }
                        ))
                        .accessibilityLabel(tr("Search the company directory") + ": " + account.email)
                        if account.directoryOn && account.directoryNeedsAccess {
                            Button(tr("Authorize directory for {account}")
                                .replacingOccurrences(of: "{account}", with: account.email)) {
                                model.addAccount = AddAccountRequest(again: account.accountId)
                            }
                        }
                    }
                } header: {
                    Text(account.email)
                }
            }
            Section {
                Text(tr("Iris can read an account's contacts: names, email addresses, photos, organizations, and phone numbers. It uses them to suggest recipients, to show faces beside mail, and to fill the card behind a sender's name. What it reads stays on this computer, and turning an account off deletes its contacts."))
                    .font(.callout)
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
            ContactListsSection()
        }
        .formStyle(.grouped)
        .onAppear { load() }
        .onChange(of: model.addAccount == nil) { _, closed in
            if closed { load() }
        }
    }

    private func load() {
        accounts = (try? model.mail?.contactAccounts()) ?? []
    }

    private func setDirectory(_ account: ContactAccount, on: Bool) {
        guard let mail = model.mail else { return }
        do { try mail.setAccountDirectory(accountId: account.accountId, on: on) }
        catch { model.say("\(error)") }
        load()
    }

    /// Reads or forgets the account's address book off the main thread,
    /// which can take a while for a large one.
    private func set(_ account: ContactAccount, on: Bool) {
        guard let mail = model.mail else { return }
        if let index = accounts.firstIndex(where: { $0.accountId == account.accountId }) {
            accounts[index].on = on
        }
        Task.detached {
            let done = Result { try mail.setAccountContacts(accountId: account.accountId, email: account.email, on: on) }
            await MainActor.run {
                if case .failure(let error) = done { model.say("\(error)") }
                load()
            }
        }
    }
}

private struct ContactListsSection: View {
    @Environment(MailModel.self) private var model
    @State private var lists: [LocalContactList] = []
    @State private var editing: LocalContactList?
    @State private var deleting: LocalContactList?
    @State private var revision = UUID()
    @State private var busy = false
    @State private var problem: String?

    var body: some View {
        Section {
            ForEach(lists) { list in
                HStack {
                    VStack(alignment: .leading, spacing: 2) {
                        Text(list.name)
                        Text(list.summary).font(.callout).foregroundStyle(.secondary)
                    }
                    Spacer()
                    Button(tr("Edit List"), systemImage: "pencil") { editing = list }
                        .labelStyle(.iconOnly)
                        .buttonStyle(.borderless)
                        .help(tr("Edit List"))
                    Button(tr("Delete List"), systemImage: "trash") { deleting = list }
                        .labelStyle(.iconOnly)
                        .buttonStyle(.borderless)
                        .help(tr("Delete List"))
                }
            }
            Button(tr("Add List")) {
                editing = LocalContactList(id: 0, name: "", members: [], summary: "")
            }
            if let problem {
                Text(problem).font(.callout).foregroundStyle(.red)
                Button(tr("Try Again")) { revision = UUID() }
            }
        } header: {
            Text(tr("Contact Lists"))
        } footer: {
            Text(tr("Keep a group on this computer, then type its name in To, Cc, or Bcc to add its members."))
                .foregroundStyle(.secondary)
        }
        .disabled(busy)
        .task(id: revision) {
            guard let mail = model.mail else { return }
            do {
                let found = try await Task.detached { try mail.contactLists() }.value
                guard !Task.isCancelled else { return }
                lists = found
                problem = nil
            } catch {
                if !Task.isCancelled { problem = "\(error)" }
            }
        }
        .sheet(item: $editing, onDismiss: { revision = UUID() }) { list in
            ContactListEditor(list: list)
        }
        .alert(
            tr("Delete {name}?").replacingOccurrences(of: "{name}", with: deleting?.name ?? ""),
            isPresented: Binding(get: { deleting != nil }, set: { if !$0 { deleting = nil } }),
            presenting: deleting
        ) { list in
            Button(tr("Cancel"), role: .cancel) {}
            Button(tr("Delete"), role: .destructive) { delete(list.id) }
        } message: { _ in
            Text(tr("The members stay in your address books."))
        }
    }

    private func delete(_ id: Int64) {
        guard let mail = model.mail, !busy else { return }
        busy = true
        Task {
            do {
                try await Task.detached { try mail.deleteContactList(id: id) }.value
                revision = UUID()
            } catch {
                problem = "\(error)"
            }
            busy = false
        }
    }
}

extension LocalContactList: Identifiable {}

private struct ContactListEditor: View {
    @Environment(MailModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let list: LocalContactList
    @State private var name: String
    @State private var members: [ContactListMember]
    @State private var adding = ""
    @State private var problem: String?
    @State private var busy = false

    init(list: LocalContactList) {
        self.list = list
        _name = State(initialValue: list.name)
        _members = State(initialValue: list.members)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(list.id == 0 ? tr("New List") : tr("Edit List")).font(.headline)
            TextField(tr("Name"), text: $name)
            Text(tr("Members")).font(.subheadline).foregroundStyle(.secondary)
            ScrollView {
                VStack(spacing: 8) {
                    ForEach(members, id: \.email) { member in
                        HStack {
                            VStack(alignment: .leading, spacing: 2) {
                                if !member.name.isEmpty { Text(member.name) }
                                Text(member.email).foregroundStyle(.secondary)
                            }
                            Spacer()
                            Button(tr("Remove Member"), systemImage: "minus.circle") {
                                members.removeAll { $0.email == member.email }
                            }
                            .labelStyle(.iconOnly)
                            .buttonStyle(.borderless)
                            .help(tr("Remove Member"))
                        }
                    }
                }
                .padding(10)
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            .frame(height: 180)
            .background(.quaternary.opacity(0.4), in: RoundedRectangle(cornerRadius: 8))
            HStack(alignment: .top) {
                RecipientField(placeholder: tr("Name or email address"), text: $adding, account: 0,
                               leaving: Set(members.map(\.email)), taken: { addMembers() })
                Button(tr("Add")) { addMembers() }
                    .disabled(adding.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
            }
            Text(tr("Add an email address or choose a suggested contact. You can paste several addresses at once."))
                .font(.callout).foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
            if let problem {
                Text(problem).font(.callout).foregroundStyle(.red)
                    .fixedSize(horizontal: false, vertical: true)
            }
            HStack {
                Spacer()
                Button(tr("Cancel"), role: .cancel) { dismiss() }
                    .keyboardShortcut(.cancelAction)
                Button(list.id == 0 ? tr("Create") : tr("Save")) { save() }
                    .keyboardShortcut(.defaultAction)
            }
        }
        .disabled(busy)
        .padding(20)
        .frame(width: 460)
    }

    private func addMembers() {
        guard let mail = model.mail else { return }
        do {
            members = try mail.contactListMembers(existing: members, adding: adding)
            adding = ""
            problem = nil
        } catch {
            problem = "\(error)"
        }
    }

    private func save() {
        guard let mail = model.mail, !busy else { return }
        let id = list.id
        let name = name
        let members = members
        let adding = adding
        busy = true
        Task {
            do {
                _ = try await Task.detached {
                    let included = try mail.contactListMembers(existing: members, adding: adding)
                    return try mail.saveContactList(id: id, name: name, members: included)
                }.value
                dismiss()
            } catch {
                problem = "\(error)"
            }
            busy = false
        }
    }
}
