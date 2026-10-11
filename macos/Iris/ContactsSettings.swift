import SwiftUI

/// Settings, Contacts: one switch per account for reading its address
/// book, as the GTK Preferences' Contacts group has it. An account whose
/// sign-in left contacts out offers Grant Access; one whose server keeps
/// none says why.
struct ContactsSettings: View {
    @Environment(MailModel.self) private var model
    @State private var accounts: [ContactAccount] = []

    var body: some View {
        Form {
            Section {
                if accounts.isEmpty {
                    Text(tr("No accounts yet")).foregroundStyle(.secondary)
                }
                ForEach(accounts, id: \.accountId) { account in
                    switch account.kind {
                    case "switch":
                        Toggle(account.email, isOn: Binding(
                            get: { account.on },
                            set: { set(account, on: $0) }
                        ))
                    case "withheld":
                        HStack {
                            VStack(alignment: .leading, spacing: 2) {
                                Text(account.email)
                                Text(account.reason).font(.callout).foregroundStyle(.secondary)
                            }
                            Spacer()
                            Button(tr("Grant Access")) {
                                model.addAccount = AddAccountRequest(again: account.accountId)
                            }
                        }
                    default:
                        VStack(alignment: .leading, spacing: 2) {
                            Text(account.email)
                            Text(account.reason).font(.callout).foregroundStyle(.secondary)
                        }
                    }
                }
            } header: {
                Text(tr("Contacts"))
            } footer: {
                Text(tr("Iris can read an account's contacts: names, email addresses, photos, organizations, and phone numbers. It uses them to suggest recipients, to show faces beside mail, and to fill the card behind a sender's name. What it reads stays on this computer, and turning an account off deletes its contacts."))
                    .foregroundStyle(.secondary)
            }
        }
        .formStyle(.grouped)
        .onAppear { load() }
    }

    private func load() {
        accounts = (try? model.mail?.contactAccounts()) ?? []
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
