import AppKit
import SwiftUI
import UniformTypeIdentifiers

/// Settings, Writing, Signing and Encryption, as the GTK Preferences
/// group has it: what gpg and gpgsm hold for the person's addresses, an
/// Import button for each, and the two switches. With neither program on
/// this Mac there is no section at all.
struct ProtectionSection: View {
    @Environment(MailModel.self) private var model
    @State private var settings: ProtectionSettings?
    @State private var report: ImportReport?
    @State private var importing = false

    var body: some View {
        if let settings {
            Section {
                if !settings.keysTitle.isEmpty {
                    row(settings.keysTitle, settings.keysLine, kind: "pgp", import: settings.importTitle)
                }
                if !settings.certificatesTitle.isEmpty {
                    row(settings.certificatesTitle, settings.certificatesLine, kind: "smime", import: settings.importTitle)
                }
                toggle(settings.signTitle, settings.signSubtitle, "sign_by_default", settings.signByDefault)
                toggle(settings.encryptTitle, settings.encryptSubtitle, "encrypt_when_possible", settings.encryptWhenPossible)
            } header: {
                Text(settings.title)
            } footer: {
                Text(settings.description).foregroundStyle(.secondary)
            }
            .alert(report?.heading ?? "", isPresented: Binding(get: { report != nil }, set: { if !$0 { report = nil } })) {
                Button(tr("Close")) {}
            } message: {
                Text(([report?.body ?? ""] + (report?.lines ?? [])).joined(separator: "\n"))
            }
        } else {
            Color.clear.frame(height: 0).task { load() }
        }
    }

    private func row(_ title: String, _ line: String, kind: String, import label: String) -> some View {
        HStack {
            VStack(alignment: .leading, spacing: 2) {
                Text(title)
                Text(line).font(.callout).foregroundStyle(.secondary)
            }
            Spacer()
            if importing { ProgressView().controlSize(.small) }
            Button(label) { pick(kind, prompt: label.replacingOccurrences(of: "…", with: "")) }
                .disabled(importing)
        }
    }

    private func toggle(_ title: String, _ subtitle: String, _ name: String, _ value: Bool) -> some View {
        Toggle(isOn: Binding(get: { value }, set: { on in
            model.setPreference(name, on)
            load()
        })) {
            Text(title)
            Text(subtitle)
        }
    }

    private func load() {
        guard let mail = model.mail else { return }
        Task.detached {
            let found = mail.protectionSettings()
            await MainActor.run { settings = found }
        }
    }

    /// Asks for a key or certificate file and imports it. gpg or gpgsm
    /// may ask for the passphrase in a window of its own meanwhile.
    private func pick(_ kind: String, prompt: String) {
        let panel = NSOpenPanel()
        panel.allowsMultipleSelection = false
        panel.canChooseDirectories = false
        panel.prompt = prompt
        guard panel.runModal() == .OK, let url = panel.url, let data = try? Data(contentsOf: url),
              let mail = model.mail else { return }
        importing = true
        Task.detached {
            let said = mail.importKeys(kind: kind, file: data)
            await MainActor.run {
                importing = false
                report = said
                load()
            }
        }
    }
}
