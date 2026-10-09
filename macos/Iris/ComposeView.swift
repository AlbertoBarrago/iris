import SwiftUI
import UniformTypeIdentifiers
import WebKit

/// What a composer window opens on: a new message, a reply, a forward, or
/// a message that Undo Send brought back.
struct ComposeRequest: Codable, Hashable {
    enum Kind: String, Codable { case new, reply, replyAll, forward, restore }
    var kind: Kind
    var account: Int64?
    var thread: String?
    /// The message Undo Send kept, by its key in the model.
    var restore: UUID?
}

/// One message being written, in a window of its own as in Mail.
struct ComposeView: View {
    @Environment(MailModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let request: ComposeRequest

    @State private var draft: ComposeDraft?
    /// The draft as it opened or was last saved, to tell whether closing
    /// would lose anything.
    @State private var saved: ComposeDraft?
    @State private var senders: [Sender] = []
    @State private var showCopies = false
    @State private var showQuote = false
    @State private var problem: String?
    @State private var editor = RichController()

    var body: some View {
        Group {
            if let binding = Binding($draft) {
                form(binding)
            } else if let problem {
                ContentUnavailableView(problem, systemImage: "exclamationmark.triangle")
            } else {
                ProgressView().frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        .frame(minWidth: 560, minHeight: 420)
        .background(WindowCloseGuard(
            mustAsk: { draft != nil && draft != saved },
            save: { await saveAndClose() },
            discard: { discard() }
        ))
        .navigationTitle(draft.map { $0.subject.isEmpty ? tr("New Message") : $0.subject } ?? tr("New Message"))
        .task { await load() }
        .onDrop(of: [.fileURL], isTargeted: nil) { providers in
            for provider in providers {
                _ = provider.loadObject(ofClass: URL.self) { url, _ in
                    if let url { Task { @MainActor in attach([url]) } }
                }
            }
            return true
        }
        .toolbar {
            ToolbarItem {
                Button(macKeys(tr("Attach Files (Ctrl+Shift+A)")), systemImage: "paperclip") { pickFiles() }
                    .keyboardShortcut("a", modifiers: [.command, .shift])
                    .disabled(draft == nil)
            }
            ToolbarItem(placement: .primaryAction) {
                Button(tr("Send"), systemImage: "paperplane.fill") { send() }
                    .keyboardShortcut(.return, modifiers: .command)
                    .disabled(draft == nil)
            }
        }
    }

    private func form(_ draft: Binding<ComposeDraft>) -> some View {
        VStack(spacing: 0) {
            Grid(alignment: .leading, horizontalSpacing: 10, verticalSpacing: 8) {
                if senders.count > 1 {
                    GridRow {
                        label(tr("From"))
                        Picker("", selection: Binding(
                            get: { "\(draft.wrappedValue.accountId)|\(draft.wrappedValue.from)" },
                            set: { picked in
                                let parts = picked.split(separator: "|", maxSplits: 1).map(String.init)
                                if parts.count == 2, let account = Int64(parts[0]) {
                                    draft.wrappedValue.accountId = account
                                    draft.wrappedValue.from = parts[1]
                                }
                            }
                        )) {
                            ForEach(senders, id: \.email) { sender in
                                Text(sender.name.map { "\($0) <\(sender.email)>" } ?? sender.email)
                                    .tag("\(sender.accountId)|\(sender.email)")
                            }
                        }
                        .labelsHidden()
                    }
                }
                GridRow(alignment: .top) {
                    label(tr("To"))
                    HStack(alignment: .top) {
                        RecipientField(placeholder: tr("Recipients"), text: draft.to, account: draft.wrappedValue.accountId)
                        Button(tr("Cc/Bcc")) { showCopies.toggle() }
                            .buttonStyle(.borderless)
                            .foregroundStyle(.secondary)
                    }
                }
                if showCopies || !draft.wrappedValue.cc.isEmpty || !draft.wrappedValue.bcc.isEmpty {
                    GridRow(alignment: .top) {
                        label(tr("Cc"))
                        RecipientField(placeholder: "", text: draft.cc, account: draft.wrappedValue.accountId)
                    }
                    GridRow(alignment: .top) {
                        label(tr("Bcc"))
                        RecipientField(placeholder: "", text: draft.bcc, account: draft.wrappedValue.accountId)
                    }
                }
                GridRow {
                    label(tr("Subject"))
                    TextField("", text: draft.subject).textFieldStyle(.plain).font(.headline)
                }
            }
            .padding(14)
            Divider()
            FormatBar(controller: editor)
            Divider()
            ScrollView {
                VStack(alignment: .leading, spacing: 12) {
                    RichEditor(
                        blocks: Binding(get: { draft.wrappedValue.rich ?? [] }, set: { draft.wrappedValue.rich = $0 }),
                        controller: editor
                    )
                    .frame(minHeight: 200)
                    if !draft.wrappedValue.attachments.isEmpty {
                        FlowLayout(spacing: 8) {
                            ForEach(Array(draft.wrappedValue.attachments.enumerated()), id: \.offset) { index, file in
                                HStack(spacing: 6) {
                                    Image(systemName: "doc")
                                    Text(file.filename).lineLimit(1)
                                    Text(ByteCountFormatter.string(fromByteCount: Int64(file.data.count), countStyle: .file))
                                        .foregroundStyle(.secondary)
                                    Button(tr("Remove {file}").replacingOccurrences(of: "{file}", with: file.filename), systemImage: "xmark") {
                                        draft.wrappedValue.attachments.remove(at: index)
                                    }
                                    .labelStyle(.iconOnly)
                                    .buttonStyle(.borderless)
                                }
                                .padding(.horizontal, 10)
                                .padding(.vertical, 5)
                                .background(Capsule().fill(Color.secondary.opacity(0.15)))
                            }
                        }
                    }
                    if let signature = draft.wrappedValue.signatureHtml {
                        SignaturePreview(html: signature)
                            .frame(height: 180)
                    }
                    if let quoted = draft.wrappedValue.quoted {
                        HStack {
                            Button("•••") { showQuote.toggle() }
                                .help(tr("Show trimmed content"))
                            Button(tr("Remove Quoted Text"), systemImage: "xmark") {
                                draft.wrappedValue.quoted = nil
                            }
                            .labelStyle(.iconOnly)
                            .buttonStyle(.borderless)
                            .help(tr("Remove Quoted Text"))
                        }
                        if showQuote {
                            Text(quoted)
                                .foregroundStyle(.secondary)
                                .textSelection(.enabled)
                                .frame(maxWidth: .infinity, alignment: .leading)
                        }
                    }
                    if let forwarded = draft.wrappedValue.forwardedHtml {
                        Text(tr("Forwarded Message")).font(.caption).foregroundStyle(.secondary)
                        SignaturePreview(html: forwarded).frame(height: 280)
                    }
                }
                .padding(14)
            }
        }
    }

    private func label(_ text: String) -> some View {
        Text(text)
            .foregroundStyle(.secondary)
            .frame(width: 64, alignment: .trailing)
            .padding(.top, 1)
    }

    private func load() async {
        do {
            senders = try await model.senders()
            if request.kind == .restore, let key = request.restore {
                draft = model.takeKept(key)
                return
            }
            draft = try await model.startDraft(request)
            saved = draft
        } catch {
            problem = "\(error)"
        }
    }

    /// Picks files to attach with the Mac's open panel.
    private func pickFiles() {
        let panel = NSOpenPanel()
        panel.allowsMultipleSelection = true
        panel.canChooseDirectories = false
        panel.prompt = tr("Attach Files")
        guard panel.runModal() == .OK else { return }
        attach(panel.urls)
    }

    /// Adds files to the message, each with the type the Mac knows it by.
    private func attach(_ urls: [URL]) {
        for url in urls {
            guard let data = try? Data(contentsOf: url) else { continue }
            let type = UTType(filenameExtension: url.pathExtension)?.preferredMIMEType ?? "application/octet-stream"
            draft?.attachments.append(OutgoingFile(filename: url.lastPathComponent, mimeType: type, data: data, contentId: nil))
        }
    }

    private func send() {
        guard let draft else { return }
        model.send(draft)
        saved = draft
        dismiss()
    }

    /// Saves the draft on the server and closes, or says why it could not.
    /// True when the window may close.
    private func saveAndClose() async -> Bool {
        guard let draft else { return true }
        do {
            let kept = try await model.saveDraft(draft)
            self.draft = kept
            saved = kept
            model.say(tr("Draft saved"))
            return true
        } catch {
            problem = nil
            model.say("\(error)")
            return false
        }
    }

    /// Lets the message go, and the draft saved of it before.
    private func discard() {
        if let draft { model.discardDraft(draft) }
        saved = draft
    }
}

/// HTML shown read-only in the composer: a formatted signature, or the
/// message a forward carries. It loads nothing from the network.
struct SignaturePreview: NSViewRepresentable {
    let html: String

    func makeNSView(context: Context) -> WKWebView {
        let configuration = WKWebViewConfiguration()
        configuration.websiteDataStore = .nonPersistent()
        configuration.defaultWebpagePreferences.allowsContentJavaScript = false
        let view = WKWebView(frame: .zero, configuration: configuration)
        view.setValue(false, forKey: "drawsBackground")
        return view
    }

    func updateNSView(_ view: WKWebView, context: Context) {
        let page = """
        <!doctype html><html><head><meta charset="utf-8">
        <meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'unsafe-inline'; img-src data: https:">
        <style>body{margin:0;font:13px -apple-system,sans-serif;color:#1d1d1f;background:#fff;padding:10px;border-radius:8px}</style>
        </head><body>\(html)</body></html>
        """
        view.loadHTMLString(page, baseURL: nil)
    }
}

/// A recipient field that offers the people the store knows as the person
/// types, as the GTK composer does: arrows move through them, Return or a
/// click takes one, Escape puts them away.
struct RecipientField: View {
    @Environment(MailModel.self) private var model
    let placeholder: String
    @Binding var text: String
    let account: Int64

    @State private var offered: [Suggestion] = []
    @State private var chosen = 0
    @FocusState private var focused: Bool

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            TextField(placeholder, text: $text)
                .textFieldStyle(.plain)
                .focused($focused)
                .onChange(of: text) { look() }
                .onKeyPress(.downArrow) { move(1) }
                .onKeyPress(.upArrow) { move(-1) }
                .onKeyPress(.escape) {
                    guard !offered.isEmpty else { return .ignored }
                    offered = []
                    return .handled
                }
                .onSubmit { take(chosen) }
            if focused, !offered.isEmpty {
                VStack(alignment: .leading, spacing: 0) {
                    ForEach(Array(offered.enumerated()), id: \.offset) { index, person in
                        Button { take(index) } label: {
                            HStack(spacing: 8) {
                                Text(person.name ?? person.email).fontWeight(.semibold)
                                if person.name != nil {
                                    Text(person.email).foregroundStyle(.secondary)
                                }
                                Spacer()
                            }
                            .padding(.horizontal, 8)
                            .padding(.vertical, 4)
                            .background(RoundedRectangle(cornerRadius: 5).fill(index == chosen ? Color.accentColor.opacity(0.25) : .clear))
                            .contentShape(Rectangle())
                        }
                        .buttonStyle(.plain)
                    }
                }
                .padding(4)
                .background(RoundedRectangle(cornerRadius: 8).fill(.background).shadow(radius: 3, y: 1))
            }
        }
    }

    private func look() {
        let field = text
        Task {
            let found = await model.suggest(field, account: account)
            if text == field {
                offered = found
                chosen = 0
            }
        }
    }

    private func move(_ step: Int) -> KeyPress.Result {
        guard !offered.isEmpty else { return .ignored }
        chosen = (chosen + step + offered.count) % offered.count
        return .handled
    }

    private func take(_ index: Int) {
        guard offered.indices.contains(index) else { return }
        text = offered[index].completed
        offered = []
    }
}

/// Asks before a composer window closes on unsaved words, by the red
/// button or Command-W, as the GTK composer asks: Save Draft, Discard or
/// Cancel. It stands in as the window's delegate and hands everything
/// else on to the one SwiftUI set.
struct WindowCloseGuard: NSViewRepresentable {
    let mustAsk: () -> Bool
    let save: () async -> Bool
    let discard: () -> Void

