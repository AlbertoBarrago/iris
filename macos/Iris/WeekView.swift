import SwiftUI

/// The Week and Day grids: the days' headings, the all-day strip, then
/// the hours, scrolled to the day's first event or to eight. The core
/// lays out every card (`Mail.weekView`); this draws them.
struct WeekGrid: View {
    @Environment(MailModel.self) private var model

    static let gutter: CGFloat = 52
    static let hour: CGFloat = 48
    static let stripLine: CGFloat = 24
    /// A quarter hour, the step a drag snaps to.
    static let quarter: CGFloat = hour / 4
    /// The hours' coordinates, which drags read their points in.
    static let space = "hours"
    static let stripSpace = "strip"

    /// The span a drag across empty time is making.
    @State private var ghost: Selection?

    struct Selection: Equatable {
        var column: Int
        var top: CGFloat
        var bottom: CGFloat
    }

    var body: some View {
        if let week = model.weekView {
            VStack(spacing: 0) {
                heads(week)
                Divider()
                strip(week)
                Divider()
                hours(week)
            }
        } else {
            ProgressView().frame(maxWidth: .infinity, maxHeight: .infinity)
        }
    }

    // MARK: Headings and the strip

    private func heads(_ week: WeekView) -> some View {
        HStack(spacing: 0) {
            // A fixed spacer: a bare Color would take all the height free.
            Spacer().frame(width: Self.gutter, height: 1)
            ForEach(week.days, id: \.date) { day in
                HStack(spacing: 6) {
                    Text(day.weekday).foregroundStyle(.secondary)
                    Text("\(day.number)")
                        .fontWeight(day.today ? .bold : .medium)
                        .foregroundStyle(day.today ? Color.white : Color.primary)
                        .frame(minWidth: 24, minHeight: 24)
                        .background(day.today ? Color.accentColor : .clear, in: Circle())
                }
                .font(.callout)
                .frame(maxWidth: .infinity)
            }
        }
        .padding(.vertical, 6)
        .fixedSize(horizontal: false, vertical: true)
    }

