import SwiftUI

/// The two spaces of the window, as the GTK window's switch names them.
enum Space: String {
    case mail, calendar
}

/// The calendar's three grids.
enum CalendarKind: String, CaseIterable {
    case day, week, month
}

/// The calendar page: the grid chosen, under one toolbar that steps it,
/// goes back to today and switches between Day, Week and Month.
struct CalendarPage: View {
    @Environment(MailModel.self) private var model

    var body: some View {
        @Bindable var model = model
        Group {
            switch model.calendarKind {
            case .month: MonthGrid()
            case .week, .day: WeekGrid()
            }
        }
        .navigationTitle(title)
        .sheet(item: Bindable(model).openedEvent) { details in
            EventSheet(details: details)
        }
        .sheet(item: Bindable(model).editing) { request in
            EventEditorSheet(editor: request.editor)
        }
        .sheet(item: Bindable(model).dropping) { request in
            QuestionSheet(question: request.question) { response, scope, send in
                model.dropping = nil
                if let response {
                    model.writeDrop(request.editor, response: response, scope: scope, send: send)
                } else {
                    // Cancel puts the card back where it was.
                    model.loadCalendar()
                }
            }
        }
        .toolbar {
            ToolbarItemGroup(placement: .navigation) {
                Button(previousWords, systemImage: "chevron.left") { model.stepCalendar(-1) }
                    .keyboardShortcut(.leftArrow, modifiers: .command)
                Button(nextWords, systemImage: "chevron.right") { model.stepCalendar(1) }
                    .keyboardShortcut(.rightArrow, modifiers: .command)
            }
            ToolbarItem {
                Button(tr("Today")) { model.showToday() }
                    .keyboardShortcut("t", modifiers: .command)
            }
            ToolbarItem {
                // The GTK calendar's N.
                Button(tr("New Event (N)"), systemImage: "plus") { model.newEventNow() }
                    .keyboardShortcut("n", modifiers: [])
            }
            ToolbarItem {
                Picker(tr("View"), selection: $model.calendarKind) {
                    Text(tr("Day")).tag(CalendarKind.day)
                    Text(tr("Week")).tag(CalendarKind.week)
                    Text(tr("Month")).tag(CalendarKind.month)
                }
                .pickerStyle(.segmented)
            }
        }
        // The GTK calendar's d, w and m.
        .background {
            Group {
                Button("") { model.calendarKind = .day }.keyboardShortcut("d", modifiers: [])
                Button("") { model.calendarKind = .week }.keyboardShortcut("w", modifiers: [])
                Button("") { model.calendarKind = .month }.keyboardShortcut("m", modifiers: [])
            }
            .opacity(0)
            .accessibilityHidden(true)
        }
    }

    private var title: String {
        switch model.calendarKind {
        case .month: model.monthView?.title ?? tr("Calendar")
        case .week, .day: model.weekView?.title ?? tr("Calendar")
        }
    }

    private var previousWords: String {
        switch model.calendarKind {
        case .day: tr("Previous Day")
        case .week: tr("Previous Week")
        case .month: tr("Previous Month")
        }
    }

    private var nextWords: String {
        switch model.calendarKind {
        case .day: tr("Next Day")
        case .week: tr("Next Week")
        case .month: tr("Next Month")
        }
    }
}

/// Mail and Calendar, at the top of either sidebar, as the GTK sidebar's
/// two toggles. ⌥1 and ⌥2 flip it from the View menu.
struct SpaceSwitch: View {
    @Environment(MailModel.self) private var model

    var body: some View {
        HStack(spacing: 4) {
            choice(.mail, tr("Mail"), "envelope")
            choice(.calendar, tr("Calendar"), "calendar")
        }
        .padding(3)
        .background(.quaternary.opacity(0.5), in: RoundedRectangle(cornerRadius: 9, style: .continuous))
        .padding(.horizontal, 10)
        .padding(.top, 4)
        .padding(.bottom, 6)
    }

