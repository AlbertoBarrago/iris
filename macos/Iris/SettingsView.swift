import AppKit
import SwiftUI
import UniformTypeIdentifiers

/// Iris's Settings window (⌘,), with the GTK Preferences' pages as tabs
/// and its words. Every change lands in the same settings file at once.
struct SettingsView: View {
    /// The tab on show, kept so Settings opens where it was left, and so
    /// the assistant's Choose a Model can open it on the AI page.
    @AppStorage(SettingsView.tabKey) private var tab = "general"

    static let tabKey = "settingsTab"

    var body: some View {
        TabView(selection: $tab) {
            GeneralSettings()
                .tabItem { Label(tr("General"), systemImage: "gearshape") }
                .tag("general")
            WritingSettings()
                .tabItem { Label(tr("Writing"), systemImage: "square.and.pencil") }
                .tag("writing")
            CalendarSettings()
                .tabItem { Label(tr("Calendar"), systemImage: "calendar") }
                .tag("calendar")
            SyncSettingsView()
                .tabItem { Label(tr("Sync"), systemImage: "arrow.triangle.2.circlepath") }
                .tag("sync")
            AISettings()
                .tabItem { Label(tr("AI"), systemImage: "sparkles") }
                .tag("ai")
            LanguageSettings()
                .tabItem { Label(tr("Language"), systemImage: "globe") }
                .tag("language")
        }
        .frame(minWidth: 520, idealWidth: 620, maxWidth: .infinity, minHeight: 420, idealHeight: 640, maxHeight: .infinity)
        .background(Resizable())
    }
}

/// SwiftUI's Settings window has a fixed size; this gives it the
/// resizable style once it is on screen, so long pages can be made taller.
private struct Resizable: NSViewRepresentable {
    func makeNSView(context: Context) -> NSView {
        let view = NSView()
        DispatchQueue.main.async { view.window?.styleMask.insert(.resizable) }
        return view
    }

    func updateNSView(_ view: NSView, context: Context) {
        view.window?.styleMask.insert(.resizable)
    }
}

/// A title, and under it in smaller words what the setting does.
private struct Titled: View {
    let title: String
    var subtitle: String? = nil

    var body: some View {
        Text(title)
        if let subtitle { Text(subtitle) }
    }
}

/// A switch bound to one preference by its name.
private struct Switch: View {
    @Environment(MailModel.self) private var model
    let title: String
    var subtitle: String? = nil
    let name: String
    let value: Bool

    var body: some View {
        Toggle(isOn: Binding(get: { value }, set: { model.setPreference(name, $0) })) {
            Titled(title: title, subtitle: subtitle)
        }
    }
}

/// A choice bound to one preference by its name, offering the core's
/// options in its order and words.
private struct ChoicePicker: View {
    @Environment(MailModel.self) private var model
    let title: String
    var subtitle: String? = nil
    let name: String
    let current: String
    let items: [ChoiceItem]

    var body: some View {
        Picker(selection: Binding(get: { current }, set: { model.setPreference(name, $0) })) {
            ForEach(items, id: \.key) { item in
                Text(item.label).tag(item.key)
            }
        } label: {
            Titled(title: title, subtitle: subtitle)
        }
    }
}

private struct GeneralSettings: View {
    @Environment(MailModel.self) private var model

    var body: some View {
        if let prefs = model.prefs, let choices = model.choices {
            Form {
                Section(tr("Reading")) {
                    Switch(title: tr("Group Messages into Conversations"),
                           subtitle: tr("Show a thread's replies together instead of one row per message"),
                           name: "threading", value: prefs.threading)
                    Switch(title: tr("Group Inbox into Categories"),
                           subtitle: tr("Sort the inbox into Primary, Updates, Promotions, and Social, as Gmail does"),
                           name: "inbox_categories", value: prefs.inboxCategories)
                    ChoicePicker(title: tr("Open the Inbox On"), subtitle: tr("Which category the window starts on"),
                                 name: "default_category", current: prefs.defaultCategory, items: choices.defaultCategory)
                        .disabled(!prefs.inboxCategories)
                    Switch(title: tr("Suggest Follow-Ups"),
                           subtitle: tr("List mail you sent that has had no reply for three days"),
                           name: "suggest_follow_ups", value: prefs.suggestFollowUps)
                    ChoicePicker(title: tr("Mark as Read"), name: "mark_read", current: prefs.markRead, items: choices.markRead)
                    ChoicePicker(title: tr("Remote Images"), subtitle: tr("Loading them can tell senders when you read their mail"),
                                 name: "remote_images", current: prefs.remoteImages, items: choices.remoteImages)
                    ImageSenders()
                    ChoicePicker(title: tr("Text Size"), name: "text_size", current: prefs.textSize, items: choices.textSize)
                }
                Section(tr("Appearance")) {
                    ChoicePicker(title: tr("Style"), name: "color_scheme", current: prefs.colorScheme, items: choices.colorScheme)
                }
                Section(tr("Notifications")) {
                    Switch(title: tr("Notify About New Mail"), name: "notifications", value: prefs.notifications)
                    Group {
                        Switch(title: tr("Only for VIPs"), subtitle: tr("Stay quiet about mail from everyone else"),
                               name: "notify_vips_only", value: prefs.notifyVipsOnly)
                        Switch(title: tr("Show Sender and Subject"), subtitle: tr("Turn off to see only how much mail arrived"),
                               name: "notification_previews", value: prefs.notificationPreviews)
                        DisclosureGroup {
                            ForEach(choices.notificationButtons, id: \.key) { button in
                                Switch(title: button.label, name: "notification_button:\(button.key)",
                                       value: prefs.notificationButtons.contains(button.key))
                            }
                        } label: {
                            Titled(title: tr("Buttons"), subtitle: tr("What a notification offers besides opening the conversation"))
                        }
                    }
                    .disabled(!prefs.notifications)
                }
            }
            .formStyle(.grouped)
        } else {
            ProgressView().padding(40)
        }
    }
}

