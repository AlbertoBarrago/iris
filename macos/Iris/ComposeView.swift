import SwiftUI
import UniformTypeIdentifiers
import WebKit

/// What a composer window opens on: a new message, a reply, a forward, or
/// a message that Undo Send brought back.
struct ComposeRequest: Codable, Hashable {
    enum Kind: String, Codable { case new, reply, replyAll, forward, restore, editDraft }
    /// Makes every request its own. A window group shows one window per
    /// value, so without it a second New Message brought back the first,
    /// and a closed composer's window could come back holding its old draft.
    var id = UUID()
    var kind: Kind
    var account: Int64?
    var thread: String?
    /// The message of `thread` a reply answers, picked on its header; the
    /// toolbar leaves it out and the newest is answered.
    var message: String?
    /// The message Undo Send kept, by its key in the model.
    var restore: UUID?
    /// Who a new message is to, as the recipient field reads.
    var to: String?
    /// A `mailto:` link a new message starts from, with its recipients,
    /// subject and body.
    var mailto: String?
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
    /// The Bcc row shows, picked in the header fields menu; until then it
    /// shows when the draft has a Bcc.
    @State private var showBcc: Bool?
    /// Whether the sender's signature is in the message; `nil` when the
    /// address has none, which hides the Signature menu.
    @State private var signatureShown: Bool?
    /// The formatting bar shows under the fields, as Mail's Aa button
    /// leaves it. A per-person convenience, so the defaults keep it.
    @AppStorage("composeShowsFormatBar") private var showsFormatBar = true
    @State private var showQuote = false
    /// Send Later's "Choose a Time…" is open.
    @State private var choosingTime = false
    /// The time picked there, sent once its sheet has gone: closing the
    /// composer while the sheet shows would close the sheet instead.
    @State private var pickedTime: Int64?
    @State private var problem: String?
    /// Why Send did nothing, shown under the recipients until they change.
    @State private var sendProblem: String?
    /// A drag carrying files is over the words, or over the rest of the window.
    @State private var droppingOnText = false
    @State private var droppingOnWindow = false
    @State private var editor = RichController()
    @State private var templates: [TemplateItem] = []
    /// What Sign and Encrypt can do for the recipients as they stand.
    @State private var protection: ProtectionState?
    @State private var savingTemplate = false
    @State private var templateName = ""

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
            mustAsk: { mustAskToSave },
            save: { await saveAndClose() },
            discard: { discard() }
        ))
        .navigationTitle(draft.map { $0.subject.isEmpty ? tr("New Message") : $0.subject } ?? tr("New Message"))
        // Mail shows no title over its composer's toolbar; the window
        // keeps it for the Window menu and Mission Control.
        .toolbar(removing: .title)
        .background { FormatShortcuts(controller: editor) }
        .task { await load() }
        .onAppear {
            editor.dropped = { urls, failed in attach(urls, failed: failed) }
            editor.hovering = { droppingOnText = $0 }
        }
        // The words take their own drops; this takes the rest of the window.
        .onDrop(of: [.fileURL, .image], isTargeted: $droppingOnWindow) { providers in
            dropped(providers)
            return true
        }
        .overlay {
            if droppingOnText || droppingOnWindow {
                RoundedRectangle(cornerRadius: 10)
                    .strokeBorder(Color.accentColor, lineWidth: 3)
                    .overlay(alignment: .bottom) {
                        Text(tr("Drop to attach"))
                            .padding(.horizontal, 12)
                            .padding(.vertical, 6)
                            .background(.regularMaterial, in: Capsule())
                            .padding(16)
                    }
                    .padding(4)
                    .allowsHitTesting(false)
            }
        }
        .sheet(isPresented: $choosingTime, onDismiss: {
            if let at = pickedTime {
                pickedTime = nil
                sendLater(at)
            }
        }) {
            TimePickerSheet(
                title: tr("Send Later"),
                explanation: tr("Iris sends it at this time while it runs, even in the tray."),
                confirm: tr("Schedule")
            ) { at in pickedTime = at }
        }
        .alert(tr("Save as Template"), isPresented: $savingTemplate) {
            TextField(tr("Name"), text: $templateName)
            Button(tr("Cancel"), role: .cancel) {}
            Button(tr("Save")) { saveTemplate() }
                .keyboardShortcut(.defaultAction)
        } message: {
            Text(tr("Placeholders such as {{first_name}} fill in each time you use it."))
        }
        .onReceive(NotificationCenter.default.publisher(for: .templatesChanged)) { _ in loadTemplates() }
        .onChange(of: recipientsKey) { sendProblem = nil }
        .task(id: senderKey) { readSignature() }
        .task(id: recipientsKey) {
            // Asked once the typing pauses, as the GTK composer does.
            try? await Task.sleep(for: .milliseconds(400))
            guard !Task.isCancelled else { return }
            checkKeys()
        }
        .toolbar { toolbar }
    }

    /// The composer's toolbar, laid out as Mail's: the writing tools, the
    /// header fields, attaching, sending later, then Send on its own at
    /// the far end.
    @ToolbarContentBuilder
    private var toolbar: some ToolbarContent {
        ToolbarItemGroup(placement: .navigation) {
            Button(tr("Formatting"), systemImage: "textformat") { showsFormatBar.toggle() }
                .help(tr("Formatting"))
                .accessibilityAddTraits(showsFormatBar ? .isSelected : [])
            Button(tr("Emoji & Symbols"), systemImage: "face.smiling") {
                focusWords()
                NSApp.orderFrontCharacterPalette(nil)
            }
            .help(tr("Emoji & Symbols"))
            Button(tr("Writing Tools"), systemImage: "apple.writing.tools") {
                focusWords()
                NSApp.sendAction(#selector(NSResponder.showWritingTools(_:)), to: nil, from: nil)
            }
            .help(tr("Writing Tools"))
        }
        ToolbarItem(placement: .navigation) {
            Menu {
                Toggle(tr("Bcc Field"), isOn: Binding(get: { bccVisible }, set: { showBcc = $0 }))
            } label: {
                Label(tr("Header Fields"), systemImage: "list.bullet.rectangle")
            }
            .help(tr("Header Fields"))
            .disabled(draft == nil)
        }
        ToolbarItem(placement: .navigation) {
            Menu {
                Button(tr("Attach Files…")) { pickFiles() }
                Divider()
                if templates.isEmpty {
                    Button(tr("No Templates Yet")) {}.disabled(true)
                }
                ForEach(templates, id: \.id) { template in
                    Button(template.name) { insertTemplate(template.id) }
                }
                Divider()
                Button(tr("Save as Template…")) {
                    templateName = draft?.subject ?? ""
                    savingTemplate = true
                }
            } label: {
                Label(macKeys(tr("Attach Files (Ctrl+Shift+A)")), systemImage: "paperclip")
            } primaryAction: {
                pickFiles()
            }
            .help(macKeys(tr("Attach Files (Ctrl+Shift+A)")))
            .keyboardShortcut("a", modifiers: [.command, .shift])
            .disabled(draft == nil)
        }
        ToolbarItem(placement: .navigation) {
            Menu {
                ForEach(model.sendLaterTimes(), id: \.at) { time in
                    Button(time.label) { sendLater(time.at) }
                }
                Divider()
                Button(tr("Choose a Time…")) { if readyToSend() { choosingTime = true } }
            } label: {
                Label(tr("Send Later"), systemImage: "clock")
            }
            .help(tr("Send Later"))
            .disabled(draft == nil)
        }
        ToolbarItem(placement: .navigation) {
            // Sign and Encrypt, as the GTK composer's More menu has them.
            // Encrypt works only once every recipient has a key or
            // certificate here, and says why not otherwise.
            Menu {
                Toggle(tr("Sign"), isOn: Binding(
                    get: { draft?.sign ?? false },
                    set: { draft?.sign = $0 }
                ))
                .disabled(!(protection?.canSign ?? false))
                Toggle(tr("Encrypt"), isOn: Binding(
                    get: { draft?.encrypt ?? false },
                    set: { draft?.encrypt = $0 }
                ))
                .disabled(!(protection?.canEncrypt ?? false) && !(draft?.encrypt ?? false))
                if let note = protection?.encryptNote, !note.isEmpty {
                    Divider()
                    Text(note)
                }
            } label: {
                Label(shieldWords, systemImage: shieldSymbol)
            }
            .help(shieldWords)
            .disabled(draft == nil)
        }
        ToolbarItem(placement: .primaryAction) {
            Button(tr("Send"), systemImage: "arrow.up") { send() }
                .labelStyle(.iconOnly)
                .buttonStyle(.glassProminent)
                .buttonBorderShape(.circle)
                .help(tr("Send"))
                .keyboardShortcut(.return, modifiers: .command)
                .disabled(draft == nil)
        }
    }

    private func form(_ draft: Binding<ComposeDraft>) -> some View {
        VStack(spacing: 0) {
            addressRow(tr("To:")) {
                RecipientField(placeholder: "", text: draft.to, account: draft.wrappedValue.accountId)
                    .accessibilityLabel(tr("To"))
            }
            if let sendProblem {
                Text(sendProblem)
                    .font(.callout)
                    .foregroundStyle(.red)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, 14)
                    .padding(.vertical, 6)
                Divider()
            }
            addressRow(tr("Cc:")) {
                RecipientField(placeholder: "", text: draft.cc, account: draft.wrappedValue.accountId)
                    .accessibilityLabel(tr("Cc"))
            }
            if bccVisible {
                addressRow(tr("Bcc:")) {
                    RecipientField(placeholder: "", text: draft.bcc, account: draft.wrappedValue.accountId)
                        .accessibilityLabel(tr("Bcc"))
                }
            }
            addressRow(tr("Subject:")) {
                TextField("", text: draft.subject)
                    .textFieldStyle(.plain)
                    .accessibilityLabel(tr("Subject"))
            }
            addressRow(tr("From:")) {
                fromField(draft)
                Spacer(minLength: 12)
                if let shown = signatureShown {
                    Text(tr("Signature:"))
                        .foregroundStyle(.secondary)
                    Picker(tr("Signature"), selection: Binding(get: { shown }, set: { showSignature($0) })) {
                        Text(tr("None")).tag(false)
                        Text(tr("Account Signature")).tag(true)
                    }
                    .labelsHidden()
                    .pickerStyle(.menu)
                    .controlSize(.small)
                    .fixedSize()
                    .help(tr("Signature"))
                }
            }
            if showsFormatBar {
                FormatBar(controller: editor)
                Divider()
            }
            ScrollView {
                VStack(alignment: .leading, spacing: 12) {
                    RichEditor(
                        blocks: Binding(get: { draft.wrappedValue.rich ?? [] }, set: { draft.wrappedValue.rich = $0 }),
                        controller: editor
                    )
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
                                    .help(tr("Remove {file}").replacingOccurrences(of: "{file}", with: file.filename))
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
                .padding(.horizontal, 16)
                .padding(.vertical, 12)
            }
        }
    }

    /// Who the message comes from: a menu when there is more than one
    /// address to send from, as Mail's From field.
    @ViewBuilder
    private func fromField(_ draft: Binding<ComposeDraft>) -> some View {
        if senders.count > 1 {
            Picker(tr("From"), selection: Binding(
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
                    Text(senderName(sender)).tag("\(sender.accountId)|\(sender.email)")
                }
            }
            .labelsHidden()
            .pickerStyle(.menu)
            .buttonStyle(.borderless)
            .fixedSize()
        } else {
            Text(senders.first.map(senderName) ?? draft.wrappedValue.from)
                .textSelection(.enabled)
        }
    }

    private func senderName(_ sender: Sender) -> String {
        sender.name.map { "\($0) <\(sender.email)>" } ?? sender.email
    }

    private var bccVisible: Bool {
        showBcc ?? !(draft?.bcc.isEmpty ?? true)
    }

    /// One header line as Mail draws it: the label and its field on one
    /// row, a hairline under it from edge to edge.
    @ViewBuilder
    private func addressRow<Content: View>(
        _ title: String, @ViewBuilder content: () -> Content
    ) -> some View {
        HStack(alignment: .firstTextBaseline, spacing: 6) {
            Text(title)
                .foregroundStyle(.secondary)
                .fixedSize()
            content()
        }
        .frame(maxWidth: .infinity, minHeight: 39, alignment: .leading)
        .padding(.horizontal, 14)
        Divider()
    }

    /// Whether closing must ask to save: only when something was written
    /// since the composer opened or last saved, which the core decides. A
    /// message Undo Send brought back was never saved, so it always asks.
    private var mustAskToSave: Bool {
        guard let draft else { return false }
        guard let saved else { return true }
        return model.mail?.wouldLose(opened: saved, current: draft) ?? (draft != saved)
    }

    /// The address the message comes from, so the Signature menu reads
    /// again when it changes.
    private var senderKey: String {
        guard let draft else { return "" }
        return "\(draft.accountId)|\(draft.from)"
    }

    private func readSignature() {
        guard let draft else { return }
        signatureShown = model.mail?.signatureShown(composed: draft)
    }

    /// Puts the sender's signature in or takes it out, through the core,
    /// and gives the words back to the editor as one step Undo takes back.
    private func showSignature(_ shown: Bool) {
        guard let mail = model.mail, let current = draft else { return }
        do {
            let signed = try mail.showSignature(composed: current, shown: shown)
            draft = signed
            editor.replaceAll(signed.rich ?? [])
            signatureShown = shown
        } catch {
            model.say("\(error)")
        }
    }

    /// Puts the cursor in the words, where emoji and Writing Tools act.
    private func focusWords() {
        guard let text = editor.text else { return }
        text.window?.makeFirstResponder(text)
    }

    private func load() async {
        loadTemplates()
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
    private var recipientsKey: String {
        guard let draft else { return "" }
        return [draft.to, draft.cc, draft.bcc].joined(separator: "|")
    }

    /// Asks what gpg and gpgsm hold for the recipients. Preferences may
    /// say to encrypt whenever everyone can read it.
    private func checkKeys() {
        guard let mail = model.mail, let draft else { return }
        let (to, cc, bcc) = (draft.to, draft.cc, draft.bcc)
        Task.detached {
            let state = mail.protectionState(to: to, cc: cc, bcc: bcc)
            await MainActor.run {
                protection = state
                if state.encryptWhenPossible, state.canEncrypt, self.draft?.encrypt == false {
                    self.draft?.encrypt = true
                }
                if !state.canEncrypt, self.draft?.encrypt == true, state.encryptWhenPossible {
                    self.draft?.encrypt = false
                }
            }
        }
    }

    private var shieldWords: String {
        switch (draft?.sign ?? false, draft?.encrypt ?? false) {
        case (true, true): tr("Signed and encrypted")
        case (false, true): tr("Encrypted")
        case (true, false): tr("Signed")
        default: tr("Encrypt")
        }
    }

    private var shieldSymbol: String {
        switch (draft?.sign ?? false, draft?.encrypt ?? false) {
        case (_, true): "lock.fill"
        case (true, false): "checkmark.seal"
        default: "lock.open"
        }
    }

    private func loadTemplates() {
        templates = (try? model.mail?.templates()) ?? []
    }

    /// Puts a template in at the cursor, its placeholders filled for this
    /// message. One with a subject gives it to a message that has none.
    private func insertTemplate(_ id: Int64) {
        guard let mail = model.mail, let current = draft else { return }
        do {
            let filled = try mail.fillTemplate(id: id, to: current.to, subject: current.subject)
            if let subject = filled.subject { draft?.subject = subject }
            editor.insert(filled.blocks)
        } catch {
            model.say("\(error)")
        }
    }

    /// Keeps what is written as a template under the name given.
    private func saveTemplate() {
        guard let mail = model.mail, let current = draft else { return }
        do {
            _ = try mail.saveDraftAsTemplate(name: templateName, subject: current.subject, blocks: current.rich ?? [])
            model.say(tr("Template saved"))
            NotificationCenter.default.post(name: .templatesChanged, object: nil)
        } catch {
            model.say("\(error)")
        }
    }

    private func pickFiles() {
        let panel = NSOpenPanel()
        panel.allowsMultipleSelection = true
        panel.canChooseDirectories = false
        panel.prompt = tr("Attach Files")
        guard panel.runModal() == .OK else { return }
        attach(panel.urls)
    }

    /// Adds files to the message, each with the type the Mac knows it by,
    /// and says how many could not be read, `failed` among them, rather than
    /// leave one out unsaid.
    private func attach(_ urls: [URL], failed: Int = 0) {
        var missed = failed
        var added = 0
        for url in urls {
            guard let data = try? Data(contentsOf: url) else {
                missed += 1
                continue
            }
            let type = UTType(filenameExtension: url.pathExtension)?.preferredMIMEType ?? "application/octet-stream"
            draft?.attachments.append(OutgoingFile(filename: url.lastPathComponent, mimeType: type, data: data, contentId: nil))
            added += 1
        }
        if missed > 0 {
            model.say(trPlural("Could not read {count} file", "Could not read {count} files", missed)
                .replacingOccurrences(of: "{count}", with: "\(missed)"))
        }
        if added > 0 {
            // The files show beside the words; VoiceOver hears it here.
            let words = trPlural("Attached {count} file", "Attached {count} files", added)
                .replacingOccurrences(of: "{count}", with: "\(added)")
            NSAccessibility.post(
                element: NSApp.keyWindow as Any,
                notification: .announcementRequested,
                userInfo: [.announcement: words, .priority: NSAccessibilityPriorityLevel.medium.rawValue]
            )
        }
    }

    /// Files and pictures dropped outside the words: a file by its place on
    /// disk, a picture dragged as data written to a file first.
    private func dropped(_ providers: [NSItemProvider]) {
        for provider in providers {
            if provider.hasItemConformingToTypeIdentifier(UTType.fileURL.identifier) {
                _ = provider.loadObject(ofClass: URL.self) { url, _ in
                    Task { @MainActor in attach(url.map { [$0] } ?? [], failed: url == nil ? 1 : 0) }
                }
            } else {
                let name = tr("Image")
                _ = provider.loadDataRepresentation(for: .png) { data, _ in
                    let url = data.flatMap { FileDrop.image($0, named: name) }
                    Task { @MainActor in attach(url.map { [$0] } ?? [], failed: url == nil ? 1 : 0) }
                }
            }
        }
    }

    /// Keeps the message to go out at `at`, and closes.
    private func sendLater(_ at: Int64) {
        guard let draft, readyToSend() else { return }
        model.sendLater(draft, at: at)
        saved = draft
        dismiss()
    }

    /// Says why the message cannot go, under the recipients, and keeps the
    /// composer open to fix it. The core decides; true when it can go.
    private func readyToSend() -> Bool {
        guard let draft, let problem = model.mail?.sendProblem(composed: draft) else { return true }
        sendProblem = problem
        NSAccessibility.post(
            element: NSApp.keyWindow as Any,
            notification: .announcementRequested,
            userInfo: [.announcement: problem, .priority: NSAccessibilityPriorityLevel.high.rawValue]
        )
        return false
    }

    private func send() {
        guard let draft, readyToSend() else { return }
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
    /// Addresses not to offer again, such as the guests already invited.
    var leaving: Set<String> = []
    /// Called once a suggestion is taken, or Return is pressed with none
    /// offered, for a field whose words become a list at once.
    var taken: (() -> Void)?

    @State private var offered: [Suggestion] = []
    @State private var chosen = 0
    @State private var lookup: Task<Void, Never>?
    @FocusState private var focused: Bool

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            TextField(placeholder, text: $text)
                .textFieldStyle(.plain)
                .focused($focused)
                .onChange(of: text) { look() }
                .onChange(of: account) { look() }
                .onChange(of: leaving) { look() }
                .onKeyPress(.downArrow) { move(1) }
                .onKeyPress(.upArrow) { move(-1) }
                .onKeyPress(.escape) {
                    lookup?.cancel()
                    guard !offered.isEmpty else { return .ignored }
                    offered = []
                    return .handled
                }
                .onSubmit {
                    if offered.isEmpty { taken?() } else { take(chosen) }
                }
            if focused, !offered.isEmpty {
                VStack(alignment: .leading, spacing: 0) {
                    ForEach(Array(offered.enumerated()), id: \.offset) { index, person in
                        Button { take(index) } label: {
                            HStack(spacing: 8) {
                                if person.isList {
                                    Image(systemName: "person.3").accessibilityHidden(true)
                                }
                                Text(person.name ?? person.email).fontWeight(.semibold)
                                if !person.detail.isEmpty {
                                    Text(person.detail).foregroundStyle(.secondary)
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
        .onDisappear { lookup?.cancel() }
    }

    private func look() {
        lookup?.cancel()
        offered = []
        let field = text
        let excluded = leaving
        let sender = account
        lookup = Task {
            do { try await Task.sleep(for: .milliseconds(250)) } catch { return }
            guard !Task.isCancelled else { return }
            let local = await model.suggest(field, account: sender, excluding: excluded, directory: false)
            guard !Task.isCancelled, text == field, account == sender, leaving == excluded else { return }
            offered = local
            chosen = 0
            let found = await model.suggest(field, account: sender, excluding: excluded)
            if !Task.isCancelled, text == field, account == sender, leaving == excluded {
                let selection = offered.indices.contains(chosen) ? offered[chosen].completed : nil
                offered = found
                chosen = found.firstIndex(where: { $0.completed == selection }) ?? 0
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
        taken?()
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
                // The alert's sheet is still on the window here, and a
                // window with a sheet refuses to close without a word, so
                // the close waits for the sheet to go.
                DispatchQueue.main.async { window.performClose(nil) }
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

extension Notification.Name {
    /// A template was saved or deleted, so every list of them reads again.
    static let templatesChanged = Notification.Name("IrisTemplatesChanged")
}
