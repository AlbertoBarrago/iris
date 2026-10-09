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
            if let page = model.page {
                MailPage(html: page)
            } else {
                ContentUnavailableView("No Conversation Selected", systemImage: "envelope")
            }
        }
        .toolbar {
            // The actions arrive with syncing; the bar is already where
            // the GTK app has it.
            ToolbarItemGroup {
                Button("Reply", systemImage: "arrowshape.turn.up.left") {}
                Button("Reply All", systemImage: "arrowshape.turn.up.left.2") {}
                Button("Forward", systemImage: "arrowshape.turn.up.right") {}
            }
            ToolbarItemGroup {
                Button("Archive", systemImage: "archivebox") {}
                Button("Delete", systemImage: "trash") {}
                Button("Junk", systemImage: "xmark.bin") {}
            }
            ToolbarItemGroup {
                Button("Label", systemImage: "tag") {}
                Button("Flag", systemImage: "flag") {}
            }
        }
        .disabled(model.page == nil)
        .onAppear { model.theme = PageTheme.current(dark: colorScheme == .dark) }
        .onChange(of: colorScheme) { model.theme = PageTheme.current(dark: colorScheme == .dark) }
        .onReceive(NotificationCenter.default.publisher(for: NSColor.systemColorsDidChangeNotification)) { _ in
            model.theme = PageTheme.current(dark: colorScheme == .dark)
        }
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

    func makeCoordinator() -> Coordinator { Coordinator() }

    func makeNSView(context: Context) -> WKWebView {
        let configuration = WKWebViewConfiguration()
        configuration.websiteDataStore = .nonPersistent()
        configuration.defaultWebpagePreferences.allowsContentJavaScript = false
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
            if ["http", "https", "mailto"].contains(url.scheme?.lowercased() ?? "") {
                NSWorkspace.shared.open(url)
            }
        }
    }
}