    /// One half of the switch: its icon and name, filled while picked.
    private func choice(_ space: Space, _ name: String, _ symbol: String) -> some View {
        let picked = model.space == space
        return Button {
            withAnimation(.snappy(duration: 0.2)) { model.space = space }
        } label: {
            Label(name, systemImage: symbol)
                .font(.callout.weight(.semibold))
                .frame(maxWidth: .infinity)
                .padding(.vertical, 6)
                .foregroundStyle(picked ? Color.white : Color.primary)
                .background(picked ? Color.accentColor : .clear, in: RoundedRectangle(cornerRadius: 7, style: .continuous))
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityAddTraits(picked ? .isSelected : [])
    }
}

/// The calendar space's sidebar: each account's calendars, each with its
/// colour and a tick for whether its events show.
struct CalendarSidebar: View {
    @Environment(MailModel.self) private var model

    var body: some View {
        List {
            ForEach(model.calendarAccounts, id: \.accountId) { account in
                Section(account.email) {
                    ForEach(account.calendars, id: \.id) { calendar in
                        Toggle(isOn: Binding(
                            get: { calendar.shown },
                            set: { model.showCalendar(calendar, $0) }
                        )) {
                            Text(calendar.name).lineLimit(1)
                        }
                        .toggleStyle(CalendarCheck(color: Color(hex: calendar.color) ?? .accentColor))
                    }
                }
            }
        }
        .listStyle(.sidebar)
        .safeAreaInset(edge: .top) { SpaceSwitch() }
        .onAppear { model.loadCalendar() }
    }
}

/// A calendar's tick in its own colour, as the GTK sidebar draws it.
private struct CalendarCheck: ToggleStyle {
    let color: Color