    private func strip(_ week: WeekView) -> some View {
        let rows = CGFloat(max(week.stripRows, 1))
        return GeometryReader { proxy in
            let column = (proxy.size.width - Self.gutter) / CGFloat(max(week.days.count, 1))
            ZStack(alignment: .topLeading) {
                Text(tr("All day"))
                    .font(.caption2)
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.trailing)
                    .frame(width: Self.gutter - 8, alignment: .trailing)
                    .offset(y: 4)
                ForEach(Array(week.strip.enumerated()), id: \.offset) { _, item in
                    StripBar(item: item, column: column, days: week.days.count)
                }
            }
            // A bar moves while it is dragged, so its drag reads points
            // in the row's coordinates rather than its own.
            .coordinateSpace(name: Self.stripSpace)
        }
        .frame(height: max(rows * Self.stripLine + 6, 34))
    }

    // MARK: The hours

    private func hours(_ week: WeekView) -> some View {
        ScrollViewReader { scroller in
            ScrollView(.vertical) {
                // One row a hour, each named for the scroll to find, with
                // the grid laid over them.
                VStack(spacing: 0) {
                    ForEach(0..<24, id: \.self) { hour in
                        Color.clear.frame(height: Self.hour).id(hour)
                    }
                }
                .frame(maxWidth: .infinity)
                .overlay(alignment: .topLeading) {
                    GeometryReader { proxy in
                        let column = (proxy.size.width - Self.gutter) / CGFloat(max(week.days.count, 1))
                        ZStack(alignment: .topLeading) {
                            shading(week, column: column)
                            lines(week, width: proxy.size.width, column: column)
                            // A double click on empty time makes an event
                            // there, on the half hour it falls in.
                            // A drag across it makes one covering the
                            // quarter hours it crossed.
                            Color.clear
                                .contentShape(Rectangle())
                                .onTapGesture(count: 2) { at in create(at, week: week, column: column) }
                                .gesture(DragGesture(minimumDistance: 4, coordinateSpace: .named(Self.space))
                                    .onChanged { drag in
                                        ghost = Self.selection(from: drag.startLocation, to: drag.location, column: column, days: week.days.count)
                                    }
                                    .onEnded { _ in
                                        if let ghost { create(ghost, week: week) }
                                        ghost = nil
                                    })
                            if let ghost {
                                RoundedRectangle(cornerRadius: 5)
                                    .fill(Color.accentColor.opacity(0.3))
                                    .strokeBorder(Color.accentColor, lineWidth: 1)
                                    .frame(width: column - 3, height: ghost.bottom - ghost.top)
                                    .offset(x: Self.gutter + column * CGFloat(ghost.column) + 1, y: ghost.top)
                                    .allowsHitTesting(false)
                            }
                            ForEach(Array(week.timed.enumerated()), id: \.offset) { _, item in
                                TimedCard(item: item, column: column, days: week.days)
                            }
                            ForEach(Array(week.more.enumerated()), id: \.offset) { _, more in
                                MoreHourCard(more: more, column: column)
                            }
                            if let today = week.nowColumn {
                                nowLine(at: week.nowHours, column: Int(today), width: column)
                            }
                        }
                        .coordinateSpace(name: Self.space)
                    }
                }
            }
            // Opens at eight, or at the day's first event when earlier; the
            // rows have to be laid out before the scroll can find them.
            .task(id: week.days.first?.date) {
                try? await Task.sleep(for: .milliseconds(30))
                scroller.scrollTo(Int(week.firstHour), anchor: .top)
            }
        }
    }

    private func create(_ at: CGPoint, week: WeekView, column: CGFloat) {
        guard at.x > Self.gutter, column > 0 else { return }
        let index = min(Int((at.x - Self.gutter) / column), week.days.count - 1)
        guard let day = CalendarDate.date(week.days[index].date) else { return }
        let halfHours = Int(at.y / (Self.hour / 2))
        let start = Calendar.current.date(byAdding: .minute, value: halfHours * 30, to: day) ?? day
        model.newEvent(start: start, end: start.addingTimeInterval(3600), allDay: false)
    }

    /// Out-of-hours time and days off, shaded as the GTK grid shades them.
    private func shading(_ week: WeekView, column: CGFloat) -> some View {
        ForEach(Array(week.days.enumerated()), id: \.offset) { index, day in
            let x = Self.gutter + column * CGFloat(index)
            if day.working {
                Rectangle().fill(Color.primary.opacity(0.035))
                    .frame(width: column, height: week.workStart * Self.hour)
                    .offset(x: x)
                Rectangle().fill(Color.primary.opacity(0.035))
                    .frame(width: column, height: (24 - week.workEnd) * Self.hour)
                    .offset(x: x, y: week.workEnd * Self.hour)
            } else {
                Rectangle().fill(Color.primary.opacity(0.035))
                    .frame(width: column, height: 24 * Self.hour)
                    .offset(x: x)
            }
        }
    }

    /// The hour lines with their labels, and the lines between the days.
    private func lines(_ week: WeekView, width: CGFloat, column: CGFloat) -> some View {
        ZStack(alignment: .topLeading) {
            ForEach(1..<24, id: \.self) { hour in
                Rectangle().fill(Color.secondary.opacity(0.18))
                    .frame(width: width - Self.gutter, height: 1)
                    .offset(x: Self.gutter, y: CGFloat(hour) * Self.hour)
                Text(String(format: "%02d:00", hour))
                    .font(.caption2.monospacedDigit())
                    .foregroundStyle(.secondary)
                    .frame(width: Self.gutter - 8, alignment: .trailing)
                    .offset(y: CGFloat(hour) * Self.hour - 7)
            }
            ForEach(0..<week.days.count, id: \.self) { index in
                Rectangle().fill(Color.secondary.opacity(0.18))
                    .frame(width: 1, height: 24 * Self.hour)
                    .offset(x: Self.gutter + column * CGFloat(index))
            }
        }
    }

    /// The quarter hours a drag from `from` to `to` crossed, at least
    /// one, in the column it began in, as the GTK grid's
    /// `calendar_drag::selection` takes them.
    static func selection(from: CGPoint, to: CGPoint, column: CGFloat, days: Int) -> Selection? {
        guard from.x > gutter, column > 0 else { return nil }
        let index = min(Int((from.x - gutter) / column), days - 1)
        let day = 24 * hour
        let (low, high) = (min(from.y, to.y).clamped(0, day), max(from.y, to.y).clamped(0, day))
        let top = (low / quarter).rounded(.down) * quarter
        let bottom = max((high / quarter).rounded(.up) * quarter, top + quarter)
        return Selection(column: index, top: top, bottom: min(bottom, day))
    }

    private func create(_ selection: Selection, week: WeekView) {
        guard let day = CalendarDate.date(week.days[selection.column].date) else { return }
        let start = day.addingTimeInterval(TimeInterval(selection.top / Self.hour * 3600))
        let end = day.addingTimeInterval(TimeInterval(selection.bottom / Self.hour * 3600))
        model.newEvent(start: start, end: end, allDay: false)
    }

    /// The now line across today's column, a dot at its start.
    private func nowLine(at hours: Double, column: Int, width: CGFloat) -> some View {
        HStack(spacing: 0) {
            Circle().fill(.red).frame(width: 8, height: 8)
            Rectangle().fill(.red).frame(height: 2)
        }
        .frame(width: width + 4)
        .offset(x: Self.gutter + width * CGFloat(column) - 4, y: CGFloat(hours) * Self.hour - 4)
        .allowsHitTesting(false)
    }
}

