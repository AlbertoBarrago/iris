import SwiftUI

/// The AI page of Settings: which connection and model the assistant
/// runs on, the keys it needs, how careful it is and how it reaches the
/// web. The words are the GTK AI page's, and every change lands in the
/// same settings file and Keychain.
struct AISettings: View {
    @Environment(MailModel.self) private var model
    @Environment(AssistantModel.self) private var assistant
    @State private var ai: AiPreferences?
    @State private var choices: AiChoices?

    var body: some View {
        if let ai, let choices {
            Form {
                AssistantRows(ai: ai, providers: choices.providers, changed: reload)
                ConnectionRows(ai: ai, changed: reload)
                FoundRows(changed: reload)
                Section(tr("Safety")) {
                    toggle(tr("Ask Before Acting"),
                           tr("Approve each message the assistant sends and each change to Gmail settings, such as automatic replies and rules"),
                           "confirm_actions", ai.confirmActions)
                }
                WebSearchRows(ai: ai, engines: choices.webSearch, changed: reload)
                Section(tr("Conversation")) {
                    toggle(tr("Show Details Expanded"),
                           tr("Open the model's thinking and each tool it runs as they appear"),
                           "details_expanded", ai.detailsExpanded)
                }
                if !ai.allowedTools.isEmpty {
                    AllowedRows(tools: ai.allowedTools, changed: reload)
                }
            }
            .formStyle(.grouped)
        } else {
            ProgressView().padding(40).onAppear(perform: reload)
        }
    }

    /// Reads the settings again, and tells the assistant's pane, whose
    /// subtitle names the model.
    private func reload() {
        ai = model.aiPreferences()
        if choices == nil { choices = model.aiChoices() }
        assistant.refresh()
    }

    private func toggle(_ title: String, _ subtitle: String, _ name: String, _ on: Bool) -> some View {
        Toggle(isOn: Binding(get: { on }, set: { value in
            model.changeAi { try $0.setAiSetting(name: name, value: value ? "true" : "false") }
            reload()
        })) {
            Text(title)
            Text(subtitle)
        }
    }
}

/// The assistant's connection and model, with the list the connection
/// offers and a test of it.
private struct AssistantRows: View {
    @Environment(MailModel.self) private var model
    let ai: AiPreferences
    let providers: [ChoiceItem]
    let changed: () -> Void
    @State private var typed = ""
    @State private var picking = false

    var body: some View {
        Section {
            Picker(tr("Connection"), selection: Binding(get: { ai.provider }, set: { provider in
                model.changeAi { try $0.setAiProvider(provider: provider) }
                changed()
            })) {
                ForEach(providers, id: \.key) { Text($0.label).tag($0.key) }
            }
            if ai.provider != "off" {
                HStack {
                    TextField(tr("Model"), text: $typed, prompt: Text(placeholder))
                        .onSubmit(keep)
                    Button {
                        picking = true
                    } label: {
                        Image(systemName: "list.bullet")
                    }
                    .buttonStyle(.borderless)
                    .help(tr("Models You Can Use"))
                    .accessibilityLabel(tr("Models You Can Use"))
                    .popover(isPresented: $picking, arrowEdge: .trailing) {
                        ModelPicker(provider: ai.provider, chosen: ai.model) { id in
                            picking = false
                            typed = id
                            keep()
                        }
                    }
                }
                TestRow(provider: ai.provider)
            }
        } header: {
            Text(tr("Assistant"))
        }
        .onAppear { typed = ai.model }
        .onChange(of: ai.provider) { typed = ai.model }
    }

    /// Claude Code runs its own default when the field is empty; the
    /// other connections need a name.
    private var placeholder: String {
        ai.provider == "claude-code" ? tr("Claude Code's own default") : ""
    }

    private func keep() {
        guard typed != ai.model else { return }
        model.changeAi { try $0.setAiModel(model: typed) }
        changed()
    }
}

