import SwiftUI

/// An editor the window opens: on an event of the person's own, or on a
/// new one.
struct EditorRequest: Identifiable {
    let id = UUID()
    let editor: EventEditor
}

/// The event editor, as the GTK calendar's: the title, the calendar, all
/// day or the times, the place and the notes, and Delete for an event
/// that already exists. The core keeps the draft's rules
/// (`calendar_draft`) and asks, before writing, what the GTK calendar
/// asks of a repeating event or of its guests.
struct EventEditorSheet: View {
    @Environment(MailModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let editor: EventEditor

    @State private var form: EventForm?
    @State private var calendars: [CalendarChoice] = []
    @State private var question: EventQuestion?
    @State private var guests: [GuestRow] = []
    @State private var guestField = ""
    @State private var deleting = false
    @State private var problem: String?

    var body: some View {
        VStack(spacing: 0) {
            if let form = Binding($form) {
                Form {
                    TextField(tr("Title"), text: form.title, prompt: Text(tr("Title")))
                        .font(.title3)
                    Picker(tr("Calendar"), selection: form.calendar) {
                        ForEach(calendars, id: \.key) { calendar in
                            Label {
                                Text(calendar.name)
                            } icon: {
                                FlagImage.dot(calendar.color)
                            }
                            .tag(calendar.key)
                        }
                    }
                    Toggle(tr("All day"), isOn: Binding(
                        get: { form.wrappedValue.allDay },
                        set: { on in
                            form.wrappedValue.allDay = on
                            // The draft decides the times all day takes.
                            editor.update(form: form.wrappedValue)
                            self.form = editor.form()
                        }
                    ))
                    if form.wrappedValue.allDay {
                        DatePicker(tr("Starts"), selection: dayBinding(form, \.start), displayedComponents: .date)
                        DatePicker(tr("Ends"), selection: lastDayBinding(form), displayedComponents: .date)
                    } else {
                        DatePicker(tr("Starts"), selection: timeBinding(form, \.start, keepsLength: true))
                        DatePicker(tr("Ends"), selection: timeBinding(form, \.end, keepsLength: false))
                    }
                    TextField(tr("Place"), text: form.place, prompt: Text(tr("Place")))
                    Section {
                        ForEach(guests, id: \.email) { guest in
                            HStack {
                                GuestLine(guest: guest)
                                if editor.guestsEditable() && !guest.organizer {
                                    Button {
                                        editor.removeGuest(email: guest.email)
                                        guests = editor.guests()
                                    } label: {
                                        Image(systemName: "xmark.circle.fill").foregroundStyle(.secondary)
                                    }
                                    .buttonStyle(.plain)
                                    .help(tr("Remove"))
                                }
                            }
                        }
                        if editor.guestsEditable() {
                            // The people the store knows are offered as in
                            // the composer's fields; a pick joins the list.
                            RecipientField(
                                placeholder: tr("Add guests"),
                                text: $guestField,
                                account: account(form.wrappedValue),
                                leaving: Set(guests.map { $0.email.lowercased() }),
                                taken: { takeGuests() }
                            )
                        }
                    } header: {
                        HStack(spacing: 6) {
                            Text(tr("Guests"))
                            if !guests.isEmpty { Text("\(guests.count)").foregroundStyle(.secondary) }
                        }
                    }
                    Section(tr("Notes")) {
                        TextEditor(text: form.notes)
                            .frame(minHeight: 90)
                            .font(.body)
                    }
                }
                .formStyle(.grouped)
            } else {
                ProgressView().padding(40)
            }
            if let problem {
                Text(problem).foregroundStyle(.red).font(.callout).padding(.horizontal, 20)
            }
            Divider()
            HStack {
                if !editor.isNew() {
                    Button(tr("Delete"), role: .destructive) { askDelete() }
                }
                Spacer()
                Button(tr("Cancel")) { dismiss() }
                    .keyboardShortcut(.cancelAction)
                Button(tr("Save")) { askSave() }
                    .keyboardShortcut(.defaultAction)
                    .buttonStyle(.borderedProminent)
                    .disabled(form?.title.trimmingCharacters(in: .whitespaces).isEmpty ?? true)
            }
            .padding(14)
        }
        .frame(width: 520)
        .frame(maxHeight: 720)
        .onAppear {
            form = editor.form()
            calendars = editor.calendars()
            guests = editor.guests()
        }
        .sheet(item: $question) { asked in
            QuestionSheet(question: asked) { response, scope, send in
                question = nil
                guard let response else { return }
                finish(response: response, scope: scope, send: send)
            }
        }
    }

    /// The account of the calendar picked: the calendar's key is
    /// `account/calendar`.
    private func account(_ form: EventForm) -> Int64 {
        Int64(form.calendar.split(separator: "/").first ?? "") ?? 0
    }

    // MARK: Saving and deleting

    /// Adds the addresses typed; the field keeps what is not one.
    private func takeGuests() {
        guestField = editor.addGuests(text: guestField)
        guests = editor.guests()
    }

    private func askSave() {
        guard let form else { return }
        // An address still in the field is invited too, as the GTK
        // editor's Save does.
        if !guestField.trimmingCharacters(in: .whitespaces).isEmpty { takeGuests() }
        editor.update(form: form)
        deleting = false
        ask { try editor.saveQuestion() }
    }

    private func askDelete() {
        deleting = true
        ask { try editor.deleteQuestion() }
    }

    /// Asks the question the core has, or writes at once when it has none.
    private func ask(_ question: () throws -> EventQuestion?) {
        do {
            if let asked = try question() {
                self.question = asked
            } else {
                finish(response: nil, scope: nil, send: true)
            }
        } catch {
            problem = "\(error)"
        }
    }

    private func finish(response: String?, scope: String?, send: Bool) {
        let (editor, deleting) = (editor, deleting)
        Task.detached {
            let done = Result {
                deleting
                    ? try editor.delete(response: response, scope: scope, send: send)
                    : try editor.save(response: response, scope: scope, send: send)
            }
            await MainActor.run {
                switch done {
                case .success(let words):
                    model.eventWritten(words)
                    dismiss()
                case .failure(let error):
                    problem = "\(error)"
                }
            }
        }
    }

    // MARK: Dates

    /// A timed start or end. Moving the start takes the end along, as the
    /// GTK editor does.
    private func timeBinding(_ form: Binding<EventForm>, _ field: WritableKeyPath<EventForm, Int64>, keepsLength: Bool) -> Binding<Date> {
        Binding {
            Date(timeIntervalSince1970: TimeInterval(form.wrappedValue[keyPath: field]) / 1000)
        } set: { date in
            let millis = Int64(date.timeIntervalSince1970 * 1000)
            if keepsLength {
                let length = form.wrappedValue.end - form.wrappedValue.start
                form.wrappedValue.start = millis
                form.wrappedValue.end = millis + length
            } else {
                form.wrappedValue.end = max(millis, form.wrappedValue.start)
            }
        }
    }

    /// An all-day event's first day: its start is UTC midnight of that day.
    private func dayBinding(_ form: Binding<EventForm>, _ field: WritableKeyPath<EventForm, Int64>) -> Binding<Date> {
        Binding {
            AllDay.date(form.wrappedValue[keyPath: field])
        } set: { date in
            let days = (form.wrappedValue.end - form.wrappedValue.start) / AllDay.day
            form.wrappedValue.start = AllDay.millis(date)
            form.wrappedValue.end = form.wrappedValue.start + max(days, 1) * AllDay.day
        }
    }

    /// An all-day event's last day: its end is UTC midnight after it.
    private func lastDayBinding(_ form: Binding<EventForm>) -> Binding<Date> {
        Binding {
            AllDay.date(form.wrappedValue.end - AllDay.day)
        } set: { date in
            form.wrappedValue.end = max(AllDay.millis(date) + AllDay.day, form.wrappedValue.start + AllDay.day)
        }
    }
}

/// An all-day event's days are UTC dates; the pickers show them as the
/// same calendar day here.
enum AllDay {
    static let day: Int64 = 86_400_000