/// "+N" where more events overlap than the column has lanes for; a click
/// lists them.
private struct MoreHourCard: View {
    let more: MoreCard
    let column: CGFloat
    @State private var listing = false

    var body: some View {
        let lanes: CGFloat = 4
        let lane = column / lanes
        Button(more.label) { listing = true }
            .buttonStyle(.plain)
            .font(.caption.weight(.semibold))
            .frame(width: lane - 3, height: max(CGFloat(more.bottom - more.top) * WeekGrid.hour - 2, 18))
            .background(Color.secondary.opacity(0.2), in: RoundedRectangle(cornerRadius: 5))
            .popover(isPresented: $listing) {
                VStack(alignment: .leading, spacing: 6) {
                    ForEach(more.items, id: \.id) { item in
                        HStack(spacing: 6) {
                            Circle().fill(Color(hex: item.color) ?? .accentColor).frame(width: 8, height: 8)
                            Text(EventTime.text(item.starts)).foregroundStyle(.secondary).monospacedDigit()
                            Text(item.title).lineLimit(1)
                        }
                        .font(.callout)
                        .eventActions(item.details)
                    }
                }
                .padding(12)
            }
            .offset(x: WeekGrid.gutter + column * CGFloat(more.column) + lane * (lanes - 1) + 1,
                    y: CGFloat(more.top) * WeekGrid.hour + 1)
    }
}

extension CGFloat {
    func clamped(_ low: CGFloat, _ high: CGFloat) -> CGFloat { Swift.min(Swift.max(self, low), high) }
}

/// A timed event: its share of the column, from its start to its end,
/// tinted in its colour with a bar down its leading edge. One the person
/// may change drags to another time or day, and its top or bottom edge
/// moves its start or end. The card follows the hand as the GTK grid's
/// does, and on release slides to the quarter hour the core settles it on
/// (`calendar_drag`), which asks before writing what the GTK calendar
/// asks.
private struct TimedCard: View {
    @Environment(MailModel.self) private var model
    let item: TimedItem
    let column: CGFloat
    let days: [WeekDayHead]

    /// How far the drag has taken the card, in points: across, and at its
    /// top and bottom.
    @State private var shift = Shift()
    @State private var handle: String?
    /// While the hand holds the card, a read of the grid leaves it be.
    @State private var dragging = false

    struct Shift: Equatable {
        var x: CGFloat = 0
        var top: CGFloat = 0
        var bottom: CGFloat = 0
    }

    private var day: CGFloat { 24 * WeekGrid.hour }
    private var baseTop: CGFloat { CGFloat(item.top) * WeekGrid.hour }
    private var span: CGFloat { CGFloat(item.bottom - item.top) * WeekGrid.hour }

