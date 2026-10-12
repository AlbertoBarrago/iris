import AppKit
import SwiftUI
import WebKit

/// The open conversation: one page the Rust core draws, the same page the
/// GTK app shows, in the window's colors.
struct ConversationView: View {
    @Environment(MailModel.self) private var model
    @Environment(\.colorScheme) private var colorScheme
    @State private var leaving = false

    var body: some View {
        Group {
            if model.selection.count > 1 {
                SeveralSelected()
            } else if let page = model.page {
                VStack(spacing: 0) {
                    if page.remoteHidden {
                        HStack {
                            Text(tr("Remote images are hidden to protect your privacy"))
                            Spacer()
                            Button(tr("Load Images")) { model.loadImages() }
                        }
                        .padding(.horizontal, 16)
                        .padding(.vertical, 8)
                        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 12))
                        .padding(10)
                    }
                    if let mark = model.protectionMarks.last {
                        ProtectionBar(mark: mark)
                    }
                    if let card = model.translationCard {
                        TranslationBar(card: card)
                    }
                    if let offer = model.listOffer {
                        HStack {
                            Text(tr("This message is from a mailing list"))
                            Spacer()
                            Button(tr("Unsubscribe")) { leaving = true }
                        }
                        .padding(.horizontal, 16)
                        .padding(.vertical, 8)
                        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 12))
                        .padding(10)
                        .alert(offer.heading, isPresented: $leaving) {
                            Button(tr("Cancel"), role: .cancel) {}
                            Button(tr("Unsubscribe")) { model.unsubscribe() }
                                .keyboardShortcut(.defaultAction)
                        } message: {
                            Text(offer.line.prefix(1).uppercased() + offer.line.dropFirst())
                        }
                    }
                    if model.findShown {
                        FindBar()
                        Divider()
                    }
                    if let invitation = model.invitation, let key = model.openThread {
                        InvitationCard(invitation: invitation, account: key.account)
                            .id(invitation.messageId + invitation.answer)
                            .layoutPriority(1)
                    }
                    MailPage(html: page.html, pictures: model.pictures, zoom: model.prefs?.textZoom ?? 1, finder: model.finder) { url in model.followLink(url) }
                        .frame(minHeight: 120)
                }
            } else {
                ContentUnavailableView(tr("No Conversation Selected"), systemImage: "envelope")
            }
        }
        .toolbar {
            // Reply and Forward arrive with the composer.
            if model.openHoldsDraft {
                // A draft is edited, not answered, as in the GTK app.
                ToolbarItem {
                    Button(tr("Edit Draft"), systemImage: "pencil") { answer(.editDraft) }
                        .keyboardShortcut("e", modifiers: .command)
                        .disabled(model.openThread == nil)
                }
            } else {
            ToolbarItemGroup {
                Group {
                    Button(tr("Reply"), systemImage: "arrowshape.turn.up.left") { answer(.reply) }
                        .keyboardShortcut("r", modifiers: .command)
                    Button(tr("Reply All"), systemImage: "arrowshape.turn.up.left.2") { answer(.replyAll) }
                        .keyboardShortcut("r", modifiers: [.command, .shift])
                    Button(tr("Forward"), systemImage: "arrowshape.turn.up.right") { answer(.forward) }
                        .keyboardShortcut("f", modifiers: [.command, .shift])
                }
                .disabled(model.openThread == nil)
            }
            }
            ToolbarItemGroup {
                Group {
                    Button(tr("Archive"), systemImage: "archivebox") { model.perform(.archive) }
                        .keyboardShortcut("a", modifiers: [.command, .option])
                    Button(model.deleteWords, systemImage: model.inSendLater || model.inReminders ? "xmark.circle" : "trash") { model.perform(.trash) }
                    Button(tr("Junk"), systemImage: "xmark.bin") { model.perform(.junk) }
                        .keyboardShortcut("j", modifiers: [.command, .shift])
                }
                .disabled(model.selection.isEmpty)
            }
            ToolbarItemGroup {
                Group {
                    Button(
                        model.selectionIsRead ? tr("Mark as Unread") : tr("Mark as Read"),
                        systemImage: model.selectionIsRead ? "envelope.badge" : "envelope.open"
                    ) { model.toggleRead() }
                        .keyboardShortcut("u", modifiers: [.command, .shift])
                    FlagMenu()
                    RemindMenu()
                    FilingButton()
                    SenderButton()
                }
                .disabled(model.selection.isEmpty)
            }
        }
        .onAppear { model.theme = PageTheme.current(dark: colorScheme == .dark) }
        .onChange(of: colorScheme) { model.theme = PageTheme.current(dark: colorScheme == .dark) }
        .onReceive(NotificationCenter.default.publisher(for: NSColor.systemColorsDidChangeNotification)) { _ in
            model.theme = PageTheme.current(dark: colorScheme == .dark)
        }
    }
}

