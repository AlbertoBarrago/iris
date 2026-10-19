import SwiftUI

/// A smart mailbox open in its editor: a new one, or one the person made.
struct SmartEdit: Identifiable {
    let id = UUID()
    var form: SmartForm
}

/// New Smart Mailbox and Edit Smart Mailbox, as the GTK editor had them:
/// a name, the accounts it looks in, whether every condition or any one
/// must hold, and the conditions themselves.
struct SmartEditorView: View {
    @Environment(MailModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var form: SmartForm
    @State private var fields: [SmartField] = []
    @State private var accounts: [String] = []
    @State private var problem: String?

    init(edit: SmartEdit) {
        _form = State(initialValue: edit.form)
    }

    var body: some View {
        VStack(spacing: 0) {
            Form {
                Section {
                    // A grouped form sets its fields flush right, where
                    // AppKit hides a trailing space until the next letter
                    // comes, so the name reads as if spaces did not take.
                    TextField(tr("Name"), text: $form.name)
                        .multilineTextAlignment(.leading)
                    Picker(tr("Accounts"), selection: $form.account) {
                        Text(tr("All Accounts")).tag("")
                        ForEach(accounts, id: \.self) { Text($0).tag($0) }
                    }
                    Picker(tr("Match"), selection: $form.matchAll) {
                        Text(tr("All of the conditions")).tag(true)
                        Text(tr("Any of the conditions")).tag(false)
                    }
                }
                Section(tr("Conditions")) {
                    ForEach(form.conditions.indices, id: \.self) { index in
                        HStack {
                            Picker(tr("Condition"), selection: $form.conditions[index].field) {
                                ForEach(fields, id: \.key) { Text($0.label).tag($0.key) }
                            }
                            .labelsHidden()
                            if takesValue(form.conditions[index].field) {
                                TextField(tr("Matches"), text: $form.conditions[index].value)
                                    .labelsHidden()
                            }
                            Button(tr("Remove Condition"), systemImage: "minus.circle") {
                                form.conditions.remove(at: index)
                            }
                            .help(tr("Remove Condition"))
                            .labelStyle(.iconOnly)
                            .buttonStyle(.borderless)
                        }
                    }
                    Button(tr("Add Condition"), systemImage: "plus") {
                        form.conditions.append(SmartCondition(field: fields.first?.key ?? "from", value: ""))
                    }
                    .help(tr("Add Condition"))
                }
                if let problem {
                    Text(problem).foregroundStyle(.red)
                }
            }
            .formStyle(.grouped)
            Divider()
            HStack {
                Spacer()
                Button(tr("Cancel"), role: .cancel) { dismiss() }
                    .keyboardShortcut(.cancelAction)
                Button(form.id.isEmpty ? tr("Create") : tr("Save")) { save() }
                    .keyboardShortcut(.defaultAction)
            }
            .padding(14)
        }
        .frame(width: 520, height: 520)
        .navigationTitle(form.id.isEmpty ? tr("New Smart Mailbox") : tr("Edit Smart Mailbox"))
        .onAppear {
            fields = model.mail?.smartFields() ?? []
            accounts = (try? model.mail?.smartAccounts()) ?? []
            if form.conditions.isEmpty, let first = fields.first {
                form.conditions = [SmartCondition(field: first.key, value: "")]
            }
        }
    }

    private func takesValue(_ key: String) -> Bool {
        fields.first { $0.key == key }?.takesValue ?? true
    }

    private func save() {
        do {
            try model.mail?.saveSmartMailbox(form: form)
            model.refresh()
            dismiss()
        } catch {
            problem = "\(error)"
        }
    }
}
