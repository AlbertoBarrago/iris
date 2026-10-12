import SwiftUI

/// Help, Keyboard Shortcuts: every key Iris answers, by what it is for,
/// as the GTK app's shortcuts window listed them. Only the keys this app
/// binds are here; the words are the GTK window's, already translated.
struct ShortcutsView: View {
    private struct Shortcut: Identifiable {
        let id = UUID()
        let keys: String
        let words: String
    }

    private struct Group: Identifiable {
        let id = UUID()
        let title: String
        let shortcuts: [Shortcut]
    }

    private var groups: [Group] {
        [
            Group(title: tr("Reading"), shortcuts: [
                Shortcut(keys: "⌘F", words: tr("Search")),
                Shortcut(keys: "⌘G  ⇧⌘G", words: tr("Find in the conversation")),
                Shortcut(keys: "⌘O", words: tr("Open in a new window")),
                Shortcut(keys: "⌥⌘U", words: tr("View source")),
                Shortcut(keys: "⌘P", words: tr("Print")),
            ]),
            Group(title: tr("Organizing"), shortcuts: [
                Shortcut(keys: "⌥⌘A  e", words: tr("Archive")),
                Shortcut(keys: "⌫", words: tr("Move to trash")),
                Shortcut(keys: "⇧⌘J", words: tr("Junk")),
                Shortcut(keys: "⇧⌘U  u", words: tr("Mark read or unread")),
                Shortcut(keys: "⇧M", words: tr("Mute or unmute")),
                Shortcut(keys: "l", words: tr("Labels")),
                Shortcut(keys: "⌘Z", words: tr("Undo")),
            ]),
            Group(title: tr("Writing"), shortcuts: [
                Shortcut(keys: "⌘N", words: tr("New message")),
                Shortcut(keys: "⌘R", words: tr("Reply")),
                Shortcut(keys: "⇧⌘R", words: tr("Reply all")),
                Shortcut(keys: "⇧⌘F", words: tr("Forward")),
                Shortcut(keys: "⌘↩", words: tr("Send")),
                Shortcut(keys: "⇧⌘A", words: tr("Attach files")),
                Shortcut(keys: "⌘B  ⌘I  ⌘K", words: tr("Bold, italic, link")),
            ]),
            Group(title: tr("Calendar"), shortcuts: [
                Shortcut(keys: "⌥1", words: tr("Show mail")),
                Shortcut(keys: "⌥2", words: tr("Show the calendar")),
                Shortcut(keys: "⌘T", words: tr("Go to today")),
                Shortcut(keys: "d  w  m", words: tr("Day, week or month")),
                Shortcut(keys: "⌘←  ⌘→", words: tr("Previous or next")),
                Shortcut(keys: "n", words: tr("New event")),
            ]),
            Group(title: tr("General"), shortcuts: [
                Shortcut(keys: "⌘,", words: tr("Settings")),
                Shortcut(keys: "⌘J", words: tr("Show or hide the assistant")),
                Shortcut(keys: "⌘?", words: tr("Keyboard shortcuts")),
                Shortcut(keys: "⌘W", words: tr("Close window")),
                Shortcut(keys: "⌘Q", words: tr("Quit")),
            ]),
        ]
    }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 22) {
                ForEach(groups) { group in
                    VStack(alignment: .leading, spacing: 8) {
                        Text(group.title).font(.headline)
                        ForEach(group.shortcuts) { shortcut in
                            HStack(alignment: .firstTextBaseline) {
                                Text(shortcut.words)
                                Spacer(minLength: 16)
                                Text(shortcut.keys)
                                    .font(.body.monospaced())
                                    .foregroundStyle(.secondary)
                            }
                            .accessibilityElement(children: .combine)
                        }
                    }
                }
            }
            .padding(24)
        }
        .frame(width: 440, height: 620)
    }
}