    func makeNSView(context: Context) -> NSView {
        let view = NSView()
        DispatchQueue.main.async {
            guard let window = view.window, !(window.delegate is CloseGuardDelegate) else { return }
            let guardian = CloseGuardDelegate(original: window.delegate, guardView: self)
            context.coordinator.guardian = guardian
            window.delegate = guardian
        }
        return view
    }

    func updateNSView(_ view: NSView, context: Context) {
        context.coordinator.guardian?.guardView = self
    }

    func makeCoordinator() -> Coordinator { Coordinator() }

    final class Coordinator {
        var guardian: CloseGuardDelegate?
    }
}

final class CloseGuardDelegate: NSObject, NSWindowDelegate {
    weak var original: NSWindowDelegate?
    var guardView: WindowCloseGuard
    /// Set once the person chose, so the close that follows goes through.
    private var settled = false

    init(original: NSWindowDelegate?, guardView: WindowCloseGuard) {
        self.original = original
        self.guardView = guardView
    }

    func windowShouldClose(_ window: NSWindow) -> Bool {
        if settled || !guardView.mustAsk() {
            return original?.windowShouldClose?(window) ?? true
        }
        let alert = NSAlert()
        alert.messageText = tr("Save as Draft?")
        alert.informativeText = tr("The draft is kept in Gmail, so you can finish it later on any device.")
        alert.addButton(withTitle: tr("Save Draft"))
        alert.addButton(withTitle: tr("Discard"))
        alert.addButton(withTitle: tr("Cancel"))
        alert.buttons[1].hasDestructiveAction = true
        alert.beginSheetModal(for: window) { answer in
            switch answer {
            case .alertFirstButtonReturn:
                Task { @MainActor in
                    if await self.guardView.save() {
                        self.settled = true
                        window.performClose(nil)
                    }
                }
            case .alertSecondButtonReturn:
                self.guardView.discard()
                self.settled = true
                window.performClose(nil)
            default:
                break
            }
        }
        return false
    }

    // Everything else goes to the delegate SwiftUI set.
    override func responds(to selector: Selector!) -> Bool {
        super.responds(to: selector) || (original?.responds(to: selector) ?? false)
    }

    override func forwardingTarget(for selector: Selector!) -> Any? {
        original?.responds(to: selector) == true ? original : super.forwardingTarget(for: selector)
    }
}