extension ConversationView {
    /// Opens a composer on a reply to, or a forward of, the open conversation.
    func answer(_ kind: ComposeRequest.Kind) {
        guard let key = model.openThread else { return }
        model.composeAsked = ComposeRequest(kind: kind, account: key.account, thread: key.thread)
    }
}

extension PageTheme {
    /// The page colors for the current appearance: the system accent, and
    /// for text the accent moved toward the side of the page that gives it
    /// contrast.
    static func current(dark: Bool) -> PageTheme {
        let accent = NSColor.controlAccentColor.usingColorSpace(.sRGB) ?? .systemBlue
        let text = (dark ? accent.blended(withFraction: 0.35, of: .white) : accent.blended(withFraction: 0.25, of: .black)) ?? accent
        return PageTheme(dark: dark, accent: accent.cssHex, accentText: text.cssHex)
    }
}

extension NSColor {
    /// `#rrggbb` for CSS.
    var cssHex: String {
        let color = usingColorSpace(.sRGB) ?? self
        let channel = { (value: CGFloat) in Int((value * 255).rounded()).clamped(to: 0...255) }
        return String(format: "#%02x%02x%02x", channel(color.redComponent), channel(color.greenComponent), channel(color.blueComponent))
    }
}

extension Comparable {
    func clamped(to range: ClosedRange<Self>) -> Self {
        min(max(self, range.lowerBound), range.upperBound)
    }
}

/// A page in a WKWebView of its own. It is the window's own view, so
/// selection, Copy and scrolling work as they do in any Mac app. Links
/// open in the browser; the page itself never navigates.
struct MailPage: NSViewRepresentable {
    let html: String
    /// Answers the page's `mailrs-cid:` requests for pictures in the mail.
    var pictures: PictureScheme? = nil
    /// How much the page is magnified, from Preferences' Text Size.
    var zoom: Double = 1
    /// Finds in the page, for the find bar.
    var finder: PageFinder? = nil
    /// What a click on one of the page's own `mailrs:` links asks for.
    var onLink: (URL) -> Void = { _ in }

    func makeCoordinator() -> Coordinator { Coordinator(onLink: onLink, finder: finder) }

    func makeNSView(context: Context) -> WKWebView {
        let configuration = WKWebViewConfiguration()
        configuration.websiteDataStore = .nonPersistent()
        configuration.defaultWebpagePreferences.allowsContentJavaScript = false
        if let pictures {
            configuration.setURLSchemeHandler(pictures, forURLScheme: "mailrs-cid")
        }
        let view = WKWebView(frame: .zero, configuration: configuration)
        view.navigationDelegate = context.coordinator
        // The page paints its own background, the window's color.
        view.underPageBackgroundColor = .windowBackgroundColor
        finder?.view = view
        return view
    }

    func updateNSView(_ view: WKWebView, context: Context) {
        if view.pageZoom != zoom { view.pageZoom = zoom }
        finder?.view = view
        guard context.coordinator.loaded != html else { return }
        context.coordinator.loaded = html
        view.loadHTMLString(html, baseURL: nil)
    }

    final class Coordinator: NSObject, WKNavigationDelegate {
        var loaded: String?
        let onLink: (URL) -> Void
        weak var finder: PageFinder?

        init(onLink: @escaping (URL) -> Void, finder: PageFinder?) {
            self.onLink = onLink
            self.finder = finder
        }

        func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) {
            let finder = finder
            Task { @MainActor in finder?.pageLoaded() }
        }

