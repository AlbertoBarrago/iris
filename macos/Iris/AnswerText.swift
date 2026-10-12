import AppKit
import SwiftUI

/// An answer of the assistant's in AppKit's own text view rather than a
/// SwiftUI `Text`. A selectable `Text` shows the text cursor over its links
/// as over its words, so a link read like a place to type; here a link shows
/// the pointing hand, the words stay selectable, and a click on a link goes
/// to `open`. The Markdown is the inline kind the model writes: bold,
/// italic, code, strikethrough and links.
struct AnswerText: NSViewRepresentable {
    let markdown: String
    let open: (URL) -> Void

    func makeNSView(context: Context) -> NSTextView {
        let text = NSTextView(usingTextLayoutManager: false)
        text.isEditable = false
        text.isSelectable = true
        text.drawsBackground = false
        text.isVerticallyResizable = false
        text.textContainerInset = .zero
        text.textContainer?.lineFragmentPadding = 0
        text.textContainer?.widthTracksTextView = true
        text.linkTextAttributes = [
            .foregroundColor: NSColor.linkColor,
            .underlineStyle: NSUnderlineStyle.single.rawValue,
            .cursor: NSCursor.pointingHand,
        ]
        text.delegate = context.coordinator
        return text
    }

    func updateNSView(_ text: NSTextView, context: Context) {
        context.coordinator.open = open
        guard context.coordinator.shown != markdown else { return }
        context.coordinator.shown = markdown
        text.textStorage?.setAttributedString(Self.attributed(markdown))
    }

    /// As tall as the answer is at the width offered.
    func sizeThatFits(_ proposal: ProposedViewSize, nsView text: NSTextView, context: Context) -> CGSize? {
        guard let width = proposal.width, width.isFinite, let container = text.textContainer,
              let layout = text.layoutManager else { return nil }
        container.containerSize = NSSize(width: width, height: .greatestFiniteMagnitude)
        layout.ensureLayout(for: container)
        return CGSize(width: width, height: ceil(layout.usedRect(for: container).height))
    }

    func makeCoordinator() -> Coordinator { Coordinator(open: open) }

    @MainActor
    final class Coordinator: NSObject, NSTextViewDelegate {
        var open: (URL) -> Void
        /// The Markdown on show, so an unchanged answer is not set again
        /// and a selection in it survives the pane redrawing.
        var shown: String?

        init(open: @escaping (URL) -> Void) {
            self.open = open
        }

        func textView(_ textView: NSTextView, clickedOnLink link: Any, at charIndex: Int) -> Bool {
            let url = link as? URL ?? (link as? String).flatMap(URL.init(string:))
            if let url { open(url) }
            return true
        }
    }

    /// The model's inline Markdown in the body font, its styles as font
    /// traits and its links as links.
    static func attributed(_ markdown: String) -> NSAttributedString {
        let options = AttributedString.MarkdownParsingOptions(interpretedSyntax: .inlineOnlyPreservingWhitespace)
        let parsed = (try? AttributedString(markdown: markdown, options: options)) ?? AttributedString(markdown)
        let body = NSFont.systemFont(ofSize: NSFont.systemFontSize)
        let out = NSMutableAttributedString()
        for run in parsed.runs {
            let intent = run.inlinePresentationIntent ?? []
            var font = intent.contains(.code)
                ? NSFont.monospacedSystemFont(ofSize: body.pointSize - 1, weight: .regular)
                : body
            var traits: NSFontDescriptor.SymbolicTraits = []
            if intent.contains(.stronglyEmphasized) { traits.insert(.bold) }
            if intent.contains(.emphasized) { traits.insert(.italic) }
            if !traits.isEmpty {
                font = NSFont(descriptor: font.fontDescriptor.withSymbolicTraits(traits), size: font.pointSize) ?? font
            }
            var attributes: [NSAttributedString.Key: Any] = [.font: font, .foregroundColor: NSColor.labelColor]
            if intent.contains(.strikethrough) {
                attributes[.strikethroughStyle] = NSUnderlineStyle.single.rawValue
            }
            if let link = run.link {
                attributes[.link] = link
            }
            out.append(NSAttributedString(string: String(parsed[run.range].characters), attributes: attributes))
        }
        return out
    }
}
