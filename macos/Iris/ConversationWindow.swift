import SwiftUI

/// A conversation in a window of its own, titled with its subject, with
/// the reading pane's actions: reply, archive, delete, flag, remind. It
/// keeps its own open and closed messages, apart from the main window's.
struct ConversationWindow: View {
    @Environment(MailModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let key: ThreadKey

    @State private var page: ConversationPage?
    @State private var toggled: [String] = []
    @State private var title = ""

    var body: some View {
        Group {
            if let page {
                MailPage(html: page.html, pictures: model.pictures, zoom: model.prefs?.textZoom ?? 1) { url in follow(url) }
            } else {
                ProgressView().frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        .navigationTitle(title)
        .task(id: toggled) {
            title = model.windowTitle(key)
            page = await model.windowPage(key, toggled: toggled)
        }
        .toolbar {
            ToolbarItemGroup {
                Button(tr("Reply"), systemImage: "arrowshape.turn.up.left") { answer(.reply) }
                Button(tr("Reply All"), systemImage: "arrowshape.turn.up.left.2") { answer(.replyAll) }
                Button(tr("Forward"), systemImage: "arrowshape.turn.up.right") { answer(.forward) }
            }
            ToolbarItemGroup {
                // Mail that leaves takes its window with it.
                Button(tr("Archive"), systemImage: "archivebox") { leave(.archive) }
                Button(model.deleteWords, systemImage: "trash") { leave(.trash) }
                Button(tr("Junk"), systemImage: "xmark.bin") { leave(.junk) }
            }
            ToolbarItemGroup {
                FlagMenu(keys: [key])
                RemindMenu(keys: [key])
            }
        }
    }

    private func answer(_ kind: ComposeRequest.Kind) {
        model.composeAsked = ComposeRequest(kind: kind, account: key.account, thread: key.thread)
    }

    private func leave(_ action: Action) {
        model.perform(action, on: [key])
        dismiss()
    }

    /// A message's header opens or closes it in this window; the rest goes
    /// to the model, told which account the page belongs to.
    private func follow(_ url: URL) {
        if let id = url.absoluteString.removingPrefix("mailrs:toggle/") {
            if let at = toggled.firstIndex(of: id) { toggled.remove(at: at) } else { toggled.append(id) }
        } else {
            model.followLink(url, account: key.account)
        }
    }
}