/// The models a connection offers, asked for when the list opens, with a
/// search over their names.
private struct ModelPicker: View {
    @Environment(MailModel.self) private var model
    let provider: String
    let chosen: String
    let pick: (String) -> Void
    @State private var listing: ModelListing?
    @State private var problem: String?
    @State private var query = ""

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            TextField(tr("Search models"), text: $query)
                .textFieldStyle(.roundedBorder)
            if let listing {
                List {
                    if provider == "claude-code" {
                        row(id: "", title: tr("Claude Code's own default"), alias: false)
                    }
                    ForEach(shown(listing.models), id: \.id) { item in
                        row(id: item.id, title: item.name.isEmpty ? item.id : item.name, alias: item.alias)
                    }
                }
                .listStyle(.plain)
                .frame(minHeight: 220)
                if let note = note(listing) {
                    Text(note).font(.caption).foregroundStyle(.secondary)
                }
            } else if let problem {
                Text(problem).foregroundStyle(.secondary)
            } else {
                Text(tr("Asking for the model list…")).foregroundStyle(.secondary)
            }
        }
        .padding(12)
        .frame(width: 320)
        .task {
            do {
                listing = try await model.listAiModels(provider)
            } catch {
                problem = "\(error)"
            }
        }
    }

    private func row(id: String, title: String, alias: Bool) -> some View {
        Button { pick(id) } label: {
            HStack {
                VStack(alignment: .leading) {
                    Text(title)
                    if title != id, !id.isEmpty {
                        Text(id).font(.caption).foregroundStyle(.secondary)
                    }
                }
                Spacer()
                if alias { Text(tr("alias")).font(.caption).foregroundStyle(.secondary) }
                if id == chosen { Image(systemName: "checkmark") }
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }

    private func shown(_ models: [ModelChoice]) -> [ModelChoice] {
        let query = query.trimmingCharacters(in: .whitespaces).lowercased()
        guard !query.isEmpty else { return models }
        return models.filter { "\($0.name) \($0.id)".lowercased().contains(query) }
    }

    private func note(_ listing: ModelListing) -> String? {
        var notes = listing.note.map { [$0] } ?? []
        if listing.models.isEmpty, provider != "claude-code" {
            notes.append(tr("This provider lists no models. Type a name in the field instead."))
        }
        return notes.isEmpty ? nil : notes.joined(separator: " ")
    }
}

/// Asks a connection for one line and shows what came back.
private struct TestRow: View {
    @Environment(MailModel.self) private var model
    let provider: String
    @State private var testing = false
    @State private var answer: String?

    var body: some View {
        HStack {
            VStack(alignment: .leading) {
                Text(tr("Test the Connection"))
                if let answer {
                    Text(answer).font(.caption).foregroundStyle(.secondary).textSelection(.enabled)
                }
            }
            Spacer()
            if testing { ProgressView().controlSize(.small) }
            Button(tr("Test"), action: test).disabled(testing)
        }
        .onChange(of: provider) { answer = nil }
    }

    private func test() {
        testing = true
        answer = nil
        Task {
            do {
                answer = try await model.testAi(provider)
            } catch {
                answer = "\(error)"
            }
            testing = false
        }
    }
}

/// How each connection is reached: the local server's address and key,
/// Anthropic's key, and the `claude` command.
private struct ConnectionRows: View {
    @Environment(MailModel.self) private var model
    let ai: AiPreferences
    let changed: () -> Void
    @State private var address = ""
    @State private var localKey = ""
    @State private var anthropicKey = ""

    var body: some View {
        Section {
            DisclosureGroup {
                TextField(tr("Server Address"), text: $address)
                    .onSubmit {
                        model.changeAi { try $0.setAiSetting(name: "base_url", value: address) }
                        changed()
                    }
                KeyField(title: tr("API Key (Optional)"), name: "local", value: $localKey, changed: changed)
            } label: {
                Text(tr("Local Server"))
                Text(ai.baseUrl)
            }
            DisclosureGroup {
                KeyField(title: ai.anthropicKeySaved ? tr("Anthropic API Key (Saved)") : tr("Anthropic API Key"),
                         name: "anthropic", value: $anthropicKey, changed: changed)
            } label: {
                Text(tr("Anthropic API"))
                Text(ai.anthropicKeySaved ? tr("Key saved in the keyring") : tr("No key saved"))
            }
            VStack(alignment: .leading) {
                Text(tr("Claude Subscription"))
                Text(claude).font(.caption).foregroundStyle(.secondary)
            }
        } header: {
            Text(tr("Connections"))
        } footer: {
            Text(tr("A local server is LM Studio, Ollama, or any server with OpenAI's API. Local models keep your mail on this computer. With Anthropic or a Claude subscription, what a feature reads goes to Anthropic."))
                .font(.caption)
                .foregroundStyle(.secondary)
        }
        .onAppear { address = ai.baseUrl }
        .onChange(of: ai.baseUrl) { address = ai.baseUrl }
    }

    private var claude: String {
        guard let path = ai.claudePath else {
            return tr("Not found. Install Claude Code and sign in by running claude once.")
        }
        return tr("Uses your Claude subscription through {path}").replacingOccurrences(of: "{path}", with: path)
    }
}

/// A key typed once and kept in the Keychain on Return; an empty field
/// removes the saved one. The field empties after, so the key is never
/// shown again.
private struct KeyField: View {
    @Environment(MailModel.self) private var model
    let title: String
    let name: String
    @Binding var value: String
    let changed: () -> Void

    var body: some View {
        SecureField(title, text: $value)
            .onSubmit {
                let key = value
                Task {
                    do {
                        // The field's title says the key is saved.
                        try await model.saveAiKey(name, key)
                    } catch {
                        model.say("\(error)")
                    }
                    value = ""
                    changed()
                }
            }
    }
}

/// LM Studio, Ollama, Unsloth and Claude Code found on this Mac, each a
/// button away from being the assistant's model. Looked for when the page
/// opens.
private struct FoundRows: View {
    @Environment(MailModel.self) private var model
    let changed: () -> Void
    @State private var found: [FoundModel]?

    var body: some View {
        Section(tr("Found on This Computer")) {
            if let found {
                if found.isEmpty {
                    VStack(alignment: .leading) {
                        Text(tr("Nothing found"))
                        Text(tr("Start LM Studio's server or Ollama, or install Claude Code, then open Preferences again."))
                            .font(.caption).foregroundStyle(.secondary)
                    }
                }
                ForEach(found, id: \.label) { item in
                    HStack {
                        VStack(alignment: .leading) {
                            Text(item.label)
                            if let models = models(item) {
                                Text(models).font(.caption).foregroundStyle(.secondary)
                            }
                        }
                        Spacer()
                        Button(tr("Use")) {
                            model.changeAi { try $0.useFoundAi(found: item) }
                            changed()
                        }
                        .accessibilityLabel(tr("Use {provider}").replacingOccurrences(of: "{provider}", with: item.label))
                    }
                }
            } else {
                HStack {
                    Text(tr("Looking for LM Studio, Ollama, Unsloth, and Claude Code…")).foregroundStyle(.secondary)
                    Spacer()
                    ProgressView().controlSize(.small)
                }
            }
        }
        .task { found = await model.findAi() }
    }

    private func models(_ item: FoundModel) -> String? {
        guard let first = item.models.first else { return nil }
        let more = item.models.count - 1
        guard more > 0 else { return first }
        return trPlural("{model} and {count} more", "{model} and {count} more", more)
            .replacingOccurrences(of: "{model}", with: first)
            .replacingOccurrences(of: "{count}", with: "\(more)")
    }
}

/// How the assistant searches the web, with the engine's key or server
/// when a local model needs one.
private struct WebSearchRows: View {
    @Environment(MailModel.self) private var model
    let ai: AiPreferences
    let engines: [ChoiceItem]
    let changed: () -> Void
    @State private var braveKey = ""
    @State private var searxng = ""

    var body: some View {
        Section {
            Picker(tr("Search With"), selection: Binding(get: { ai.webSearch }, set: { engine in
                model.changeAi { try $0.setAiSetting(name: "web_search", value: engine) }
                changed()
            })) {
                ForEach(engines, id: \.key) { Text($0.label).tag($0.key) }
            }
            if ai.webSearch == "brave" {
                KeyField(title: ai.braveKeySaved ? tr("Brave Search API Key (Saved)") : tr("Brave Search API Key"),
                         name: "brave-search", value: $braveKey, changed: changed)
            }
            if ai.webSearch == "searxng" {
                TextField(tr("SearXNG Server Address"), text: $searxng)
                    .onSubmit {
                        model.changeAi { try $0.setAiSetting(name: "searxng_url", value: searxng) }
                        changed()
                    }
            }
        } header: {
            Text(tr("Web Search"))
        } footer: {
            Text(tr("Lets the assistant search the web and read pages. Claude searches with Anthropic's own tools. A local model searches with the engine chosen here, which receives every query."))
                .font(.caption)
                .foregroundStyle(.secondary)
        }
        .onAppear { searxng = ai.searxngUrl }
    }
}

/// Outside tools answered Always Allow, each with a way back to being
/// asked.
private struct AllowedRows: View {
    @Environment(MailModel.self) private var model
    let tools: [String]
    let changed: () -> Void

    var body: some View {
        Section {
            ForEach(tools, id: \.self) { key in
                let parts = key.split(separator: "/", maxSplits: 1).map(String.init)
                let tool = parts.last ?? key
                HStack {
                    VStack(alignment: .leading) {
                        Text(tool)
                        if parts.count > 1 { Text(parts[0]).font(.caption).foregroundStyle(.secondary) }
                    }
                    Spacer()
                    Button {
                        model.changeAi { try $0.setAiSetting(name: "forbid_tool", value: key) }
                        changed()
                    } label: {
                        Image(systemName: "trash")
                    }
                    .buttonStyle(.borderless)
                    .help(tr("Ask Again"))
                    .accessibilityLabel(tr("Ask again before {tool}").replacingOccurrences(of: "{tool}", with: tool))
                }
            }
        } header: {
            Text(tr("Always Allowed"))
        } footer: {
            Text(tr("Tools from outside sources that run without asking. Remove one to be asked again next time."))
                .font(.caption)
                .foregroundStyle(.secondary)
        }
    }
}