        func webView(
            _ webView: WKWebView,
            decidePolicyFor action: WKNavigationAction,
            decisionHandler: @escaping @MainActor @Sendable (WKNavigationActionPolicy) -> Void
        ) {
            guard action.navigationType == .linkActivated, let url = action.request.url else {
                decisionHandler(.allow)
                return
            }
            decisionHandler(.cancel)
            switch url.scheme?.lowercased() {
            case "http", "https": NSWorkspace.shared.open(url)
            case "mailrs", "mailto": onLink(url)
            default: break
            }
        }
    }
}

/// The Labels button, or Move to Folder on an account that files in
/// folders, with its picker; L opens it from the list.
struct FilingButton: View {
    @Environment(MailModel.self) private var model

    var body: some View {
        @Bindable var model = model
        let menu = model.selection.isEmpty ? nil : model.filingMenu()
        Button(menu?.name ?? tr("Labels"), systemImage: menu?.folders == true ? "folder" : "tag") {
            model.filingShown.toggle()
        }
        .help(menu?.tooltip ?? "")
        .popover(isPresented: $model.filingShown, arrowEdge: .bottom) {
            FilingPicker()
        }
    }
}

/// What the reading pane shows while several conversations are picked:
/// how to act on all of them at once, as the GTK app's pane says.
struct SeveralSelected: View {
    @Environment(MailModel.self) private var model

