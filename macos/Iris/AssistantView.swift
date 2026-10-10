import AppKit
import SwiftUI

/// The assistant's chat, as the pane on the right of the window shows it.
/// The Rust core runs the model and its tools; this model keeps what the
/// pane draws and carries the questions the tools ask.
@MainActor
@Observable
final class AssistantModel {
    /// One thing in the transcript.
    enum Entry: Identifiable {
        case question(UUID, String)
        case turn(UUID, TurnView)
        case note(UUID, String, problem: Bool)
        case approval(UUID, Approval)

        var id: UUID {
            switch self {
            case .question(let id, _), .turn(let id, _), .note(let id, _, _), .approval(let id, _): id
            }
        }
    }

    /// A question a tool asked, and the answer once given.
    struct Approval {
        let ask: UInt64
        let question: String
        let always: Bool
        var answer: ApprovalAnswer?
    }

    /// The Unsubscribe question: the lists, each ticked or not.
    struct Leaving: Identifiable {
        let id: UInt64
        let heading: String
        let body: String?
        var lines: [LeaveLine]
        var ticked: Set<Int>
    }

    var shown = false
    private(set) var entries: [Entry] = []
    private(set) var running = false
    var leaving: Leaving?
    /// The model in the pane's subtitle, or nil while the assistant is off.
    private(set) var modelName: String? = assistantModel()

    weak var mail: MailModel?
    private var assistant: Assistant?
    private var bridge: AssistantBridge?
    private var turn: UUID?

    /// Reads the model name again, after Settings changed.
    func refresh() {
        modelName = assistantModel()
    }

    func ask(_ text: String) {
        let text = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty, !running else { return }
        guard let assistant = started() else {
            // The store opens a moment after the window does.
            entries.append(.note(UUID(), tr("Loading…"), problem: false))
            return
        }
        if let mail { assistant.setScreen(screen: mail.assistantScreen) }
        entries.append(.question(UUID(), text))
        let id = UUID()
        turn = id
        entries.append(.turn(id, TurnView(steps: [], phase: tr("Waiting for the model…"))))
        running = true
        assistant.ask(text: text)
    }

    func stop() {
        assistant?.stop()
    }

    /// Starts over with an empty chat.
    func newChat() {
        guard !running else { return }
        entries = []
        assistant?.reset()
    }

    func answer(_ entry: UUID, _ answer: ApprovalAnswer) {
        guard let at = entries.firstIndex(where: { $0.id == entry }),
              case .approval(_, var approval) = entries[at], approval.answer == nil else { return }
        approval.answer = answer
        entries[at] = .approval(entry, approval)
        assistant?.answer(id: approval.ask, answer: answer)
    }

    /// Answers the Unsubscribe question with the lists ticked, or nothing.
    func leave(_ go: Bool) {
        guard let leaving else { return }
        self.leaving = nil
        assistant?.answerLeave(id: leaving.id, ticked: go ? leaving.ticked.sorted().map { UInt32($0) } : nil)
    }

    /// The assistant, started the first time it is needed, once the store
    /// is open.
    private func started() -> Assistant? {
        if let assistant { return assistant }
        let bridge = AssistantBridge(self)
        guard let made = mail?.makeAssistant(window: bridge) else { return nil }
        self.bridge = bridge
        assistant = made
        return made
    }

    // What the core says, on the main actor.

    fileprivate func turned(_ view: TurnView) {
        guard let turn, let at = entries.firstIndex(where: { $0.id == turn }) else { return }
        entries[at] = .turn(turn, view)
    }

    fileprivate func finished(_ problem: String?) {
        running = false
        turn = nil
        if let problem { entries.append(.note(UUID(), problem, problem: true)) }
    }

    fileprivate func asked(_ id: UInt64, _ question: String, always: Bool) {
        entries.append(.approval(UUID(), Approval(ask: id, question: question, always: always)))
    }

    fileprivate func leaveLists(_ id: UInt64, heading: String, body: String?, lines: [LeaveLine]) {
        leaving = Leaving(id: id, heading: heading, body: body, lines: lines, ticked: Set(lines.indices))
    }