    var body: some View {
        let color = Color(hex: item.color) ?? .accentColor
        let lane = column / CGFloat(max(item.lanes, 1))
        let height = max(span + shift.bottom - shift.top - 2, 18)
        let edge = min(8, height / 3)
        let moving = handle != nil
        VStack(alignment: .leading, spacing: 1) {
            Text(item.title).font(.caption.weight(.semibold)).lineLimit(height > 34 ? 2 : 1)
            if height > 30 || moving {
                Text(moving ? landingWords : EventTime.text(item.starts))
                    .font(.caption2).foregroundStyle(.secondary).monospacedDigit()
            }
        }
        .padding(.horizontal, 6)
        .padding(.vertical, 3)
        .frame(width: lane - 3, height: height, alignment: .topLeading)
        .background(color.opacity(moving ? 0.35 : 0.22), in: RoundedRectangle(cornerRadius: 5))
        .background(moving ? Color(nsColor: .windowBackgroundColor) : .clear, in: RoundedRectangle(cornerRadius: 5))
        .overlay(alignment: .leading) {
            UnevenRoundedRectangle(topLeadingRadius: 5, bottomLeadingRadius: 5).fill(color).frame(width: 3)
        }
        .overlay {
            if item.details.movable {
                VStack(spacing: 0) {
                    Color.clear.frame(height: edge).contentShape(Rectangle()).pointerStyle(.rowResize)
                    Spacer(minLength: 0)
                    Color.clear.frame(height: edge).contentShape(Rectangle()).pointerStyle(.rowResize)
                }
            }
        }
        .clipped()
        .shadow(color: .black.opacity(moving ? 0.25 : 0), radius: 6, y: 3)
        .eventActions(item.details, busy: moving)
        .gesture(drag(height: height, edge: edge), including: item.details.movable ? .all : .subviews)
        .offset(x: WeekGrid.gutter + column * CGFloat(item.column) + lane * CGFloat(item.lane) + 1 + shift.x,
                y: baseTop + shift.top + 1)
        .zIndex(moving ? 1 : 0)
        // The grid read again: the card sits where the core puts it.
        .onChange(of: model.calendarLoads) {
            guard !dragging else { return }
            shift = Shift()
            handle = nil
        }
    }

    /// The whole columns and quarter hours the drag comes to so far.
    private var snapped: (columns: Int, top: CGFloat, bottom: CGFloat) {
        let quarter = WeekGrid.quarter
        let columns = Int((shift.x / column).rounded()).clamped(-Int(item.column), days.count - 1 - Int(item.column))
        return (columns, (shift.top / quarter).rounded() * quarter, (shift.bottom / quarter).rounded() * quarter)
    }

    /// The time the card would land on, while it is dragged.
    private var landingWords: String {
        let (columns, top, bottom) = snapped
        let minute: Int64 = 60_000
        let dayMillis: Int64 = 86_400_000
        let moved = { (points: CGFloat) in Int64((points / WeekGrid.quarter).rounded()) * 15 * minute }
        let start = item.starts + moved(top) + Int64(columns) * dayMillis
        let end = item.ends + moved(bottom) + Int64(columns) * dayMillis
        return "\(EventTime.text(start)) – \(EventTime.text(end))"
    }

    private func drag(height: CGFloat, edge: CGFloat) -> some Gesture {
        DragGesture(minimumDistance: 3, coordinateSpace: .named(WeekGrid.space))
            .onChanged { drag in
                dragging = true
                let grabbed = handle ?? {
                    let y = drag.startLocation.y - baseTop - 1
                    return y < edge ? "start" : y >= height - edge ? "end" : "body"
                }()
                handle = grabbed
                let dy = drag.translation.height
                let quarter = WeekGrid.quarter
                switch grabbed {
                case "start":
                    shift = Shift(top: dy.clamped(-baseTop, span - quarter))
                case "end":
                    shift = Shift(bottom: dy.clamped(quarter - span, day - baseTop - span))
                default:
                    // Kept inside the week across and the day down.
                    let left = -column * CGFloat(item.column)
                    let right = column * CGFloat(days.count - 1 - Int(item.column))
                    let y = dy.clamped(-baseTop, day - baseTop - span)
                    shift = Shift(x: drag.translation.width.clamped(left, right), top: y, bottom: y)
                }
            }
            .onEnded { drag in
                dragging = false
                guard let grabbed = handle else { return }
                let (columns, top, bottom) = snapped
                guard (columns, top, bottom) != (0, 0, 0),
                      let from = instant(drag.startLocation.y, column: Int(item.column)) else {
                    withAnimation(.snappy(duration: 0.2)) { shift = Shift() }
                    handle = nil
                    return
                }
                // Slides onto the quarter hour and day it lands on, and
                // stays there until the grid is read again, written or
                // put back.
                withAnimation(.snappy(duration: 0.2)) {
                    shift = Shift(x: CGFloat(columns) * column, top: top, bottom: bottom)
                }
                let landed = grabbed == "body" ? Int(item.column) + columns : Int(item.column)
                guard let to = instant(drag.startLocation.y + drag.translation.height, column: landed) else { return }
                model.dropEvent(item.details) {
                    $0.dragTimed(handle: grabbed, grabbed: from.at, released: to.at, dayStart: to.day.0, dayEnd: to.day.1)
                }
            }
    }

