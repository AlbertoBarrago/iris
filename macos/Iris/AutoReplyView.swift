import SwiftUI

/// What an account's menu opens in a sheet over the window.
enum AccountSheet: Identifiable, Equatable {
    case autoReply(Int64)
    case rules(Int64)

    var id: String {
        switch self {
        case .autoReply(let account): "reply-\(account)"
        case .rules(let account): "rules-\(account)"
        }
    }
}

/// The automatic reply of one account, as the GTK dialog has it: on or
/// off, the days it runs, the message, and whether only contacts hear
/// it. The server keeps it, so it answers while this Mac is off.
struct AutoReplyView: View {
    @Environment(MailModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let account: Int64
    @State private var form: AutoReplyForm?
    @State private var refused: String?
    @State private var problem: String?
    @State private var saving = false

    var body: some View {
        VStack(spacing: 0) {
            if let binding = Binding($form) {
                editor(binding)
            } else if let refused {
                ContentUnavailableView {
                    Label(tr("Allow Automatic Replies"), systemImage: "lock")
                } description: {
                    Text(refused)
                } actions: {
                    Button(tr("Grant Access")) {
                        dismiss()
                        model.addAccount = AddAccountRequest(again: account)
                    }
                }
            } else if let problem {
                ContentUnavailableView(tr("Could Not Load the Automatic Reply"), systemImage: "exclamationmark.triangle", description: Text(problem))
            } else {
                ProgressView().frame(maxWidth: .infinity, maxHeight: .infinity)
            }
            Divider()
            HStack {
                if saving { ProgressView().controlSize(.small) }
                Spacer()
                Button(tr("Cancel"), role: .cancel) { dismiss() }
                    .keyboardShortcut(.cancelAction)
                Button(tr("Save")) { save() }
                    .keyboardShortcut(.defaultAction)
                    .disabled(form == nil || saving)
            }
            .padding(14)
        }
        .frame(width: 500, height: 600)
        .navigationTitle(tr("Automatic Reply"))
        .task { load() }
    }

    private func editor(_ form: Binding<AutoReplyForm>) -> some View {
        Form {
            Section {
                Toggle(isOn: form.enabled) {
                    Text(tr("Send Automatic Replies"))
                    Text(form.wrappedValue.subtitle)
                }
            }
            Group {
                Section(tr("Dates")) {
                    Toggle(tr("Only Between These Dates"), isOn: form.dated)
                    DatePicker(tr("First Day"), selection: day(form.firstDay), displayedComponents: .date)
                        .disabled(!form.wrappedValue.dated)
                    DatePicker(tr("Last Day"), selection: day(form.lastDay), displayedComponents: .date)
                        .disabled(!form.wrappedValue.dated)
                }
                Section {
                    if form.wrappedValue.keepsSubject {
                        TextField(tr("Subject"), text: form.subject)
                    }
                    TextEditor(text: form.body)
                        .font(.body)
                        .frame(minHeight: 140)
                        .accessibilityLabel(tr("Message"))
                } header: {
                    Text(tr("Message"))
                } footer: {
                    if !form.wrappedValue.keepsSubject {
                        Text(tr("The same reply goes to people inside and outside your organization."))
                            .foregroundStyle(.secondary)
                    }
                }
                if form.wrappedValue.canLimitToContacts {
                    Section {
                        Toggle(tr("Only Reply to My Contacts"), isOn: form.contactsOnly)
                    }
                }
            }
            .disabled(!form.wrappedValue.enabled)
        }
        .formStyle(.grouped)
    }

    /// A day as the date picker takes it, kept as local midnight.
    private func day(_ millis: Binding<Int64>) -> Binding<Date> {
        Binding(
            get: { Date(timeIntervalSince1970: TimeInterval(millis.wrappedValue) / 1000) },
            set: { millis.wrappedValue = Int64(Calendar.current.startOfDay(for: $0).timeIntervalSince1970 * 1000) }
        )
    }

    private func load() {
        guard let mail = model.mail else { return }
        Task.detached {
            let read = Result { try mail.automaticReply(accountId: account) }
            await MainActor.run {
                switch read {
                case .success(.form(let form)): self.form = form
                case .success(.needsPermission(let reason)): refused = reason
                case .failure(let error): problem = "\(error)"
                }
            }
        }
    }

    private func save() {
        guard let mail = model.mail, let form else { return }
        saving = true
        Task.detached {
            let done = Result { try mail.setAutomaticReply(accountId: account, form: form) }
            await MainActor.run {
                saving = false
                switch done {
                case .success(let words):
                    model.say(words)
                    dismiss()
                case .failure(let error):
                    model.say("\(error)")
                }
            }
        }
    }
}