    static func date(_ millis: Int64) -> Date {
        var utc = Calendar(identifier: .gregorian)
        utc.timeZone = TimeZone(identifier: "UTC")!
        let parts = utc.dateComponents([.year, .month, .day], from: Date(timeIntervalSince1970: TimeInterval(millis) / 1000))
        return Calendar.current.date(from: parts) ?? Date()
    }

    static func millis(_ date: Date) -> Int64 {
        let parts = Calendar.current.dateComponents([.year, .month, .day], from: date)
        var utc = Calendar(identifier: .gregorian)
        utc.timeZone = TimeZone(identifier: "UTC")!
        return Int64((utc.date(from: parts) ?? date).timeIntervalSince1970 * 1000)
    }
}

/// The question before a save or a delete, as the GTK dialog lays it out:
/// the heading, what happens, the occurrences to pick from and the
/// guests tick when the buttons already choose something else, then the
/// buttons.
struct QuestionSheet: View {
    let question: EventQuestion
    let answered: (String?, String?, Bool) -> Void
    @State private var scope: String?
    @State private var send = true

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(question.heading).font(.headline)
            ForEach(question.body, id: \.self) { line in
                Text(line).fixedSize(horizontal: false, vertical: true)
            }
            if !question.scopes.isEmpty {
                Picker("", selection: Binding(get: { scope ?? question.scopes.first?.id }, set: { scope = $0 })) {
                    ForEach(question.scopes, id: \.id) { option in
                        Text(option.label).tag(Optional(option.id))
                    }
                }
                .pickerStyle(.radioGroup)
                .labelsHidden()
            }
            if question.sendCheck {
                Toggle(tr("Send an update to the guests"), isOn: $send)
            }
            HStack {
                Spacer()
                ForEach(question.responses, id: \.id) { button in
                    let pressed = {
                        answered(button.id == "cancel" ? nil : button.id, scope ?? question.scopes.first?.id, send)
                    }
                    Group {
                        switch button.look {
                        case "suggested":
                            Button(button.label, action: pressed).buttonStyle(.borderedProminent)
                        case "destructive":
                            Button(button.label, role: .destructive, action: pressed)
                        default:
                            Button(button.label, action: pressed)
                        }
                    }
                    .keyboardShortcut(button.id == question.defaultId ? .defaultAction : nil)
                }
            }
        }
        .padding(20)
        .frame(width: 420)
    }
}