    func makeBody(configuration: Configuration) -> some View {
        Button { configuration.isOn.toggle() } label: {
            HStack(spacing: 8) {
                RoundedRectangle(cornerRadius: 4)
                    .strokeBorder(color, lineWidth: 1.5)
                    .background(configuration.isOn ? color : .clear, in: RoundedRectangle(cornerRadius: 4))
                    .overlay {
                        if configuration.isOn {
                            Image(systemName: "checkmark").font(.system(size: 9, weight: .bold)).foregroundStyle(.white)
                        }
                    }
                    .frame(width: 14, height: 14)
                configuration.label
                Spacer(minLength: 0)
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }
}

/// The month: six whole weeks under the weekday names, each day with its
/// number and as many events as its row holds, the rest behind "N more".
/// The core lays the weeks out (`Mail.monthView`); this draws them.
struct MonthGrid: View {
    @Environment(MailModel.self) private var model

    /// A day's number above its events, and one line of events.
    static let heading: CGFloat = 24
    static let line: CGFloat = 20
    /// The month's coordinates, which a drag reads its points in.
    static let space = "month"

    /// An event being dragged to another day.
    @State private var ghost: MonthGhost?

    var body: some View {
        VStack(spacing: 0) {
            if let view = model.monthView {
                HStack(spacing: 0) {
                    ForEach(Array(view.weekdays.enumerated()), id: \.offset) { _, name in
                        Text(name)
                            .font(.caption.weight(.semibold))
                            .foregroundStyle(.secondary)
                            .frame(maxWidth: .infinity)
                    }
                }
                .padding(.vertical, 6)
                Divider()
                GeometryReader { proxy in
                    let rowHeight = proxy.size.height / CGFloat(max(view.weeks.count, 1))
                    let column = proxy.size.width / 7
                    // The day under a point of the month, counted from the
                    // top left; a row is as tall as its divider too.
                    let cell = { (point: CGPoint) -> Int in
                        let row = Int(point.y / rowHeight).clamped(0, view.weeks.count - 1)
                        return row * 7 + Int(point.x / column).clamped(0, 6)
                    }
                    VStack(spacing: 0) {
                        ForEach(Array(view.weeks.enumerated()), id: \.offset) { index, week in
                            WeekRow(week: week, width: proxy.size.width, height: rowHeight, ghost: $ghost, cell: cell)
                                // The row an event is dragged out of stays
                                // over the others, so the event shows.
                                .zIndex(ghost.map { $0.from / 7 == index } == true ? 1 : 0)
                            Divider()
                        }
                    }
                    .coordinateSpace(name: Self.space)
                    // The day a dragged event would land on.
                    .overlay(alignment: .topLeading) {
                        if let ghost, ghost.to != ghost.from {
                            RoundedRectangle(cornerRadius: 4)
                                .fill(Color.accentColor.opacity(0.12))
                                .strokeBorder(Color.accentColor.opacity(0.7), lineWidth: 1.5)
                                .frame(width: column - 2, height: rowHeight - 2)
                                .offset(x: column * CGFloat(ghost.to % 7) + 1, y: rowHeight * CGFloat(ghost.to / 7) + 1)
                                .allowsHitTesting(false)
                        }
                    }
                    .onAppear { fit(rowHeight) }
                    .onChange(of: rowHeight) { fit(rowHeight) }
                    // A read mid-drag clears the mark; the next move of
                    // the hand puts it back.
                    .onChange(of: model.calendarLoads) { ghost = nil }
                            }
            } else {
                ProgressView().frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
    }

    /// How many lines of events fit a row `height` tall, which the core
    /// lays the month out for.
    private func fit(_ height: CGFloat) {
        model.monthRows = UInt32(max(1, Int((height - Self.heading - 4) / Self.line)))
    }
}

/// One week: the seven day cells, then its events laid over them on the
/// lines the core gave each.
private struct WeekRow: View {
    @Environment(MailModel.self) private var model
    let week: MonthWeek
    let width: CGFloat
    let height: CGFloat
    @Binding var ghost: MonthGhost?
    let cell: (CGPoint) -> Int

    var body: some View {
        let column = width / 7
        ZStack(alignment: .topLeading) {
            HStack(spacing: 0) {
                ForEach(Array(week.days.enumerated()), id: \.offset) { index, day in
                    DayCell(day: day, more: week.more[index])
                        .frame(width: column, height: height)
                        .overlay(alignment: .trailing) {
                            if index < 6 { Divider() }
                        }
                }
            }
            ForEach(Array(week.items.enumerated()), id: \.offset) { _, item in
                EventChip(item: item, ghost: $ghost, cell: cell)
                    .frame(width: column * CGFloat(item.end - item.start) - 4, height: MonthGrid.line - 2)
                    .offset(x: column * CGFloat(item.start) + 2,
                            y: MonthGrid.heading + CGFloat(item.line) * MonthGrid.line)
            }
        }
        .frame(width: width, height: height, alignment: .topLeading)
    }
}

/// A day's number, today in the accent's pill, a day of another month
/// dimmed and a day off shaded; "N more" at the bottom opens the day.
private struct DayCell: View {
    @Environment(MailModel.self) private var model
    let day: MonthDay
    let more: String
    @State private var listing = false

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack {
                Spacer()
                Text("\(day.number)")
                    .font(.callout.weight(day.today ? .bold : .regular))
                    .foregroundStyle(day.today ? Color.white : day.inMonth ? Color.primary : Color.secondary.opacity(0.6))
                    .padding(.horizontal, 6)
                    .padding(.vertical, 1)
                    .background(day.today ? Color.accentColor : .clear, in: Capsule())
            }
            .padding(.top, 3)
            .padding(.trailing, 4)
            Spacer(minLength: 0)
            if !more.isEmpty {
                Button(more) { listing = true }
                    .buttonStyle(.plain)
                    .font(.caption.weight(.medium))
                    .foregroundStyle(.secondary)
                    .padding(.leading, 6)
                    .padding(.bottom, 3)
                    .popover(isPresented: $listing) { DayList(date: day.date) }
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .background(day.working ? Color.clear : Color.primary.opacity(0.035))
        .contentShape(Rectangle())
        // A double click on the day makes an event on it, at nine.
        .onTapGesture(count: 2) {
            guard let date = CalendarDate.date(day.date) else { return }
            let start = Calendar.current.date(byAdding: .hour, value: 9, to: date) ?? date
            model.newEvent(start: start, end: start.addingTimeInterval(3600), allDay: false)
        }
    }
}

/// An event dragged across the month: which, and the days it left and
/// is over.
struct MonthGhost: Equatable {
    let id: String
    let starts: Int64
    let from: Int
    let to: Int
}

enum CalendarDate {
    /// The day `YYYY-MM-DD` names, at its local midnight.
    static func date(_ key: String) -> Date? {
        let parts = key.split(separator: "-").compactMap { Int($0) }
        guard parts.count == 3 else { return nil }
        return Calendar.current.date(from: DateComponents(year: parts[0], month: parts[1], day: parts[2]))
    }
}

/// An event in the grid: a bar in its colour across several days, or a
/// chip with a dot and its time on one. Resting the pointer on it shows
/// its card; a double click keeps the card open.
private struct EventChip: View {
    @Environment(MailModel.self) private var model
    let item: MonthItem
    @Binding var ghost: MonthGhost?
    let cell: (CGPoint) -> Int

    /// How far the hand has taken the event, in points.
    @State private var moved = CGSize.zero
    /// While the hand holds the event, a read of the month leaves it be.
    @State private var dragging = false

    var body: some View {
        let dragged = moved != .zero || ghost?.id == item.id && ghost?.starts == item.starts
        chip
            .background(dragged && !(item.bar || item.allDay) ? Color(nsColor: .windowBackgroundColor) : .clear,
                        in: RoundedRectangle(cornerRadius: 4))
            .shadow(color: .black.opacity(moved != .zero ? 0.25 : 0), radius: 6, y: 3)
            .eventActions(item.details, busy: dragged)
            .gesture(drag, including: item.details.movable ? .all : .subviews)
            .offset(moved)
            .zIndex(dragged ? 1 : 0)
            .onChange(of: model.calendarLoads) {
                guard !dragging else { return }
                moved = .zero
            }
    }

    /// A drag to another day moves the event there, keeping its time of
    /// day; the core counts the days (`calendar_drag::by_days`). The event
    /// follows the hand, and the day it would land on is marked.
    private var drag: some Gesture {
        DragGesture(minimumDistance: 3, coordinateSpace: .named(MonthGrid.space))
            .onChanged { drag in
                dragging = true
                moved = drag.translation
                let (from, to) = (cell(drag.startLocation), cell(drag.location))
                // Only a new day changes the month around it.
                if ghost?.to != to || ghost?.id != item.id {
                    ghost = MonthGhost(id: item.id, starts: item.starts, from: from, to: to)
                }
            }
            .onEnded { _ in
                dragging = false
                guard let landed = ghost, landed.to != landed.from else {
                    withAnimation(.snappy(duration: 0.2)) { moved = .zero }
                    ghost = nil
                    return
                }
                // It stays where it was let go until the month is read
                // again, written or put back.
                model.dropEvent(item.details) { $0.moveDays(days: Int64(landed.to - landed.from)) }
            }
    }

    @ViewBuilder private var chip: some View {
        let color = Color(hex: item.color) ?? .accentColor
        Group {
            if item.bar || item.allDay {
                Text(item.title)
                    .font(.caption.weight(.medium))
                    .foregroundStyle(.white)
                    .lineLimit(1)
                    .padding(.horizontal, 6)
                    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .leading)
                    .background(color, in: UnevenRoundedRectangle(
                        topLeadingRadius: item.squaredStart ? 0 : 4, bottomLeadingRadius: item.squaredStart ? 0 : 4,
                        bottomTrailingRadius: item.squaredEnd ? 0 : 4, topTrailingRadius: item.squaredEnd ? 0 : 4))
            } else {
                HStack(spacing: 4) {
                    Circle().fill(color).frame(width: 7, height: 7)
                    Text(EventTime.text(item.starts)).foregroundStyle(.secondary).monospacedDigit()
                    Text(item.title).lineLimit(1)
                    Spacer(minLength: 0)
                }
                .font(.caption)
                .padding(.horizontal, 4)
            }
        }
        .accessibilityLabel(item.described)
    }
}

/// What every event in the calendar answers: resting the pointer on it
/// shows its card, a double click opens it whole.
struct EventActions: ViewModifier {
    @Environment(MailModel.self) private var model
    let details: EventDetails
    /// While the event is dragged its card stays away.
    let busy: Bool
    @State private var hovering = false
    @State private var peeking = false

    func body(content: Content) -> some View {
        content
            .contentShape(Rectangle())
            .onHover { inside in
                hovering = inside
                if !inside { peeking = false }
            }
            .task(id: hovering) {
                // A short rest before the card, so sweeping across the
                // calendar does not flash one under every event.
                guard hovering, !busy else { return }
                try? await Task.sleep(for: .milliseconds(600))
                if hovering, !busy, !Task.isCancelled { peeking = true }
            }
            .onChange(of: busy) { if busy { peeking = false } }
            // A double click opens the event, as in the GTK calendar.
            .simultaneousGesture(TapGesture(count: 2).onEnded {
                peeking = false
                model.openEvent(details)
            })
            .popover(isPresented: $peeking, arrowEdge: .top) {
                EventCard(details: details)
            }
    }
}

extension View {
    func eventActions(_ details: EventDetails, busy: Bool = false) -> some View {
        modifier(EventActions(details: details, busy: busy))
    }
}

/// An event opened with a double click: everything about it, as a
/// meeting app shows it. When, where and the call at the top, then who
/// is asked and what each answered, then the notes.
struct EventSheet: View {
    @Environment(\.dismiss) private var dismiss
    let details: EventDetails

    var body: some View {
        VStack(spacing: 0) {
            HStack(alignment: .top, spacing: 12) {
                RoundedRectangle(cornerRadius: 3)
                    .fill(Color(hex: details.color) ?? .accentColor)
                    .frame(width: 6, height: 44)
                VStack(alignment: .leading, spacing: 4) {
                    Text(details.title).font(.title2.weight(.semibold)).fixedSize(horizontal: false, vertical: true)
                    Text(details.when).foregroundStyle(.secondary)
                }
                Spacer(minLength: 0)
                if let url = URL(string: details.conference), !details.conference.isEmpty {
                    Link(destination: url) { Label(tr("Join Call"), systemImage: "video.fill") }
                        .buttonStyle(.borderedProminent)
                }
            }
            .padding(20)
            Divider()
            ScrollView {
                VStack(alignment: .leading, spacing: 18) {
                    if !details.place.isEmpty || !details.calendar.isEmpty {
                        VStack(alignment: .leading, spacing: 8) {
                            if !details.place.isEmpty { fact("mappin.and.ellipse", details.place) }
                            if !details.calendar.isEmpty { fact("calendar", details.calendar) }
                        }
                    }
                    if !details.guests.isEmpty {
                        section(tr("Guests"), count: details.guests.count) {
                            VStack(alignment: .leading, spacing: 10) {
                                ForEach(details.guests, id: \.email) { guest in GuestLine(guest: guest) }
                            }
                        }
                    }
                    if !details.notes.isEmpty {
                        section(tr("Notes"), count: nil) {
                            Text(details.notes)
                                .textSelection(.enabled)
                                .fixedSize(horizontal: false, vertical: true)
                                .frame(maxWidth: .infinity, alignment: .leading)
                        }
                    }
                }
                .padding(20)
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            // As tall as what it holds, up to a point, then it scrolls.
            .scrollBounceBehavior(.basedOnSize)
            .frame(maxHeight: 480)
            .fixedSize(horizontal: false, vertical: true)
            Divider()
            HStack {
                Spacer()
                Button(tr("Close")) { dismiss() }
                    .keyboardShortcut(.cancelAction)
            }
            .padding(12)
        }
        .frame(width: 560)
    }

    private func fact(_ symbol: String, _ text: String) -> some View {
        Label {
            Text(text).textSelection(.enabled)
        } icon: {
            Image(systemName: symbol).foregroundStyle(.secondary)
        }
    }

    private func section<Content: View>(_ title: String, count: Int?, @ViewBuilder content: () -> Content) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack(spacing: 6) {
                Text(title).font(.headline)
                if let count { Text("\(count)").foregroundStyle(.secondary) }
            }
            content()
        }
    }
}

/// One guest: their initials, name and address, and their answer as an
/// icon in its colour with its word.
struct GuestLine: View {
    let guest: GuestRow

    var body: some View {
        HStack(spacing: 10) {
            ZStack {
                Circle().fill(Color.secondary.opacity(0.18))
                Text(initials).font(.caption.weight(.semibold))
            }
            .frame(width: 30, height: 30)
            .overlay(alignment: .bottomTrailing) {
                Image(systemName: symbol)
                    .font(.system(size: 11, weight: .bold))
                    .foregroundStyle(.white)
                    .padding(2)
                    .background(tint, in: Circle())
                    .offset(x: 3, y: 3)
            }
            VStack(alignment: .leading, spacing: 1) {
                Text(guest.name).fontWeight(.medium).lineLimit(1)
                if guest.name != guest.email {
                    Text(guest.email).font(.caption).foregroundStyle(.secondary).lineLimit(1)
                }
            }
            Spacer(minLength: 8)
            Text(guest.said).font(.callout).foregroundStyle(tint)
        }
        .accessibilityElement(children: .combine)
    }

    private var initials: String {
        let words = guest.name.split(separator: " ").prefix(2)
        let letters = words.compactMap(\.first).map(String.init).joined()
        return letters.isEmpty ? String(guest.email.prefix(1)).uppercased() : letters.uppercased()
    }

    private var symbol: String {
        switch guest.answer {
        case "yes": "checkmark"
        case "no": "xmark"
        case "maybe": "questionmark"
        default: "clock"
        }
    }

    private var tint: Color {
        switch guest.answer {
        case "yes": .green
        case "no": .red
        case "maybe": .orange
        default: .secondary
        }
    }
}

/// An event's card: its title in its colour, when, where, on which
/// calendar, who organizes it, how many are asked, and its call.
struct EventCard: View {
    let details: EventDetails

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(alignment: .firstTextBaseline, spacing: 8) {
                RoundedRectangle(cornerRadius: 2)
                    .fill(Color(hex: details.color) ?? .accentColor)
                    .frame(width: 4, height: 16)
                Text(details.title).font(.headline).fixedSize(horizontal: false, vertical: true)
            }
            line("clock", details.when)
            if !details.place.isEmpty { line("mappin.and.ellipse", details.place) }
            if !details.calendar.isEmpty { line("calendar", details.calendar) }
            if !details.organizer.isEmpty { line("person", "\(tr("Organizer")): \(details.organizer)") }
            if !details.guests.isEmpty {
                line("person.2", trPlural("{count} guest", "{count} guests", details.guests.count)
                    .replacingOccurrences(of: "{count}", with: "\(details.guests.count)"))
            }
            if let url = URL(string: details.conference), !details.conference.isEmpty {
                Link(destination: url) { Label(tr("Join Call"), systemImage: "video") }
                    .buttonStyle(.borderedProminent)
                    .controlSize(.small)
            }
        }
        .padding(14)
        .frame(minWidth: 280, alignment: .leading)
    }

