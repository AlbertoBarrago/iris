import AppKit
import SwiftUI
import UniformTypeIdentifiers

extension MessageSource: Identifiable {
    public var id: String { fileName + String(raw.count) }
}

/// View Source: the newest message of the conversation as it arrived,
/// headers and all, in a fixed-width face, with Copy and Save As, as the
/// GTK app's window had it.
struct SourceView: View {
    @Environment(MailModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let source: MessageSource

    var body: some View {
        VStack(spacing: 0) {
            ScrollView {
                Text(source.text)
                    .font(.system(.callout, design: .monospaced))
                    .textSelection(.enabled)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(14)
            }
            .accessibilityLabel(tr("Message Source"))
            Divider()
            HStack {
                Text(source.subject).foregroundStyle(.secondary).lineLimit(1)
                Spacer()
                Button(tr("Copy")) {
                    NSPasteboard.general.clearContents()
                    NSPasteboard.general.setString(source.text, forType: .string)
                }
                Button(tr("Save As…")) { save() }
                    .help(tr("Save as an .eml file"))
                Button(tr("Close")) { dismiss() }
                    .keyboardShortcut(.cancelAction)
            }
            .padding(12)
        }
        .frame(minWidth: 640, idealWidth: 760, minHeight: 480, idealHeight: 640)
        .navigationTitle(tr("Message Source"))
    }

    private func save() {
        let panel = NSSavePanel()
        panel.title = tr("Save Message")
        panel.nameFieldStringValue = source.fileName
        guard panel.runModal() == .OK, let url = panel.url else { return }
        do {
            try source.raw.write(to: url)
            model.say(tr("Saved {file}").replacingOccurrences(of: "{file}", with: url.lastPathComponent))
        } catch {
            model.say(error.localizedDescription)
        }
    }
}

extension MailModel {
    /// Opens View Source on the open conversation's newest message.
    func viewSource() {
        guard let mail, let key = openThread else { return }
        Task.detached {
            let found = Result { try mail.messageSource(accountId: key.account, threadId: key.thread) }
            await MainActor.run {
                switch found {
                case .success(let source): self.source = source
                case .failure(let error): self.say("\(error)")
                }
            }
        }
    }

    /// Saves the conversations picked as one mbox file where the person
    /// says.
    func exportSelection() {
        guard let mail, !selection.isEmpty else { return }
        let threads = selection.map { ThreadRef(accountId: $0.account, threadId: $0.thread) }
        say(tr("Exporting…"))
        Task.detached {
            let found = Result { try mail.exportMbox(threads: threads) }
            await MainActor.run {
                switch found {
                case .success(let exported):
                    let panel = NSSavePanel()
                    panel.title = tr("Export Mail")
                    panel.nameFieldStringValue = exported.fileName
                    guard panel.runModal() == .OK, let url = panel.url else { return }
                    do {
                        try exported.data.write(to: url)
                        self.say(tr("Saved {file}").replacingOccurrences(of: "{file}", with: url.lastPathComponent))
                    } catch {
                        self.say(error.localizedDescription)
                    }
                case .failure(let error): self.say("\(error)")
                }
            }
        }
    }
}