/// The senders whose images load without asking, each with a way off the
/// list. Read when the row shows, so opening Settings never waits on it.
private struct ImageSenders: View {
    @Environment(MailModel.self) private var model
    @State private var list: [ImageSender] = []

    var body: some View {
        DisclosureGroup {
            ForEach(list, id: \.sender) { entry in
                HStack {
                    Titled(title: entry.sender, subtitle: entry.wholeDomain ? tr("Anyone at this domain") : tr("This address"))
                    Spacer()
                    Button {
                        model.forgetImageSender(entry.sender)
                        list = model.imageSenders()
                    } label: {
                        Image(systemName: "trash")
                    }
                    .buttonStyle(.borderless)
                    .help(tr("Stop Loading Images from This Sender"))
                    .accessibilityLabel(tr("Stop loading images from {sender}").replacingOccurrences(of: "{sender}", with: entry.sender))
                }
            }
        } label: {
            Titled(title: tr("Senders Who May Load Images"), subtitle: count)
        }
        .onAppear { list = model.imageSenders() }
    }

    private var count: String {
        list.isEmpty ? tr("Nobody yet")
            : trPlural("{count} sender", "{count} senders", list.count)
                .replacingOccurrences(of: "{count}", with: "\(list.count)")
    }
}

private struct WritingSettings: View {
    @Environment(MailModel.self) private var model
    @State private var signatures: [SignatureSetting] = []

    var body: some View {
        if let prefs = model.prefs, let choices = model.choices {
            Form {
                Section(tr("New Messages")) {
                    Picker(selection: Binding(
                        get: { prefs.defaultAccount ?? signatures.first?.email ?? "" },
                        set: { model.setPreference("default_account", $0) }
                    )) {
                        ForEach(signatures, id: \.email) { signature in
                            Text(signature.email).tag(signature.email)
                        }
                    } label: {
                        Titled(title: tr("Send New Messages From"),
                               subtitle: tr("Replies always come from the account that received the message"))
                    }
                    ChoicePicker(title: tr("New Messages Start As"),
                                 subtitle: tr("Rich text styles the words themselves; Markdown shows its marks"),
                                 name: "compose_format", current: prefs.composeFormat, items: choices.composeFormat)
                    ChoicePicker(title: tr("Undo Send"), subtitle: tr("How long you can take a message back after sending it"),
                                 name: "undo_send", current: prefs.undoSend, items: choices.undoSend)
                    Switch(title: tr("Check for Missing Attachments"),
                           subtitle: tr("Ask before sending a message that promises a file and carries none"),
                           name: "check_attachments", value: prefs.checkAttachments)
                }
                Section(tr("Signatures")) {
                    if signatures.isEmpty {
                        Text(tr("No Accounts Yet")).foregroundStyle(.secondary)
                    }
                    ForEach($signatures, id: \.email) { $signature in
                        SignatureEditor(signature: $signature)
                    }
                }
            }
            .formStyle(.grouped)
            .onAppear { signatures = model.signatures() }
        } else {
            ProgressView().padding(40)
        }
    }
}

/// The calendar's own settings: reminders before events, the first day
/// of the week, and the hours meetings usually run in.
private struct CalendarSettings: View {
    @Environment(MailModel.self) private var model

