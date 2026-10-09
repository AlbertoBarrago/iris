import SwiftUI
import WebKit

/// The messages of one conversation: a header for each, the newest open.
struct ConversationView: View {
    let messages: [MessageItem]
    @State private var shown: String?

    var body: some View {
        if messages.isEmpty {
            ContentUnavailableView("No Conversation Selected", systemImage: "envelope")
        } else {
            VStack(spacing: 0) {
                ForEach(messages, id: \.id) { message in
                    Button {
                        shown = message.id
                    } label: {
                        MessageHeader(message: message, open: message.id == current?.id)
                    }
                    .buttonStyle(.plain)
                    Divider()
                }
                if let message = current {
                    if let page = message.page {
                        MailPage(html: page)
                    } else {
                        ContentUnavailableView(
                            "Not Downloaded Yet",
                            systemImage: "arrow.down.circle",
                            description: Text("Open this conversation in Iris once to fetch it.")
                        )
                    }
                }
            }
            .onChange(of: messages.map(\.id)) { shown = nil }
        }
    }

    private var current: MessageItem? {
        messages.first { $0.id == shown } ?? messages.last
    }
}

struct MessageHeader: View {
    let message: MessageItem
    let open: Bool

    var body: some View {
        HStack(alignment: .firstTextBaseline) {
            VStack(alignment: .leading, spacing: 2) {
                Text(message.from).font(open ? .headline : .body)
                if open {
                    Text("To: \(message.to)").font(.caption).foregroundStyle(.secondary)
                }
            }
            Spacer()
            Text(Date(timeIntervalSince1970: Double(message.date) / 1000), format: .dateTime)
                .font(.caption)
                .foregroundStyle(.secondary)
        }
        .padding(.horizontal, 20)
        .padding(.vertical, 8)
        .contentShape(Rectangle())
    }
}

/// A message body in a WKWebView of its own. It is the window's own view,
/// so selection, Copy and scrolling work as they do in any Mac app. Links
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