extension EventQuestion: Identifiable {
    public var id: String { heading + responses.map(\.id).joined() }
}

extension MailModel {
    /// Opens the event the details name: its editor when it is the
    /// person's own, else everything about it, read only.
    func openEvent(_ details: EventDetails) {
        guard let mail else { return }
        if let editor = try? mail.editEvent(id: details.id, starts: details.starts) {
            editing = EditorRequest(editor: editor)
        } else {
            openedEvent = details
        }
    }

    /// A new event from `start` to `end`, all day or not.
    func newEvent(start: Date, end: Date, allDay: Bool) {
        guard let mail else { return }
        let (from, to) = (Int64(start.timeIntervalSince1970 * 1000), Int64(end.timeIntervalSince1970 * 1000))
        if let editor = try? mail.newEvent(start: from, end: to, allDay: allDay) {
            editing = EditorRequest(editor: editor)
        } else {
            say(tr("None of your calendars take new events"))
        }
    }

    /// A new event at the next whole hour of the day shown, for an hour.
    func newEventNow() {
        let calendar = Calendar.current
        let hour = calendar.component(.hour, from: Date()) + 1
        let day = calendar.startOfDay(for: calendarDay)
        let start = calendar.date(byAdding: .hour, value: min(hour, 23), to: day) ?? Date()
        newEvent(start: start, end: start.addingTimeInterval(3600), allDay: false)
    }

    /// An event was saved or deleted: say so, and draw the calendar again.
    func eventWritten(_ words: String) {
        if !words.isEmpty { say(words) }
        loadCalendar()
    }
}
