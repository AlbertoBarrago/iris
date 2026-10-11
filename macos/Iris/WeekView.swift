import SwiftUI

/// The Week and Day grids: the days' headings, the all-day strip, then
/// the hours, scrolled to the day's first event or to eight. The core
/// lays out every card (`Mail.weekView`); this draws them.
struct WeekGrid: View {
    @Environment(MailModel.self) private var model

    static let gutter: CGFloat = 52
    static let hour: CGFloat = 48
    static let stripLine: CGFloat = 24

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
                    let color = Color(hex: item.color) ?? .accentColor
                    Text(item.title)
                        .font(.caption.weight(.medium))
                        .foregroundStyle(.white)
                        .lineLimit(1)
                        .padding(.horizontal, 6)
                        .frame(width: column * CGFloat(item.end - item.start) - 4, height: Self.stripLine - 4, alignment: .leading)
                        .background(color, in: RoundedRectangle(cornerRadius: 4))
                        .eventActions(item.details)
                        .offset(x: Self.gutter + column * CGFloat(item.start) + 2, y: 4 + CGFloat(item.line) * Self.stripLine)
                }
            }
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
                            Color.clear
                                .contentShape(Rectangle())
                                .onTapGesture(count: 2) { at in create(at, week: week, column: column) }
                            ForEach(Array(week.timed.enumerated()), id: \.offset) { _, item in
                                card(item, column: column)
                            }
                            ForEach(Array(week.more.enumerated()), id: \.offset) { _, more in
                                MoreHourCard(more: more, column: column)
                            }
                            if let today = week.nowColumn {
                                nowLine(at: week.nowHours, column: Int(today), width: column)
                            }
                        }
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

    /// A timed event: its share of the column, from its start to its end,
    /// tinted in its colour with a bar down its leading edge.
    private func card(_ item: TimedItem, column: CGFloat) -> some View {
        let color = Color(hex: item.color) ?? .accentColor
        let lane = column / CGFloat(max(item.lanes, 1))
        let height = max(CGFloat(item.bottom - item.top) * Self.hour - 2, 18)
        return VStack(alignment: .leading, spacing: 1) {
            Text(item.title).font(.caption.weight(.semibold)).lineLimit(height > 34 ? 2 : 1)
            if height > 30 {
                Text(EventTime.text(item.starts)).font(.caption2).foregroundStyle(.secondary).monospacedDigit()
            }
        }
        .padding(.horizontal, 6)
        .padding(.vertical, 3)
        .frame(width: lane - 3, height: height, alignment: .topLeading)
        .background(color.opacity(0.22), in: RoundedRectangle(cornerRadius: 5))
        .overlay(alignment: .leading) {
            UnevenRoundedRectangle(topLeadingRadius: 5, bottomLeadingRadius: 5).fill(color).frame(width: 3)
        }
        .clipped()
        .eventActions(item.details)
        .offset(x: Self.gutter + column * CGFloat(item.column) + lane * CGFloat(item.lane) + 1,
                y: CGFloat(item.top) * Self.hour + 1)
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
