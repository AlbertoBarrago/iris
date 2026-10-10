import SwiftUI

/// Picks a day and a time, for Send Later's and Remind Me's "Choose a
/// Time…", as the GTK app's picker does: from an hour from now, in steps
/// of five minutes.
struct TimePickerSheet: View {
    @Environment(\.dismiss) private var dismiss
    let title: String
    let explanation: String
    let confirm: String
    let chosen: (Int64) -> Void

    @State private var day = TimePickerSheet.start()
    @State private var hour = Calendar.current.component(.hour, from: TimePickerSheet.start())
    @State private var minute = Calendar.current.component(.minute, from: TimePickerSheet.start())

    /// The day picked at the hour and minute picked.
    private var when: Date {
        Calendar.current.date(bySettingHour: hour, minute: minute, second: 0, of: day) ?? day
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text(title).font(.title3.weight(.semibold))
            Text(explanation).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
            // The calendar picks the day; the hour and the minute, in steps
            // of five, sit under it, as in the GTK app's picker.
            DatePicker("", selection: $day, in: Calendar.current.startOfDay(for: Date())..., displayedComponents: .date)
                .datePickerStyle(.graphical)
                .labelsHidden()
            HStack(spacing: 6) {
                Picker("", selection: $hour) {
                    ForEach(0..<24, id: \.self) { Text(String(format: "%02d", $0)).tag($0) }
                }
                .labelsHidden()
                .frame(width: 70)
                Text(":")
                Picker("", selection: $minute) {
                    ForEach(Array(stride(from: 0, to: 60, by: 5)), id: \.self) { Text(String(format: "%02d", $0)).tag($0) }
                }
                .labelsHidden()
                .frame(width: 70)
                Spacer()
            }
            HStack {
                Spacer()
                Button(tr("Cancel"), role: .cancel) { dismiss() }
                    .keyboardShortcut(.cancelAction)
                Button(confirm) {
                    chosen(Int64(when.timeIntervalSince1970 * 1000))
                    dismiss()
                }
                .keyboardShortcut(.defaultAction)
                .buttonStyle(.borderedProminent)
                // A time already gone would go out, or come back, at once.
                .disabled(when <= Date())
            }
        }
        .padding(20)
        .frame(width: 340)
    }

    /// An hour from now, on the next five minutes.
    private static func start() -> Date {
        let later = Date().addingTimeInterval(3600)
        let minute = Calendar.current.component(.minute, from: later)
        let round = (5 - minute % 5) % 5
        let rounded = later.addingTimeInterval(TimeInterval(round * 60))
        return Calendar.current.date(bySetting: .second, value: 0, of: rounded) ?? rounded
    }
}

/// Remind Me on the conversations picked: an hour from now, tonight,
/// tomorrow morning, next Monday, or a time of one's own.
struct RemindMenu: View {
    @Environment(MailModel.self) private var model
    var keys: Set<ThreadKey>? = nil
    @State private var choosing = false

    var body: some View {
        Menu(tr("Remind Me"), systemImage: "clock") {
            ForEach(model.remindTimes(), id: \.at) { time in
                Button(time.label) { model.perform(.remind(at: time.at), on: keys) }
            }
            Divider()
            Button(tr("Choose a Time…")) { choosing = true }
        }
        .sheet(isPresented: $choosing) {
            TimePickerSheet(
                title: tr("Remind Me"),
                explanation: tr("The conversation leaves the inbox now and comes back at this time, marked unread."),
                confirm: tr("Remind Me")
            ) { at in model.perform(.remind(at: at), on: keys) }
        }
    }
}