    fileprivate func leaveLine(_ id: UInt64, _ index: Int, _ line: LeaveLine) {
        guard leaving?.id == id, let lines = leaving?.lines, lines.indices.contains(index) else { return }
        leaving?.lines[index] = line
    }
}

/// The core's way into the pane. Its calls come from the assistant's
/// thread, so each one hops to the main actor.
final class AssistantBridge: AssistantWindow, @unchecked Sendable {
    private weak var model: AssistantModel?

    @MainActor
    init(_ model: AssistantModel) {
        self.model = model
    }

    private func main(_ work: @escaping @MainActor (AssistantModel) -> Void) {
        Task { @MainActor [weak model] in
            if let model { work(model) }
        }
    }

    func turn(view: TurnView) { main { $0.turned(view) } }
    func finished(problem: String?) { main { $0.finished(problem) } }
    func ask(id: UInt64, question: String, always: Bool) { main { $0.asked(id, question, always: always) } }

    func leaveLists(id: UInt64, heading: String, body: String?, lines: [LeaveLine]) {
        main { $0.leaveLists(id, heading: heading, body: body, lines: lines) }
    }

    func leaveLine(id: UInt64, index: UInt32, line: LeaveLine) {
        main { $0.leaveLine(id, Int(index), line) }
    }

    func compose(draft: ComposeDraft) { main { $0.mail?.openComposer(draft) } }
    func send(draft: ComposeDraft) { main { $0.mail?.send(draft) } }
    func showThread(accountId: Int64, threadId: String) { main { $0.mail?.show(account: accountId, thread: threadId) } }

    func copy(text: String) {
        main { _ in
            NSPasteboard.general.clearContents()
            NSPasteboard.general.setString(text, forType: .string)
        }
    }

    func openUrl(url: String) {
        main { _ in
            if let link = URL(string: url) { NSWorkspace.shared.open(link) }
        }
    }

    func changed() { main { $0.mail?.refresh() } }

    func settingsChanged() {
        main {
            $0.mail?.readPrefs()
            $0.refresh()
        }
    }

    func say(text: String) { main { $0.mail?.say(text) } }
}

/// The pane: the chat, or how to set the assistant up while it is off.
struct AssistantPane: View {
    /// How wide the pane opens, which is also how much the window grows.
    static let width: CGFloat = 380

    @Environment(AssistantModel.self) private var assistant
    @Environment(\.openSettings) private var openSettings
    @State private var text = ""
    @FocusState private var typing: Bool

    var body: some View {
        VStack(spacing: 0) {
            header
            Divider()
            if assistant.modelName == nil {
                setup
            } else {
                transcript
                Divider()
                entry
            }
        }
        .toolbar {
            ToolbarSpacer(.flexible)
            ToolbarItem { AssistantButton() }
        }
        .onAppear {
            assistant.refresh()
            typing = true
        }
        .sheet(item: Binding(get: { assistant.leaving }, set: { if $0 == nil { assistant.leave(false) } })) { _ in
            LeaveSheet()
        }
    }

    private var header: some View {
        HStack {
            Button(tr("New Chat"), systemImage: "plus") { assistant.newChat() }
                .labelStyle(.iconOnly)
                .buttonStyle(.borderless)
                .help(tr("New Chat"))
                .disabled(assistant.running)
            Spacer()
            VStack(spacing: 0) {
                Text(tr("Assistant")).font(.headline)
                if let name = assistant.modelName {
                    Text(name).font(.caption).foregroundStyle(.secondary)
                }
            }
            Spacer()
            Color.clear.frame(width: 16, height: 1)
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 8)
    }

    private var setup: some View {
        VStack(spacing: 14) {
            Spacer()
            Image(systemName: "sparkles").font(.system(size: 40)).foregroundStyle(.secondary)
            Text(tr("Set Up the Assistant")).font(.title3.weight(.semibold))
            Text(tr("Use a local model from LM Studio, Ollama, or any server with OpenAI's API. You can also use an Anthropic API key or your Claude subscription."))
                .multilineTextAlignment(.center)
                .foregroundStyle(.secondary)
            Button(tr("Choose a Model")) {
                UserDefaults.standard.set("ai", forKey: SettingsView.tabKey)
                openSettings()
            }
            .buttonStyle(.borderedProminent)
            Spacer()
        }
        .padding(24)
    }