    private func line(_ symbol: String, _ text: String) -> some View {
        Label {
            Text(text).fixedSize(horizontal: false, vertical: true)
        } icon: {
            Image(systemName: symbol).foregroundStyle(.secondary)
        }
        .font(.callout)
    }
}

/// Every event on one day, from its "N more".
private struct DayList: View {
    @Environment(MailModel.self) private var model
    let date: String
    @State private var items: [DayItem] = []

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            ForEach(items, id: \.id) { item in
                HStack(spacing: 6) {
                    Circle().fill(Color(hex: item.color) ?? .accentColor).frame(width: 8, height: 8)
                    Text(item.allDay ? tr("All day") : EventTime.text(item.starts))
                        .foregroundStyle(.secondary).monospacedDigit()
                    Text(item.title).lineLimit(1)
                }
                .font(.callout)
                .contentShape(Rectangle())
                .onTapGesture(count: 2) { model.openEvent(item.details) }
            }
        }
        .padding(12)
        .frame(minWidth: 220, alignment: .leading)
        .task { items = model.dayEvents(date) }
    }
}

/// A time of day as the Mac's own settings write it, 24-hour or not.
enum EventTime {
    static func text(_ millis: Int64) -> String {
        Date(timeIntervalSince1970: TimeInterval(millis) / 1000).formatted(date: .omitted, time: .shortened)
    }
}

