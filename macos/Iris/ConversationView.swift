import AppKit
import SwiftUI
import WebKit

/// The open conversation: one page the Rust core draws, the same page the
/// GTK app shows, in the window's colors.
struct ConversationView: View {
    @Environment(MailModel.self) private var model
    @Environment(\.colorScheme) private var colorScheme

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
                    MailPage(html: page.html, pictures: model.pictures) { url in model.followLink(url) }
                }
            } else {
                ContentUnavailableView(tr("No Conversation Selected"), systemImage: "envelope")
            }
        }
        .toolbar {
            // Reply and Forward arrive with the composer.
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
            ToolbarItemGroup {
                Group {
                    Button(tr("Archive"), systemImage: "archivebox") { model.perform(.archive) }
                        .keyboardShortcut("a", modifiers: [.command, .option])
                    Button(tr("Delete"), systemImage: "trash") { model.perform(.trash) }
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
    /// What a click on one of the page's own `mailrs:` links asks for.
    var onLink: (URL) -> Void = { _ in }

    func makeCoordinator() -> Coordinator { Coordinator(onLink: onLink) }

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
        return view
    }

    func updateNSView(_ view: WKWebView, context: Context) {
        guard context.coordinator.loaded != html else { return }
        context.coordinator.loaded = html
        view.loadHTMLString(html, baseURL: nil)
    }

    final class Coordinator: NSObject, WKNavigationDelegate {
        var loaded: String?
        let onLink: (URL) -> Void

        init(onLink: @escaping (URL) -> Void) {
            self.onLink = onLink
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

/// What a click on an address offers, as a menu where the pointer is.
/// New Message waits for the composer.
@MainActor
enum ContactMenu {
    static func show(for address: String) {
        let menu = NSMenu()
        let title = NSMenuItem(title: address, action: nil, keyEquivalent: "")
        title.isEnabled = false
        menu.addItem(title)
        menu.addItem(.separator())
        let copy = MenuAction(title: tr("Copy Address")) {
            NSPasteboard.general.clearContents()
            NSPasteboard.general.setString(address, forType: .string)
        }
        menu.addItem(copy.item)
        let write = NSMenuItem(title: tr("New Message"), action: nil, keyEquivalent: "")
        write.isEnabled = false
        menu.addItem(write)
        menu.popUp(positioning: nil, at: NSEvent.mouseLocation, in: nil)
        _ = copy
    }
}

/// A menu item that runs a closure, kept alive by the item itself.
final class MenuAction: NSObject {
    let item: NSMenuItem
    private let run: () -> Void

    init(title: String, run: @escaping () -> Void) {
        self.run = run
        item = NSMenuItem(title: title, action: nil, keyEquivalent: "")
        super.init()
        item.target = self
        item.action = #selector(fire)
        item.representedObject = self
    }

    @objc private func fire() { run() }
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