    /// The instant `y` points at in day `column`, and that day's span.
    private func instant(_ y: CGFloat, column: Int) -> (at: Int64, day: (Int64, Int64))? {
        guard days.indices.contains(column), let midnight = CalendarDate.date(days[column].date),
              let next = Calendar.current.date(byAdding: .day, value: 1, to: midnight) else { return nil }
        let at = midnight.addingTimeInterval(TimeInterval(y / WeekGrid.hour * 3600))
        let millis = { (date: Date) in Int64(date.timeIntervalSince1970 * 1000) }
        return (millis(at), (millis(midnight), millis(next)))
    }
}

/// An event in the all-day row. One the person may change drags by whole
/// days, and its ends stretch or shorten it; it follows the hand and
/// slides onto the day it lands on.
private struct StripBar: View {
    @Environment(MailModel.self) private var model
    let item: StripItem
    let column: CGFloat
    let days: Int

    /// How far the drag has taken the grabbed part, in points.
    @State private var shift: CGFloat = 0
    @State private var edge: String?
    /// While the hand holds the bar, a read of the grid leaves it be.
    @State private var dragging = false

    private var span: Int { Int(item.end - item.start) }

    var body: some View {
        let color = Color(hex: item.color) ?? .accentColor
        let full = column * CGFloat(span) - 4
        let (x, width): (CGFloat, CGFloat) = switch edge {
        case "start": (shift, full - shift)
        case "end": (0, full + shift)
        default: (shift, full)
        }
        let reach = min(8, full / 3)
        let moving = edge != nil
        Text(item.title)
            .font(.caption.weight(.medium))
            .foregroundStyle(.white)
            .lineLimit(1)
            .padding(.horizontal, 6)
            .frame(width: max(width, 12), height: WeekGrid.stripLine - 4, alignment: .leading)
            .background(color, in: RoundedRectangle(cornerRadius: 4))
            .overlay {
                if item.details.movable {
                    HStack(spacing: 0) {
                        Color.clear.frame(width: reach).contentShape(Rectangle()).pointerStyle(.columnResize)
                        Spacer(minLength: 0)
                        Color.clear.frame(width: reach).contentShape(Rectangle()).pointerStyle(.columnResize)
                    }
                }
            }
            .shadow(color: .black.opacity(moving ? 0.25 : 0), radius: 6, y: 3)
            .eventActions(item.details, busy: moving)
            .gesture(drag(full: full, reach: reach), including: item.details.movable ? .all : .subviews)
            .offset(x: WeekGrid.gutter + column * CGFloat(item.start) + 2 + x, y: 4 + CGFloat(item.line) * WeekGrid.stripLine)
            .zIndex(moving ? 1 : 0)
            .onChange(of: model.calendarLoads) {
                guard !dragging else { return }
                shift = 0
                edge = nil
            }
    }

    private func drag(full: CGFloat, reach: CGFloat) -> some Gesture {
        DragGesture(minimumDistance: 3, coordinateSpace: .named(WeekGrid.stripSpace))
            .onChanged { drag in
                dragging = true
                let grabbed = edge ?? {
                    let x = drag.startLocation.x - (WeekGrid.gutter + column * CGFloat(item.start) + 2)
                    return x < reach ? "start" : x >= full - reach ? "end" : ""
                }()
                edge = grabbed
                let dx = drag.translation.width
                let (left, right) = (-column * CGFloat(item.start), column * CGFloat(days - Int(item.end)))
                shift = switch grabbed {
                // Never shorter than a day.
                case "start": dx.clamped(left, full - column + 4)
                case "end": dx.clamped(column - 4 - full, right)
                default: dx.clamped(left, right)
                }
            }
            .onEnded { _ in
                dragging = false
                guard let grabbed = edge else { return }
                let moved = Int((shift / column).rounded())
                withAnimation(.snappy(duration: 0.2)) { shift = CGFloat(moved) * column }
                guard moved != 0 else {
                    edge = nil
                    return
                }
                model.dropEvent(item.details) { $0.dragStrip(edge: grabbed, days: Int64(moved)) }
            }
    }
}

extension Int {
    func clamped(_ low: Int, _ high: Int) -> Int { Swift.min(Swift.max(self, low), high) }
}
