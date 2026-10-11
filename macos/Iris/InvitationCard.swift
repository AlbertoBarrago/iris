import SwiftUI

/// The invitation in the open conversation, as a card above the page: the
/// GTK card's lines and buttons, worded by the core
/// (`ffi/src/mail_invitation.rs`). The message below still holds the
/// organizer's own links, for whoever would rather use those.
struct InvitationCard: View {
    @Environment(MailModel.self) private var model
    let invitation: MailInvitation
    let account: Int64
    @State private var note = ""
    @State private var series = false
    @State private var sending = false
    /// The answer shown as given: the one sent, before the card is read
    /// again.
    @State private var given: String
    /// Where the last answer or proposal went, under the buttons.
    @State private var went = ""
    @State private var picking = false
    @State private var target: String

    init(invitation: MailInvitation, account: Int64) {
        self.invitation = invitation
        self.account = account
        _given = State(initialValue: invitation.answer)
        _target = State(initialValue: (invitation.targets.first { $0.primary } ?? invitation.targets.first)?.calendar ?? "")
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            if !invitation.news.isEmpty {
                Text(invitation.news)
                    .font(.callout.weight(.medium))
                    .foregroundStyle(invitation.newsTone == "cancelled" ? Color.red : Color.orange)
            }
            HStack(alignment: .top, spacing: 12) {
                RoundedRectangle(cornerRadius: 2)
                    .fill(barColor)
                    .frame(width: 4)
                VStack(alignment: .leading, spacing: 4) {
                    Text(invitation.title).font(.title3.weight(.semibold))
                    if !invitation.when.isEmpty { Text(invitation.when) }
                    meta
                    ForEach(invitation.events, id: \.self) { Text($0).foregroundStyle(.secondary) }
                    if let strip = invitation.strip {
                        DayStripView(strip: strip, answered: !given.isEmpty)
                            .padding(.top, 6)
                    } else if !invitation.clash.isEmpty {
                        Label(invitation.clash, systemImage: "exclamationmark.triangle")
                            .foregroundStyle(.orange)
                    }
                    actions.padding(.top, 6)
                    if !went.isEmpty {
                        Text(went).font(.callout).foregroundStyle(.secondary)
                    }
                }
                .textSelection(.enabled)
            }
            .fixedSize(horizontal: false, vertical: true)
        }
        .padding(14)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 12))
        .padding(10)
        .accessibilityElement(children: .contain)
        .accessibilityLabel(tr("Invitation"))
        .sheet(isPresented: $picking) {
            PickTimeSheet(organizer: invitation.organizer, start: initialPick) { at in
                picking = false
                if let at { propose(at) }
            }
        }
    }

    /// The colour of the calendar that holds the event, once the strip
    /// has found it there; the accent until then.
    private var barColor: Color {
        let own = invitation.strip?.blocks.first { $0.this }?.color ?? ""
        return Color(hex: own) ?? .accentColor
    }

    /// How the series runs, who organizes it, and the guest count, which
    /// opens the list of guests.
    private var meta: some View {
        HStack(spacing: 4) {
            if !invitation.meta.isEmpty {
                Text(invitation.meta + (invitation.saidYes.isEmpty ? "" : " ·"))
                    .lineLimit(1)
                    .truncationMode(.tail)
            }
            if !invitation.saidYes.isEmpty { GuestsButton(invitation: invitation) }
        }
        .foregroundStyle(.secondary)
    }

    private var actions: some View {
        VStack(alignment: .leading, spacing: 8) {
            if invitation.answerable {
                TextField(tr("Add a note"), text: $note, prompt: Text(tr("Add a note")))
                    .textFieldStyle(.roundedBorder)
                    .frame(maxWidth: 320)
                    .help(tr("The organizer reads it with your answer"))
                    .accessibilityLabel(tr("Note with your answer"))
            }
            HStack(spacing: 8) {
                if invitation.answerable {
                    // Yes, Maybe, No: the order the GTK card answers in.
                    ForEach([("yes", tr("Yes")), ("maybe", tr("Maybe")), ("no", tr("No"))], id: \.0) { answer, label in
                        let current = given == answer
                        Button(label) { send(answer) }
                            .buttonStyle(.bordered)
                            .tint(current ? .accentColor : nil)
                            .background(current ? Color.accentColor.opacity(0.2) : .clear, in: Capsule())
                            .accessibilityValue(current ? tr("Your answer") : "")
                    }
                    if invitation.asksScope {
                        Picker(tr("What the answer covers"), selection: $series) {
                            Text(tr("This Event")).tag(false)
                            Text(tr("All Events")).tag(true)
                        }
                        .pickerStyle(.segmented)
                        .labelsHidden()
                        .fixedSize()
                    }
                }
                if sending { ProgressView().controlSize(.small) }
                Spacer(minLength: 8)
                if !invitation.proposals.isEmpty {
                    Menu(tr("Propose New Time")) {
                        ForEach(invitation.proposals, id: \.at) { time in
                            Button(time.label) { propose(time.at) }
                        }
                        Divider()
                        Button(tr("Pick a Time…")) { picking = true }
                    }
                    .menuStyle(.borderlessButton)
                    .fixedSize()
                }
                calendarButton
            }
            .disabled(sending)
        }
    }

    @ViewBuilder
    private var calendarButton: some View {
        if let spot = invitation.onCalendar {
            Button(tr("Show in Calendar"), systemImage: "calendar") { model.showSpot(spot) }
                .buttonStyle(.bordered)
        } else if invitation.adds {
            if invitation.targets.count > 1 {
                Picker(tr("Calendar to add to"), selection: $target) {
                    ForEach(invitation.targets, id: \.calendar) { Text($0.label).tag($0.calendar) }
                }
                .labelsHidden()
                .fixedSize()
            }
            Button(tr("Add to Calendar"), systemImage: "calendar.badge.plus") { add() }
                .buttonStyle(.bordered)
        }
    }

    /// Where Pick a Time starts: the hour the organizer asked for.
    private var initialPick: Date {
        invitation.proposals.first.map { Date(timeIntervalSince1970: TimeInterval($0.at) / 1000 - 1800) } ?? Date()
    }

    private func send(_ answer: String) {
        guard let mail = model.mail, given != answer || !note.isEmpty else { return }
        let (id, account, note, series) = (invitation.messageId, account, note, series)
        let before = given
        given = answer
        sending = true
        Task.detached {
            let done = Result { try mail.answerInvitation(accountId: account, messageId: id, answer: answer, series: series, note: note) }
            await MainActor.run {
                sending = false
                switch done {
                case .success(let done):
                    went = done.went
                    self.note = ""
                    if !done.toast.isEmpty { model.say(done.toast) }
                case .failure(let error):
                    given = before
                    model.say("\(error)")
                }
            }
        }
    }

    private func propose(_ at: Int64) {
        guard let mail = model.mail else { return }
        let (id, account, series) = (invitation.messageId, account, series)
        sending = true
        Task.detached {
            let done = Result { try mail.proposeInvitation(accountId: account, messageId: id, at: at, series: series) }
            await MainActor.run {
                sending = false
                switch done {
                case .success(let done):
                    went = done.went
                    model.say(done.toast)
                case .failure(let error):
                    model.say("\(error)")
                }
            }
        }
    }

    private func add() {
        guard let mail = model.mail else { return }
        let (id, account, target) = (invitation.messageId, account, target)
        sending = true
        Task.detached {
            let done = Result { try mail.addInvitation(accountId: account, messageId: id, calendar: target, picked: []) }
            await MainActor.run {
                sending = false
                switch done {
                case .success(let done):
                    went = done.went
                    if !done.toast.isEmpty { model.say(done.toast) }
                    model.loadInvitation()
                case .failure(let error):
                    model.say("\(error)")
                }
            }
        }
    }
}