extension MailModel {
    func showCalendar(_ calendar: CalendarRow, _ shown: Bool) {
        guard let mail else { return }
        let (account, id) = (calendar.accountId, calendar.id)
        Task.detached {
            try? mail.showCalendar(accountId: account, calendar: id, shown: shown)
            await MainActor.run { self.loadCalendar() }
        }
    }

    /// The day, week or month before or after the one shown.
    func stepCalendar(_ by: Int) {
        let unit: Calendar.Component = switch calendarKind {
        case .day: .day
        case .week: .weekOfYear
        case .month: .month
        }
        calendarDay = Calendar.current.date(byAdding: unit, value: by, to: calendarDay) ?? calendarDay
        loadCalendar()
    }

    func showToday() {
        calendarDay = Date()
        loadCalendar()
    }

    /// `YYYY-MM-DD`, as the core names a day.
    nonisolated static func dayKey(_ date: Date) -> String {
        let parts = Calendar.current.dateComponents([.year, .month, .day], from: date)
        return String(format: "%04d-%02d-%02d", parts.year ?? 2026, parts.month ?? 1, parts.day ?? 1)
    }

    func dayEvents(_ date: String) -> [DayItem] {
        (try? mail?.dayEvents(date: date)) ?? []
    }
}

extension EventDetails: Identifiable {}