    private var transcript: some View {
        ScrollViewReader { scroller in
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 12) {
                    if assistant.entries.isEmpty {
                        Suggestions(ask: assistant.ask)
                    }
                    ForEach(assistant.entries) { entry in
                        EntryView(entry: entry).id(entry.id)
                    }
                }
                .padding(12)
            }
            .onChange(of: assistant.entries.last.map(\.id)) {
                if let last = assistant.entries.last?.id {
                    withAnimation(.snappy) { scroller.scrollTo(last, anchor: .bottom) }
                }
            }
        }
    }

    private var entry: some View {
        HStack(spacing: 8) {
            TextField(tr("Ask Iris…"), text: $text, axis: .vertical)
                .textFieldStyle(.plain)
                .lineLimit(1...6)
                .focused($typing)
                .disabled(assistant.running)
                .onSubmit(send)
                .accessibilityLabel(tr("Ask Iris"))
            Button {
                if assistant.running { assistant.stop() } else { send() }
            } label: {
                Image(systemName: assistant.running ? "stop.circle.fill" : "arrow.up.circle.fill")
                    .font(.title2)
            }
            .buttonStyle(.borderless)
            .help(assistant.running ? tr("Stop") : tr("Send"))
            .accessibilityLabel(assistant.running ? tr("Stop") : tr("Send"))
            .disabled(!assistant.running && text.trimmingCharacters(in: .whitespaces).isEmpty)
        }
        .padding(10)
    }

    private func send() {
        let asked = text
        text = ""
        assistant.ask(asked)
    }
}

/// Starting points for an empty chat, in the reader's language.
private struct Suggestions: View {
    let ask: (String) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(tr("Ask about your mail, or tell me what to tidy. I can search, summarize, sort, draft, and change settings."))
                .foregroundStyle(.secondary)
                .padding(.bottom, 4)
            ForEach([
                "Summarize this conversation",
                "What needs a reply today?",
                "Archive newsletters older than a week",
                "Set an out-of-office reply for next week",
                "Draft a reply to the open conversation",
            ], id: \.self) { words in
                Button(tr(words)) { ask(tr(words)) }
                    .buttonStyle(.bordered)
                    .controlSize(.regular)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

private struct EntryView: View {
    @Environment(AssistantModel.self) private var assistant
    let entry: AssistantModel.Entry

    var body: some View {
        switch entry {
        case .question(_, let text):
            Text(text)
                .textSelection(.enabled)
                .padding(.horizontal, 12)
                .padding(.vertical, 8)
                .background(Color.accentColor.opacity(0.15), in: RoundedRectangle(cornerRadius: 12))
                .frame(maxWidth: .infinity, alignment: .trailing)
        case .turn(_, let view):
            TurnBlock(view: view)
        case .note(_, let text, let problem):
            Text(text)
                .textSelection(.enabled)
                .foregroundStyle(problem ? Color.red : Color.secondary)
                .font(.callout)
        case .approval(let id, let approval):
            ApprovalCard(approval: approval) { assistant.answer(id, $0) }
        }
    }
}

/// One turn: what the model thought, each tool it ran, its reply, and
/// what it is doing now.
private struct TurnBlock: View {
    let view: TurnView

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            ForEach(Array(view.steps.enumerated()), id: \.offset) { _, step in
                StepView(step: step)
            }
            if let phase = view.phase {
                HStack(spacing: 6) {
                    ProgressView().controlSize(.small)
                    Text(phase).font(.callout).foregroundStyle(.secondary)
                }
            }
        }
    }
}

private struct StepView: View {
    let step: TurnStep
    @State private var open = false