    var body: some View {
        if let prefs = model.prefs, let choices = model.choices {
            Form {
                Section(tr("Calendar")) {
                    Switch(title: tr("Event Reminders"),
                           subtitle: tr("A notification before each event, at the times the event or its calendar sets"),
                           name: "event_reminders", value: prefs.eventReminders)
                    ChoicePicker(title: tr("Week Starts On"),
                                 subtitle: tr("Automatic follows the locale's own first day of the week"),
                                 name: "week_start", current: prefs.weekStart, items: choices.weekStart)
                }
                Section(tr("Working Hours")) {
                    WorkingHoursRows(hours: prefs.workingHours)
                }
            }
            .formStyle(.grouped)
        } else {
            ProgressView().padding(40)
        }
    }
}

/// The working day's first and last hour, and the days it runs on,
/// Monday first, as the settings keep them.
private struct WorkingHoursRows: View {
    @Environment(MailModel.self) private var model
    let hours: Hours

    var body: some View {
        HStack {
            Text(tr("Hours"))
            Spacer()
            Picker(tr("Starts"), selection: Binding(get: { hours.start }, set: { change(start: $0) })) {
                ForEach(Array(UInt32(0)..<24), id: \.self) { Text(Self.time($0)).tag($0) }
            }
            .labelsHidden()
            .fixedSize()
            Text(tr("–"))
            Picker(tr("Ends"), selection: Binding(get: { hours.end }, set: { change(end: $0) })) {
                ForEach(Array(UInt32(1)...24), id: \.self) { Text(Self.time($0)).tag($0) }
            }
            .labelsHidden()
            .fixedSize()
        }
        HStack {
            Text(tr("Days"))
            Spacer()
            ForEach(0..<7, id: \.self) { day in
                Toggle(Self.weekday(day, short: true), isOn: Binding(
                    get: { hours.days.indices.contains(day) && hours.days[day] },
                    set: { on in
                        var days = hours.days
                        while days.count < 7 { days.append(false) }
                        days[day] = on
                        model.setWorkingHours(Hours(start: hours.start, end: hours.end, days: days))
                    }
                ))
                .toggleStyle(.button)
                .help(Self.weekday(day, short: false))
                .accessibilityLabel(Self.weekday(day, short: false))
            }
        }
    }

    /// A start at or after the end pushes the end an hour past it, and
    /// an end at or before the start pulls the start an hour before it,
    /// so the working day never runs backwards.
    private func change(start: UInt32? = nil, end: UInt32? = nil) {
        var from = start ?? hours.start
        var to = end ?? hours.end
        if start != nil, to <= from { to = from + 1 }
        if end != nil, from >= to { from = to - 1 }
        model.setWorkingHours(Hours(start: from, end: to, days: hours.days))
    }

    /// An hour in the system's clock; 24 is midnight at the day's end.
    private static func time(_ hour: UInt32) -> String {
        let date = Calendar.current.date(bySettingHour: Int(hour % 24), minute: 0, second: 0, of: .now) ?? .now
        return date.formatted(date: .omitted, time: .shortened)
    }

    /// The weekday `index` places after Monday, in the interface's language.
    private static func weekday(_ index: Int, short: Bool) -> String {
        let formatter = DateFormatter()
        formatter.locale = appLocale
        let names = short ? formatter.veryShortStandaloneWeekdaySymbols : formatter.standaloneWeekdaySymbols
        // The formatter's lists start on Sunday.
        return names?[(index + 1) % 7] ?? ""
    }
}

/// How the sync engine runs: how often it looks for mail, how much it
/// keeps on this Mac. A change starts syncing again with it.
private struct SyncSettingsView: View {
    @Environment(MailModel.self) private var model
    @State private var sync: SyncSettings?

    var body: some View {
        if sync != nil, let choices = model.choices {
            Form {
                Section(tr("Checking")) {
                    number(tr("Check for New Mail"), nil, \.pollSeconds, choices.poll)
                }
                Section(tr("Storage")) {
                    number(tr("Keep Mail on This Computer For"),
                           tr("Everything in your inbox stays too, and older mail remains searchable"),
                           \.windowDays, choices.window)
                    number(tr("Message Cache"), tr("Bodies of mail you have read, kept for opening offline"),
                           \.cacheMb, choices.cache)
                }
            }
            .formStyle(.grouped)
        } else {
            ProgressView().padding(40).onAppear { sync = model.syncSettings() }
        }
    }

    private func number(_ title: String, _ subtitle: String?, _ field: WritableKeyPath<SyncSettings, Int64>,
                        _ items: [NumberChoice]) -> some View {
        Picker(selection: Binding(
            get: { sync?[keyPath: field] ?? 0 },
            set: { value in
                guard var changed = sync else { return }
                changed[keyPath: field] = value
                sync = changed
                model.setSyncSettings(changed)
            }
        )) {
            ForEach(items, id: \.value) { item in Text(item.label).tag(item.value) }
        } label: {
            Titled(title: title, subtitle: subtitle)
        }
    }
}