/// "4 of 6 said yes", which opens who said what.
private struct GuestsButton: View {
    let invitation: MailInvitation
    @State private var open = false

    var body: some View {
        Button(invitation.saidYes) { open.toggle() }
            .buttonStyle(.link)
            .help(invitation.guestSummary)
            .popover(isPresented: $open, arrowEdge: .bottom) {
                VStack(alignment: .leading, spacing: 6) {
                    Text(invitation.guestSummary).font(.headline)
                    ForEach(Array(invitation.guests.enumerated()), id: \.offset) { _, guest in
                        HStack {
                            Text(guest.name)
                            Spacer(minLength: 16)
                            Text(guest.said).foregroundStyle(.secondary).font(.caption)
                        }
                    }
                }
                .padding(12)
                .frame(minWidth: 240)
            }
    }
}

/// Pick a Time: any day and hour to ask the organizer for.
private struct PickTimeSheet: View {
    let organizer: String
    @State var start: Date
    let done: (Int64?) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text(tr("Propose New Time")).font(.headline)
            Text(tr("The organizer decides. {organizer} hears what you suggest.")
                .replacingOccurrences(of: "{organizer}", with: organizer.isEmpty ? tr("Nobody") : organizer))
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
            DatePicker(tr("Starts"), selection: $start)
                .datePickerStyle(.graphical)
            HStack {
                Spacer()
                Button(tr("Cancel"), role: .cancel) { done(nil) }
                    .keyboardShortcut(.cancelAction)
                Button(tr("Propose")) { done(Int64(start.timeIntervalSince1970 * 1000)) }
                    .keyboardShortcut(.defaultAction)
            }
        }
        .padding(20)
        .frame(width: 360)
    }
}

