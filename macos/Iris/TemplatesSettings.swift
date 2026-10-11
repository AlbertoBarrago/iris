import SwiftUI

/// Settings → Writing → Templates: the saved bodies the composer's
/// Templates menu drops into a message, with a row each to edit or
/// delete it, as the GTK Preferences group lists them.
struct TemplatesSection: View {
    @Environment(MailModel.self) private var model
    @State private var templates: [TemplateItem] = []
    @State private var editing: TemplateItem?
    @State private var deleting: TemplateItem?

    var body: some View {
        Section {
            ForEach(templates, id: \.id) { template in
                HStack {
                    VStack(alignment: .leading, spacing: 2) {
                        Text(template.name)
                        Text(template.summary).font(.callout).foregroundStyle(.secondary).lineLimit(1)
                    }
                    Spacer()
                    Button(tr("Edit Template"), systemImage: "pencil") { editing = template }
                        .labelStyle(.iconOnly)
                        .buttonStyle(.borderless)
                        .help(tr("Edit {template}").replacingOccurrences(of: "{template}", with: template.name))
                    Button(tr("Delete Template"), systemImage: "trash") { deleting = template }
                        .labelStyle(.iconOnly)
                        .buttonStyle(.borderless)
                        .help(tr("Delete {template}").replacingOccurrences(of: "{template}", with: template.name))
                }
            }
            Button(tr("Add Template")) {
                editing = TemplateItem(id: 0, name: "", subject: "", markdown: "", summary: "")
            }
        } header: {
            Text(tr("Templates"))
        } footer: {
            Text(tr("Saved bodies the composer's Templates menu drops into a message. Markdown works, and {{first_name}}, {{name}}, {{email}}, {{subject}} and {{date}} fill in when you use one."))
                .foregroundStyle(.secondary)
        }
        .onAppear { load() }
        .onReceive(NotificationCenter.default.publisher(for: .templatesChanged)) { _ in load() }
        .sheet(item: $editing) { template in
            TemplateEditor(template: template)
        }
        .alert(
            tr("Delete {name}?").replacingOccurrences(of: "{name}", with: deleting?.name ?? ""),
            isPresented: Binding(get: { deleting != nil }, set: { if !$0 { deleting = nil } }),
            // The alert hands its buttons the template it asks about: by
            // the time a button runs, closing the alert has emptied
            // `deleting`.
            presenting: deleting
        ) { template in
            Button(tr("Cancel"), role: .cancel) {}
            Button(tr("Delete"), role: .destructive) { delete(template.id) }
        } message: { _ in
            Text(tr("This computer keeps the only copy."))
        }
    }

    private func load() {
        templates = (try? model.mail?.templates()) ?? []
    }

    private func delete(_ id: Int64) {
        do {
            try model.mail?.deleteTemplate(id: id)
            NotificationCenter.default.post(name: .templatesChanged, object: nil)
        } catch {
            model.say("\(error)")
        }
    }
}

extension TemplateItem: Identifiable {}

/// A template's name, subject and Markdown body, new or saved.
private struct TemplateEditor: View {
    @Environment(MailModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let template: TemplateItem
    @State private var name: String
    @State private var subject: String
    @State private var markdown: String
    @State private var problem: String?

    init(template: TemplateItem) {
        self.template = template
        _name = State(initialValue: template.name)
        _subject = State(initialValue: template.subject)
        _markdown = State(initialValue: template.markdown)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(template.id == 0 ? tr("New Template") : tr("Edit Template")).font(.headline)
            Form {
                TextField(tr("Name"), text: $name)
                TextField(tr("Subject"), text: $subject)
            }
            .formStyle(.columns)
            Text(tr("The subject is optional. A template with one gives it to a message that has none."))
                .font(.callout)
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
            TextEditor(text: $markdown)
                .font(.body.monospaced())
                .frame(minHeight: 200)
                .scrollContentBackground(.hidden)
                .padding(6)
                .background(Color(nsColor: .textBackgroundColor), in: RoundedRectangle(cornerRadius: 6))
                .accessibilityLabel(tr("Body"))
            if let problem {
                Text(problem).foregroundStyle(.red).font(.callout)
            }
            HStack {
                Spacer()
                Button(tr("Cancel"), role: .cancel) { dismiss() }
                    .keyboardShortcut(.cancelAction)
                Button(template.id == 0 ? tr("Create") : tr("Save")) { save() }
                    .keyboardShortcut(.defaultAction)
            }
        }
        .padding(20)
        .frame(width: 480)
    }

    private func save() {
        guard let mail = model.mail else { return }
        do {
            _ = try mail.saveTemplate(id: template.id, name: name, subject: subject, markdown: markdown)
            NotificationCenter.default.post(name: .templatesChanged, object: nil)
            dismiss()
        } catch {
            problem = "\(error)"
        }
    }
}