/// One address's signature: Markdown lines, kept as they are typed, or a
/// formatted one pasted from another app or imported from an `.htm` file,
/// which goes out as it was made.
private struct SignatureEditor: View {
    @Environment(MailModel.self) private var model
    @Binding var signature: SignatureSetting

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(tr("Signature for {account}").replacingOccurrences(of: "{account}", with: signature.email))
                .font(.headline)
            if let formatted = signature.formatted {
                Text(tr("This signature goes out as it was made, pictures and layout included."))
                    .font(.callout)
                    .foregroundStyle(.secondary)
                MailPage(html: formatted)
                    .frame(height: 140)
                    .clipShape(RoundedRectangle(cornerRadius: 6))
                    .overlay(RoundedRectangle(cornerRadius: 6).stroke(.separator))
            } else {
                TextEditor(text: Binding(
                    get: { signature.markdown },
                    set: { text in
                        signature.markdown = text
                        model.keepSignature(email: signature.email, markdown: text)
                    }
                ))
                .font(.body.monospaced())
                .frame(height: 80)
                .scrollContentBackground(.hidden)
                .padding(4)
                .background(.background, in: RoundedRectangle(cornerRadius: 6))
                .overlay(RoundedRectangle(cornerRadius: 6).stroke(.separator))
            }
            HStack {
                Button(tr("Import from Gmail"), action: importGmail)
                Button(tr("Paste Formatted Signature"), action: paste)
                Button(tr("Import from File…"), action: importFile)
                if signature.formatted != nil {
                    Button(tr("Remove Formatted Signature")) { keep("") }
                }
            }
            .controlSize(.small)
        }
        .padding(.vertical, 4)
    }

    /// Brings in the signature Gmail keeps for the account, as Markdown,
    /// which puts a formatted signature away.
    private func importGmail() {
        let signature = signature
        Task {
            do {
                guard let text = try await model.gmailSignature(account: signature.accountId) else {
                    model.say(tr("Gmail has no signature for this account"))
                    return
                }
                model.keepSignature(email: signature.email, markdown: text, formatted: "")
                if let kept = model.signatures().first(where: { $0.email == signature.email }) {
                    self.signature = kept
                }
                model.say(tr("Imported the signature from Gmail"))
            } catch {
                model.say(tr("Could not import: {reason}").replacingOccurrences(of: "{reason}", with: "\(error)"))
            }
        }
    }

    /// Takes the HTML another app put on the clipboard, as Outlook or a
    /// browser do when a signature is copied.
    private func paste() {
        guard let html = NSPasteboard.general.string(forType: .html), !html.isEmpty else {
            model.say(tr("There is no formatted signature to bring in"))
            return
        }
        keep(html)
    }

    private func importFile() {
        let panel = NSOpenPanel()
        panel.allowedContentTypes = [.html]
        panel.allowsMultipleSelection = false
        guard panel.runModal() == .OK, let url = panel.url else { return }
        do {
            keep(try importSignature(path: url.path(percentEncoded: false)))
        } catch {
            model.say(tr("Could not import: {reason}").replacingOccurrences(of: "{reason}", with: "\(error)"))
        }
    }

    /// Keeps `html` as the formatted signature, empty to remove it, and
    /// shows what the core kept, which is cleaned.
    private func keep(_ html: String) {
        model.keepSignature(email: signature.email, formatted: html)
        if let kept = model.signatures().first(where: { $0.email == signature.email }) {
            signature = kept
        }
        if !html.isEmpty, signature.formatted != nil {
            model.say(tr("Formatted signature saved"))
        }
    }
}

/// The interface's language: Follow System, or one of the catalogs Iris
/// carries. The words change after a restart, as in the GTK app.
private struct LanguageSettings: View {
    @Environment(MailModel.self) private var model

    /// English as written, and the Italian catalog: the two languages
    /// Iris is kept complete in.
    private let languages = [(code: "en_US", name: "English"), (code: "it_IT", name: "Italiano")]

    var body: some View {
        if let prefs = model.prefs {
            Form {
                Picker(selection: Binding(get: { prefs.language }, set: { model.setPreference("language", $0) })) {
                    Text(tr("Follow System")).tag("")
                    ForEach(languages, id: \.code) { language in
                        Text(language.name).tag(language.code)
                    }
                } label: {
                    Text(tr("Language"))
                    Text(tr("Iris shows a new language after a restart"))
                }
            }
            .formStyle(.grouped)
        } else {
            ProgressView().padding(40)
        }
    }
}
