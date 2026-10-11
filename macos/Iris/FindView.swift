import AppKit
import OSLog
import SwiftUI
import WebKit

private let log = Logger(subsystem: "io.github.AlbertoBarrago.Iris", category: "find")

/// Finds in the conversation's page: WebKit highlights the match and
/// scrolls to it, and a count over the page's text says where it stands,
/// "2 of 5", as the GTK app's find bar does.
@MainActor
@Observable
final class PageFinder {
    /// The page, set by the view that shows it.
    @ObservationIgnored weak var view: WKWebView?
    private(set) var position = 0
    private(set) var total = 0
    @ObservationIgnored private var query = ""

    /// Finds `text` from the top of the page.
    func find(_ text: String) {
        query = text
        position = 0
        total = 0
        guard let view, !text.isEmpty else { return }
        Task {
            total = await count(text, in: view)
            // A new query starts from the top, not from the last match.
            _ = try? await view.evaluateJavaScript("window.getSelection().removeAllRanges()", in: nil, contentWorld: .defaultClient)
            if total > 0 { step(forward: true) }
        }
    }

    /// The next match, or the one before, wrapping at either end.
    func step(forward: Bool) {
        guard let view, !query.isEmpty, total > 0 else { return }
        let configuration = WKFindConfiguration()
        configuration.backwards = !forward
        configuration.wraps = true
        configuration.caseSensitive = Self.caseSensitive(query)
        view.find(query, configuration: configuration) { [weak self] result in
            guard let self, result.matchFound else { return }
            Task { @MainActor in
                if forward {
                    self.position = self.position >= self.total ? 1 : self.position + 1
                } else {
                    self.position = self.position <= 1 ? self.total : self.position - 1
                }
            }
        }
    }

    /// A new page finished loading, such as the one with every message
    /// open: the query is found again in it. Counting while it loads reads
    /// an empty page, or the old one with its messages closed.
    func pageLoaded() {
        if !query.isEmpty { find(query) }
    }

    /// What the bar says: "2 of 5 matches", "No matches", or nothing yet.
    var said: String {
        if query.isEmpty { return "" }
        if total == 0 { return tr("No matches") }
        return trPlural("{position} of {total} match", "{position} of {total} matches", total)
            .replacingOccurrences(of: "{position}", with: "\(max(position, 1))")
            .replacingOccurrences(of: "{total}", with: "\(total)")
    }

    /// Matches case only when the query has a capital, as the GTK app does.
    static func caseSensitive(_ query: String) -> Bool {
        query.contains { $0.isUppercase }
    }

    /// How often `text` occurs in what the page shows. An HTML body sits
    /// in a shadow root of its own, which the page's text leaves out, so
    /// each root's text is read too; WebKit's find searches them as well.
    /// The page's own scripts are off, so this runs in the app's own
    /// script world.
    private func count(_ text: String, in view: WKWebView) async -> Int {
        let script = """
        const parts = [document.body ? document.body.innerText : ""];
        for (const host of document.querySelectorAll("*")) {
            if (!host.shadowRoot) continue;
            for (const child of host.shadowRoot.children) {
                if (child.tagName !== "STYLE") parts.push(child.innerText || "");
            }
        }
        const body = parts.join("\\n");
        const hay = sensitive ? body : body.toLowerCase();
        const needle = sensitive ? query : query.toLowerCase();
        let found = 0, at = hay.indexOf(needle);
        while (needle.length > 0 && at !== -1) { found++; at = hay.indexOf(needle, at + needle.length); }
        return found;
        """
        do {
            let answer = try await view.callAsyncJavaScript(
                script,
                arguments: ["query": text, "sensitive": Self.caseSensitive(text)],
                in: nil,
                contentWorld: .defaultClient
            )
            return (answer as? NSNumber)?.intValue ?? 0
        } catch {
            log.error("find: script failed \(error, privacy: .public)")
            return 0
        }
    }
}

/// The bar over the conversation: the query, where it stands, the
/// previous and next match, and Done.
struct FindBar: View {
    @Environment(MailModel.self) private var model
    @State private var text = ""
    @FocusState private var focused: Bool

    var body: some View {
        let finder = model.finder
        HStack(spacing: 8) {
            Image(systemName: "magnifyingglass").foregroundStyle(.secondary)
            TextField(tr("Find in the conversation"), text: $text)
                .textFieldStyle(.plain)
                .focused($focused)
                .onSubmit { finder.step(forward: true) }
                .onChange(of: text) { finder.find(text) }
                .onKeyPress(.escape) {
                    model.findShown = false
                    return .handled
                }
            Text(finder.said)
                .font(.callout.monospacedDigit())
                .foregroundStyle(.secondary)
                .contentTransition(.numericText())
            Button {
                finder.step(forward: false)
            } label: {
                Image(systemName: "chevron.up")
            }
            .help(tr("Previous Match"))
            .accessibilityLabel(tr("Previous Match"))
            .keyboardShortcut("g", modifiers: [.command, .shift])
            Button {
                finder.step(forward: true)
            } label: {
                Image(systemName: "chevron.down")
            }
            .help(tr("Next Match"))
            .accessibilityLabel(tr("Next Match"))
            .keyboardShortcut("g", modifiers: .command)
            Button(tr("Close")) { model.findShown = false }
        }
        .buttonStyle(.borderless)
        .padding(.horizontal, 12)
        .padding(.vertical, 7)
        .onAppear { focused = true }
        .onDisappear { finder.find("") }
    }
}

/// ⌘F, as the GTK app answers it: Find in the conversation while the
/// conversation has the keys, the list's search otherwise, and a
/// composer's own find in a composer.
@MainActor
enum Finding {
    static func find(_ model: MailModel) {
        let window = NSApp.keyWindow
        let responder = window?.firstResponder as? NSView
        if let responder, responder is NSTextView, window != NSApp.mainWindow || isComposer(window) {
            let item = NSMenuItem()
            item.tag = Int(NSFindPanelAction.showFindPanel.rawValue)
            NSApp.sendAction(#selector(NSTextView.performFindPanelAction(_:)), to: nil, from: item)
            return
        }
        if let responder, isInPage(responder), model.openThread != nil {
            model.findShown = true
        } else {
            model.searchOpen = true
        }
    }

    /// Whether `view` is the conversation's page or inside it.
    private static func isInPage(_ view: NSView) -> Bool {
        var current: NSView? = view
        while let at = current {
            if at is WKWebView { return true }
            current = at.superview
        }
        return false
    }

    /// A composer is a window of its own, with no conversation list.
    private static func isComposer(_ window: NSWindow?) -> Bool {
        window?.identifier?.rawValue.hasPrefix("compose") == true
    }
}

/// Prints the open conversation with macOS's own print panel, as the GTK
/// app's Print does with WebKit's.
@MainActor
enum PagePrinter {
    static func print(_ view: WKWebView?) {
        guard let view, let window = view.window else { return }
        let info = NSPrintInfo.shared.copy() as? NSPrintInfo ?? NSPrintInfo.shared
        info.horizontalPagination = .fit
        info.verticalPagination = .automatic
        info.isVerticallyCentered = false
        let operation = view.printOperation(with: info)
        // WebKit's operation draws nothing unless its view has a size.
        operation.view?.frame = view.bounds
        operation.showsPrintPanel = true
        operation.showsProgressPanel = true
        operation.runModal(for: window, delegate: nil, didRun: nil, contextInfo: nil)
    }
}