    var body: some View {
        VStack(spacing: 18) {
            Image(systemName: "tray.full")
                .font(.system(size: 56))
                .foregroundStyle(.secondary)
            Text(tr("Several Conversations Selected"))
                .font(.title2.weight(.bold))
            Text(tr("Actions and shortcuts apply to all of them. Esc clears the selection."))
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
            FlowLayout(spacing: 10) {
                Button(tr("Archive")) { model.perform(.archive) }
                    .buttonStyle(.borderedProminent)
                Button(model.selectionIsRead ? tr("Mark as Unread") : tr("Mark as Read")) { model.toggleRead() }
                Button(tr("Flag")) { model.perform(.flag(color: "red")) }
                Button(tr("Mute")) { model.perform(.mute) }
                Button(tr("Junk")) { model.perform(.junk) }
                Button(tr("Move to Trash")) { model.perform(.trash) }
            }
            .buttonStyle(.bordered)
            .buttonBorderShape(.capsule)
            .controlSize(.large)
            .frame(maxWidth: 520)
        }
        .padding(40)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

/// Gives the page the pictures its mail names by `cid:`, asked for at
/// `mailrs-cid:<account>/<message>/<version>/<content id>`. The bytes come
/// from the Rust core, which fetches the part from the cache or the
/// server, and never enter the page's own HTML.
@MainActor
final class PictureScheme: NSObject, WKURLSchemeHandler {
    private let mail: Mail?
    /// Requests the page took back, which must not be answered.
    private var stopped = Set<ObjectIdentifier>()

    init(mail: Mail?) {
        self.mail = mail
    }

    func webView(_ webView: WKWebView, start task: any WKURLSchemeTask) {
        guard let mail, let url = task.request.url, let address = Self.address(url) else {
            task.didFailWithError(URLError(.fileDoesNotExist))
            return
        }
        // WebKit hands the task over on the main thread and wants its
        // answer there; the box carries it across the fetch.
        let asked = Asked(task: task)
        Task.detached {
            let part = try? mail.picture(accountId: address.account, messageId: address.message, cid: address.cid)
            await self.answer(asked, url: url, part: part ?? nil)
        }
    }

    func webView(_ webView: WKWebView, stop task: any WKURLSchemeTask) {
        stopped.insert(ObjectIdentifier(task))
    }

    private func answer(_ asked: Asked, url: URL, part: FilePart?) {
        let task = asked.task
        guard !stopped.contains(ObjectIdentifier(task)) else { return }
        guard let part else {
            task.didFailWithError(URLError(.fileDoesNotExist))
            return
        }
        let response = URLResponse(url: url, mimeType: part.mimeType, expectedContentLength: part.data.count, textEncodingName: nil)
        task.didReceive(response)
        task.didReceive(Data(part.data))
        task.didFinish()
    }

    /// Reads `mailrs-cid:<account>/<message>/<version>/<content id>`.
    private static func address(_ url: URL) -> (account: Int64, message: String, cid: String)? {
        guard let rest = url.absoluteString.removingPrefix("mailrs-cid:") else { return nil }
        let parts = rest.split(separator: "/", maxSplits: 3).map(String.init)
        guard parts.count == 4, let account = Int64(parts[0]) else { return nil }
        let decode = { (text: String) in text.removingPercentEncoding ?? text }
        return (account, decode(parts[1]), decode(parts[3]))
    }
}

/// A WebKit task carried to a background fetch and back to the main
/// thread, where alone it is touched.
private struct Asked: @unchecked Sendable {
    let task: any WKURLSchemeTask
}

/// The card offering to translate the message, or to turn a translation
/// back to what arrived, as the GTK conversation's card does.
private struct TranslationBar: View {
    @Environment(MailModel.self) private var model
    let card: TranslationOffer

    var body: some View {
        HStack(spacing: 12) {
            Image(systemName: "character.bubble")
                .font(.title3)
                .foregroundStyle(.secondary)
            VStack(alignment: .leading, spacing: 2) {
                Text(card.title).fontWeight(.semibold)
                if !card.detail.isEmpty {
                    Text(card.detail)
                        .font(.callout)
                        .foregroundStyle(card.problem ? Color.red : Color.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
            Spacer()
            if model.translating { ProgressView().controlSize(.small) }
            Button(card.button) { model.translateOpen() }
                .disabled(model.translating)
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 8)
        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 12))
        .padding(10)
        .accessibilityElement(children: .contain)
    }
}

/// What gpg or gpgsm found in the conversation's newest protected
/// message, as the GTK card says it: a shield when the text is the
/// signer's, a warning when something is wrong, an open lock when
/// nothing here could check it.
private struct ProtectionBar: View {
    let mark: ProtectionMark

    var body: some View {
        HStack(spacing: 12) {
            Image(systemName: symbol)
                .font(.title3)
                .foregroundStyle(color)
            VStack(alignment: .leading, spacing: 2) {
                Text(mark.title).fontWeight(.semibold)
                if !mark.detail.isEmpty {
                    Text(mark.detail)
                        .font(.callout)
                        .foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
            Spacer()
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 8)
        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 12))
        .padding(10)
        .accessibilityElement(children: .combine)
        .accessibilityLabel(spoken + ", " + mark.title)
    }

    private var symbol: String {
        switch mark.tone {
        case "good": "checkmark.shield.fill"
        case "bad": "exclamationmark.triangle.fill"
        default: "lock.open"
        }
    }

    private var color: Color {
        switch mark.tone {
        case "good": .green
        case "bad": .orange
        default: .secondary
        }
    }

    private var spoken: String {
        switch mark.tone {
        case "good": tr("Checked")
        case "bad": tr("Warning")
        default: tr("Not checked")
        }
    }
}

/// What Iris does about the person who wrote the conversation: sort their
/// mail into a category, or block them, as the GTK window's sender menu
/// did. Each leaves a rule on the server for the mail still to come.
struct SenderButton: View {
    @Environment(MailModel.self) private var model
    @State private var blocking = false

    var body: some View {
        let menu = model.senderMenu
        Menu {
            if let menu, !menu.categories.isEmpty {
                Menu(tr("Categorize Sender")) {
                    ForEach(menu.categories, id: \.key) { category in
                        Button(category.label) { model.categorizeSender(category.key) }
                    }
                }
            }
            if let menu, menu.canBlock {
                Button(tr("Block Sender…"), role: .destructive) { blocking = true }
            }
        } label: {
            Label(menu?.sender ?? "", systemImage: "person.crop.circle")
        }
        .help(menu?.sender ?? "")
        .disabled(menu == nil || (menu?.categories.isEmpty ?? true) && !(menu?.canBlock ?? false))
        .alert(menu?.blockHeading ?? "", isPresented: $blocking) {
            Button(tr("Cancel"), role: .cancel) {}
            Button(tr("Block"), role: .destructive) { model.blockSender() }
        } message: {
            Text(menu?.blockBody ?? "")
        }
    }
}
