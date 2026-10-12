import SwiftUI

/// An account's rules, as the GTK Rules dialog lists them: each in plain
/// words, opened to edit or deleted, with a form for a new one. The
/// server keeps them, or this Mac does for an account without a rules
/// server, and the line under the list says which.
struct RulesView: View {
    @Environment(MailModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let account: Int64
    @State private var page: RulesPage?
    @State private var refused: String?
    @State private var problem: String?
    @State private var editing: RuleEdit?

    var body: some View {
        VStack(spacing: 0) {
            if let page {
                list(page)
            } else if let refused {
                ContentUnavailableView {
                    Label(tr("Allow Rules"), systemImage: "lock")
                } description: {
                    Text(refused)
                } actions: {
                    Button(tr("Grant Access")) {
                        dismiss()
                        model.addAccount = AddAccountRequest(again: account)
                    }
                }
            } else if let problem {
                ContentUnavailableView(tr("Could Not Load the Rules"), systemImage: "exclamationmark.triangle", description: Text(problem))
            } else {
                ProgressView().frame(maxWidth: .infinity, maxHeight: .infinity)
            }
            Divider()
            HStack {
                Button(tr("Add Rule"), systemImage: "plus") { editing = RuleEdit(id: nil, fields: .empty) }
                    .help(tr("Add Rule"))
                    .disabled(page == nil)
                Spacer()
                Button(tr("Close")) { dismiss() }
                    .keyboardShortcut(.cancelAction)
            }
            .padding(14)
        }
        .frame(width: 560, height: 560)
        .navigationTitle(tr("Rules"))
        .task { load() }
        .sheet(item: $editing) { edit in
            if let page {
                RuleFormView(account: account, edit: edit, page: page) { load() }
            }
        }
    }

    private func list(_ page: RulesPage) -> some View {
        Form {
            Section {
                if !page.waiting.isEmpty {
                    Label(page.waiting, systemImage: "wifi.slash").foregroundStyle(.orange)
                }
                if page.rules.isEmpty {
                    Button(tr("Add Rule"), systemImage: "plus") { editing = RuleEdit(id: nil, fields: .empty) }
                        .help(tr("Add Rule"))
                }
                ForEach(page.rules, id: \.id) { rule in
                    HStack {
                        VStack(alignment: .leading, spacing: 2) {
                            Text(rule.criteria)
                            Text(rule.action).font(.callout).foregroundStyle(.secondary)
                            if rule.readOnly {
                                Text(tr("Made elsewhere. Change it where you made it."))
                                    .font(.callout).foregroundStyle(.secondary)
                            }
                        }
                        Spacer()
                        if rule.readOnly {
                            Image(systemName: "lock").foregroundStyle(.secondary)
                        } else {
                            Button(tr("Edit Rule"), systemImage: "pencil") {
                                editing = RuleEdit(id: rule.id, fields: rule.fields)
                            }
                            .labelStyle(.iconOnly)
                            .buttonStyle(.borderless)
                            .help(tr("Edit the rule for {mail}").replacingOccurrences(of: "{mail}", with: rule.criteria))
                            Button(tr("Delete"), systemImage: "trash") { delete(rule.id) }
                                .labelStyle(.iconOnly)
                                .buttonStyle(.borderless)
                                .help(tr("Delete the rule for {mail}").replacingOccurrences(of: "{mail}", with: rule.criteria))
                        }
                    }
                }
            } footer: {
                Text(page.place).foregroundStyle(.secondary)
            }
            if !page.elsewhere.isEmpty {
                Section(tr("Rules Written Elsewhere")) {
                    ForEach(page.elsewhere, id: \.self) { Text($0) }
                }
            }
        }
        .formStyle(.grouped)
    }

    private func load() {
        guard let mail = model.mail else { return }
        Task.detached {
            let read = Result { try mail.rules(accountId: account) }
            await MainActor.run {
                switch read {
                case .success(.page(let page)): self.page = page
                case .success(.needsPermission(let reason)): refused = reason
                case .failure(let error): problem = "\(error)"
                }
            }
        }
    }

    private func delete(_ id: String) {
        guard let mail = model.mail else { return }
        Task.detached {
            let done = Result { try mail.deleteRule(accountId: account, id: id) }
            await MainActor.run {
                switch done {
                case .success(let words): model.say(words)
                case .failure(let error): model.say("\(error)")
                }
                load()
            }
        }
    }
}

/// A rule open in the form: a new one, or rule `id`.
struct RuleEdit: Identifiable {
    let id: String?
    var fields: RuleFields
}

extension RuleFields {
    static let empty = RuleFields(from: "", to: "", subject: "", hasWords: "", notWords: "", hasAttachment: false,
                                  skipInbox: false, markRead: false, star: false, label: nil, neverSpam: false, trash: false)
}

/// When Mail Matches and Do This, as the GTK rule form has them.
private struct RuleFormView: View {
    @Environment(MailModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let account: Int64
    let edit: RuleEdit
    let page: RulesPage
    let saved: () -> Void
    @State private var fields: RuleFields
    @State private var saving = false

    init(account: Int64, edit: RuleEdit, page: RulesPage, saved: @escaping () -> Void) {
        self.account = account
        self.edit = edit
        self.page = page
        self.saved = saved
        _fields = State(initialValue: edit.fields)
    }

    var body: some View {
        VStack(spacing: 0) {
            Form {
                Section(tr("When Mail Matches")) {
                    TextField(tr("From"), text: $fields.from)
                    TextField(tr("To"), text: $fields.to)
                    TextField(tr("Subject"), text: $fields.subject)
                    TextField(tr("Has the Words"), text: $fields.hasWords)
                    TextField(tr("Doesn't Have"), text: $fields.notWords)
                    Toggle(tr("Has an Attachment"), isOn: $fields.hasAttachment)
                }
                Section(tr("Do This")) {
                    Toggle(tr("Skip the Inbox"), isOn: $fields.skipInbox)
                    Toggle(tr("Mark as Read"), isOn: $fields.markRead)
                    Toggle(tr("Star It"), isOn: $fields.star)
                    Picker(page.folders ? tr("Move to Folder") : tr("Apply Label"), selection: Binding(
                        get: { fields.label ?? "" },
                        set: { fields.label = $0.isEmpty ? nil : $0 }
                    )) {
                        Text(page.folders ? tr("Don't Move") : tr("Don't Apply a Label")).tag("")
                        ForEach(page.labels, id: \.id) { Text($0.name).tag($0.id) }
                    }
                    if page.neverSpam {
                        Toggle(tr("Never Send to Spam"), isOn: $fields.neverSpam)
                    }
                    Toggle(tr("Delete It"), isOn: $fields.trash)
                }
            }
            .formStyle(.grouped)
            Divider()
            HStack {
                if saving { ProgressView().controlSize(.small) }
                Spacer()
                Button(tr("Cancel"), role: .cancel) { dismiss() }
                    .keyboardShortcut(.cancelAction)
                Button(edit.id == nil ? tr("Create") : tr("Save")) { save() }
                    .keyboardShortcut(.defaultAction)
                    .disabled(saving)
            }
            .padding(14)
        }
        .frame(width: 480, height: 600)
        .navigationTitle(edit.id == nil ? tr("New Rule") : tr("Edit Rule"))
    }

    private func save() {
        guard let mail = model.mail else { return }
        let (id, fields) = (edit.id, fields)
        saving = true
        Task.detached {
            let done = Result { try mail.saveRule(accountId: account, id: id, fields: fields) }
            await MainActor.run {
                saving = false
                switch done {
                case .success(let words):
                    model.say(words)
                    saved()
                    dismiss()
                case .failure(let error):
                    model.say("\(error)")
                }
            }
        }
    }
}