    var body: some View {
        switch step.kind {
        case "reply":
            Text(markdown(step.text))
                .textSelection(.enabled)
                .frame(maxWidth: .infinity, alignment: .leading)
        default:
            DisclosureGroup(isExpanded: $open) {
                VStack(alignment: .leading, spacing: 6) {
                    if !step.input.isEmpty {
                        Text(step.input).font(.caption.monospaced()).foregroundStyle(.secondary)
                    }
                    if !step.text.isEmpty {
                        Text(step.text).font(.caption).textSelection(.enabled)
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.top, 4)
            } label: {
                HStack(spacing: 6) {
                    mark
                    Text(step.title).font(.callout.weight(.medium))
                    if !step.summary.isEmpty {
                        Text(step.summary).font(.callout).foregroundStyle(.secondary).lineLimit(1)
                    }
                }
            }
        }
    }

    @ViewBuilder
    private var mark: some View {
        switch (step.kind, step.state) {
        case (_, "working"): ProgressView().controlSize(.mini)
        case (_, "failed"): Image(systemName: "xmark.circle.fill").foregroundStyle(.red)
        case ("thinking", _): Image(systemName: "brain").foregroundStyle(.secondary)
        default: Image(systemName: "checkmark.circle.fill").foregroundStyle(.green)
        }
    }

    private func markdown(_ text: String) -> AttributedString {
        let options = AttributedString.MarkdownParsingOptions(interpretedSyntax: .inlineOnlyPreservingWhitespace)
        return (try? AttributedString(markdown: text, options: options)) ?? AttributedString(text)
    }
}

/// A question a tool asks before it acts.
private struct ApprovalCard: View {
    let approval: AssistantModel.Approval
    let answer: (ApprovalAnswer) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text(approval.question).textSelection(.enabled)
            if let given = approval.answer {
                Text(said(given)).font(.caption).foregroundStyle(.secondary)
            } else {
                HStack {
                    Spacer()
                    Button(tr("Don't Allow")) { answer(.deny) }
                    if approval.always {
                        Button(tr("Always Allow")) { answer(.always) }
                    }
                    Button(tr("Allow")) { answer(.once) }
                        .buttonStyle(.borderedProminent)
                        .keyboardShortcut(.defaultAction)
                }
            }
        }
        .padding(12)
        .background(.quaternary.opacity(0.5), in: RoundedRectangle(cornerRadius: 10))
    }

    private func said(_ given: ApprovalAnswer) -> String {
        switch given {
        case .once: tr("Allowed")
        case .always: tr("Always allowed")
        case .deny: tr("Not allowed")
        }
    }
}

/// The one question before Iris leaves mailing lists: each list, what
/// leaving it will do, and a tick to leave it out.
private struct LeaveSheet: View {
    @Environment(AssistantModel.self) private var assistant

    var body: some View {
        if let leaving = assistant.leaving {
            VStack(alignment: .leading, spacing: 14) {
                Text(leaving.heading).font(.headline)
                if let body = leaving.body {
                    Text(body).foregroundStyle(.secondary)
                }
                ForEach(Array(leaving.lines.enumerated()), id: \.offset) { at, line in
                    Toggle(isOn: Binding(
                        get: { assistant.leaving?.ticked.contains(at) ?? false },
                        set: { on in
                            if on { assistant.leaving?.ticked.insert(at) } else { assistant.leaving?.ticked.remove(at) }
                        }
                    )) {
                        VStack(alignment: .leading) {
                            Text(line.name)
                            HStack(spacing: 4) {
                                if line.reading { ProgressView().controlSize(.mini) }
                                Text(line.text).font(.caption).foregroundStyle(.secondary)
                            }
                        }
                    }
                }
                HStack {
                    Spacer()
                    Button(tr("Cancel")) { assistant.leave(false) }
                        .keyboardShortcut(.cancelAction)
                    Button(tr("Unsubscribe")) { assistant.leave(true) }
                        .keyboardShortcut(.defaultAction)
                        .disabled(leaving.lines.contains(where: \.reading) || leaving.ticked.isEmpty)
                }
            }
            .padding(20)
            .frame(width: 440)
        }
    }
}

/// Opens and closes the assistant's pane. It is declared on the pane,
/// and macOS keeps an inspector's toolbar items at the toolbar's far
/// right whether the pane is open or closed, so it always comes last.
struct AssistantButton: View {
    @Environment(AssistantModel.self) private var assistant

    var body: some View {
        Button(tr("Assistant"), systemImage: "sparkles") { assistant.shown.toggle() }
            .help(tr("Assistant"))
    }
}