/// The hours around the invitation: the person's other events as tinted
/// blocks, this meeting dashed until it is answered, hairlines on the
/// hours, and whether the hour is free.
struct DayStripView: View {
    let strip: DayStrip
    /// Whether the person has answered, which makes this meeting solid.
    let answered: Bool

    private static let row: CGFloat = 22
    private static let gap: CGFloat = 3
    private static let labels: CGFloat = 16

    var body: some View {
        VStack(alignment: .leading, spacing: 5) {
            HStack {
                Text(strip.heading).font(.callout.weight(.semibold))
                Spacer()
                Text(strip.verdict)
                    .font(.callout)
                    .foregroundStyle(strip.free ? Color.green : Color.orange)
            }
            GeometryReader { geometry in
                let width = geometry.size.width
                let rows = CGFloat(max(strip.lanes, 1))
                let grid = rows * Self.row + (rows - 1) * Self.gap
                ZStack(alignment: .topLeading) {
                    ForEach(Array(strip.hours.enumerated()), id: \.offset) { _, hour in
                        Rectangle()
                            .fill(Color.secondary.opacity(0.25))
                            .frame(width: 1, height: grid)
                            .offset(x: width * hour.at)
                        Text(hour.label)
                            .font(.caption2)
                            .foregroundStyle(.secondary)
                            .offset(x: width * hour.at + 3, y: grid + 2)
                    }
                    ForEach(Array(strip.blocks.enumerated()), id: \.offset) { _, block in
                        StripBlockView(block: block, dashed: block.this && !answered)
                            .frame(width: max(width * (block.to - block.from) - 2, 4), height: Self.row)
                            .offset(x: width * block.from + 1, y: CGFloat(block.lane) * (Self.row + Self.gap))
                    }
                }
            }
            .frame(height: CGFloat(max(strip.lanes, 1)) * (Self.row + Self.gap) + Self.labels)
            .clipped()
        }
        .frame(maxWidth: 520)
        .accessibilityElement(children: .combine)
        .accessibilityLabel(strip.heading + ", " + strip.verdict)
    }
}

/// One event on the strip, in its calendar's colour.
private struct StripBlockView: View {
    let block: StripBlock
    let dashed: Bool

    var body: some View {
        let color = Color(hex: block.color) ?? .accentColor
        let shape = RoundedRectangle(cornerRadius: 5)
        HStack(spacing: 4) {
            if !dashed {
                RoundedRectangle(cornerRadius: 1).fill(color).frame(width: 3)
            }
            Text(block.title)
                .font(.caption)
                .lineLimit(1)
                .truncationMode(.tail)
            Spacer(minLength: 0)
        }
        .padding(.horizontal, 4)
        .padding(.vertical, 2)
        .background(color.opacity(dashed ? 0.08 : 0.22), in: shape)
        .overlay {
            if dashed { shape.strokeBorder(color, style: StrokeStyle(lineWidth: 1, dash: [4, 3])) }
        }
    }
}
