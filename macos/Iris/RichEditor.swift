import AppKit
import Observation
import SwiftUI

/// The composer's words in the Mac's own rich text view, with the GTK
/// composer's formatting bar above it. What the person writes goes to the
/// Rust core as lines (`RichBlock`) of styled runs (`RichSpan`), the rich
/// body the core turns into the same HTML the GTK app sends.
struct RichEditor: NSViewRepresentable {
    @Binding var blocks: [RichBlock]
    /// The text view, handed out so the formatting bar can act on it.
    let controller: RichController

    func makeNSView(context: Context) -> NSScrollView {
        let scroll = AttachingTextView.scrollableTextView()
        scroll.drawsBackground = false
        scroll.hasVerticalScroller = false
        guard let text = scroll.documentView as? AttachingTextView else { return scroll }
        text.acceptFileDrops()
        text.dropped = { [controller] urls, failed in controller.dropped(urls, failed) }
        text.hovering = { [controller] over in controller.hovering(over) }
        text.isRichText = true
        text.isHorizontallyResizable = false
        text.isVerticallyResizable = true
        text.autoresizingMask = [.width]
        text.textContainer?.widthTracksTextView = true
        text.allowsUndo = true
        text.drawsBackground = false
        text.importsGraphics = false
        text.isAutomaticQuoteSubstitutionEnabled = false
        text.isAutomaticDashSubstitutionEnabled = false
        text.usesFontPanel = false
        text.font = RichController.bodyFont
        text.textContainerInset = NSSize(width: 0, height: 6)
        text.typingAttributes = RichController.plainAttributes
        text.delegate = context.coordinator
        text.textStorage?.setAttributedString(RichController.attributed(blocks))
        controller.text = text
        controller.changed = { context.coordinator.push() }
        return scroll
    }

    func updateNSView(_ scroll: NSScrollView, context: Context) {}

    func sizeThatFits(_ proposal: ProposedViewSize, nsView scroll: NSScrollView, context: Context) -> CGSize? {
        guard let width = proposal.width, width.isFinite, width > 0,
              let text = scroll.documentView as? NSTextView else { return nil }
        // SwiftUI tries several widths. Measuring a copy keeps those
        // proposals from changing the live editor's wrapping and cursor.
        let storage = NSTextStorage(attributedString: text.attributedString())
        let layout = NSLayoutManager()
        let container = NSTextContainer(size: NSSize(width: width - 2 * text.textContainerInset.width, height: .greatestFiniteMagnitude))
        container.lineFragmentPadding = text.textContainer?.lineFragmentPadding ?? 0
        layout.addTextContainer(container)
        storage.addLayoutManager(layout)
        layout.ensureLayout(for: container)
        let line = layout.defaultLineHeight(for: text.font ?? RichController.bodyFont)
        let height = ceil(max(line, layout.usedRect(for: container).maxY, layout.extraLineFragmentUsedRect.maxY) + 2 * text.textContainerInset.height)
        if scroll.frame.size != CGSize(width: width, height: height) {
            context.coordinator.revealCaret(in: text)
        }
        return CGSize(width: width, height: height)
    }

    func makeCoordinator() -> Coordinator { Coordinator(self) }

    @MainActor
    final class Coordinator: NSObject, NSTextViewDelegate {
        let parent: RichEditor

        init(_ parent: RichEditor) {
            self.parent = parent
        }

        func textDidChange(_ notification: Notification) {
            push()
        }

        func textViewDidChangeSelection(_ notification: Notification) {
            parent.controller.refreshFormatting()
        }

        /// Reads the text back into lines for the draft.
        func push() {
            guard let text = parent.controller.text, let storage = text.textStorage else { return }
            parent.controller.refreshFormatting()
            parent.blocks = RichController.blocks(storage)
            text.enclosingScrollView?.invalidateIntrinsicContentSize()
            revealCaret(in: text)
        }

        func revealCaret(in text: NSTextView) {
            DispatchQueue.main.async { [weak text] in
                guard let text, let window = text.window, window.firstResponder === text,
                      let outer = text.enclosingScrollView?.superview?.enclosingScrollView,
                      let document = outer.documentView,
                      let storage = text.textStorage, let layout = text.layoutManager,
                      let container = text.textContainer else { return }
                window.contentView?.layoutSubtreeIfNeeded()
                layout.ensureLayout(for: container)
                let end = min(NSMaxRange(text.selectedRange()), storage.length)
                let rect: NSRect
                if storage.length == 0 || (end == storage.length && text.string.hasSuffix("\n")) {
                    rect = layout.extraLineFragmentRect
                } else {
                    let glyph = layout.glyphIndexForCharacter(at: min(end, storage.length - 1))
                    rect = layout.boundingRect(forGlyphRange: NSRange(location: glyph, length: 1), in: container)
                }
                let origin = text.textContainerOrigin
                let caret = rect.offsetBy(dx: origin.x, dy: origin.y).insetBy(dx: -1, dy: -text.textContainerInset.height)
                document.scrollToVisible(document.convert(caret, from: text))
            }
        }

        /// The core chooses the next line kind; AppKit inserts the text.
        func textView(_ textView: NSTextView, doCommandBy selector: Selector) -> Bool {
            guard selector == #selector(NSResponder.insertNewline(_:)) else { return false }
            return parent.controller.newline()
        }
    }
}

/// Acts on the editor for the formatting bar and the shortcuts, and turns
/// its text into lines and back.
@MainActor
@Observable
final class RichController {
    @ObservationIgnored weak var text: NSTextView?
    @ObservationIgnored var changed: () -> Void = {}
    /// Files dropped on the words, with how many could not be had.
    @ObservationIgnored var dropped: ([URL], Int) -> Void = { _, _ in }
    /// A drag carrying files is over the words, or has left them.
    @ObservationIgnored var hovering: (Bool) -> Void = { _ in }

    private(set) var hasSelection = false
    private(set) var style = (bold: false, italic: false, strike: false, code: false)

    /// The toolbar reflects the selection, or the style waiting at the caret.
    func refreshFormatting() {
        guard let text, let storage = text.textStorage else { return }
        let range = text.selectedRange()
        hasSelection = range.length > 0
        if range.length == 0 {
            style = Self.runStyle(text.typingAttributes)
        } else {
            style = (
                Self.all(storage, range) { Self.runStyle($0).bold },
                Self.all(storage, range) { Self.runStyle($0).italic },
                Self.all(storage, range) { Self.runStyle($0).strike },
                Self.all(storage, range) { Self.runStyle($0).code }
            )
        }
    }

    static let blockKey = NSAttributedString.Key("IrisBlock")
    static let bodyFont = NSFont.systemFont(ofSize: NSFont.systemFontSize + 1)
    static let codeFont = NSFont.monospacedSystemFont(ofSize: NSFont.systemFontSize, weight: .regular)
    static var plainAttributes: [NSAttributedString.Key: Any] {
        [.font: bodyFont, .foregroundColor: NSColor.labelColor]
    }
    static let bullet = "•\t"

    // MARK: Character styles

    func toggle(bold: Bool = false, italic: Bool = false, strike: Bool = false, code: Bool = false) {
        guard let text, let storage = text.textStorage else { return }
        text.window?.makeFirstResponder(text)
        defer { refreshFormatting() }
        let range = text.selectedRange()
        if range.length == 0 {
            // No selection: the style applies to what is typed next.
            var typing = text.typingAttributes
            Self.flip(&typing, bold: bold, italic: italic, strike: strike, code: code)
            text.typingAttributes = typing
            return
        }
        // On when any of the selection lacks it, off when all of it has it.
        let turnOn = !Self.all(storage, range) { attrs in Self.has(attrs, bold: bold, italic: italic, strike: strike, code: code) }
        text.undoManager?.beginUndoGrouping()
        storage.beginEditing()
        storage.enumerateAttributes(in: range) { attrs, part, _ in
            var changed = attrs
            if Self.has(attrs, bold: bold, italic: italic, strike: strike, code: code) != turnOn {
                Self.flip(&changed, bold: bold, italic: italic, strike: strike, code: code)
            }
            storage.setAttributes(changed, range: part)
        }
        storage.endEditing()
        text.undoManager?.endUndoGrouping()
        changed()
    }

    private static func all(_ storage: NSTextStorage, _ range: NSRange, _ test: ([NSAttributedString.Key: Any]) -> Bool) -> Bool {
        var every = true
        storage.enumerateAttributes(in: range) { attrs, _, stop in
            if !test(attrs) { every = false; stop.pointee = true }
        }
        return every
    }

    private static func has(_ attrs: [NSAttributedString.Key: Any], bold: Bool, italic: Bool, strike: Bool, code: Bool) -> Bool {
        let style = runStyle(attrs)
        return (!bold || style.bold) && (!italic || style.italic) && (!strike || style.strike) && (!code || style.code)
    }

    private static func flip(_ attrs: inout [NSAttributedString.Key: Any], bold: Bool, italic: Bool, strike: Bool, code: Bool) {
        var style = runStyle(attrs)
        if bold { style.bold.toggle() }
        if italic { style.italic.toggle() }
        if strike { style.strike.toggle() }
        if code { style.code.toggle() }
        let size = (attrs[.font] as? NSFont)?.pointSize ?? bodyFont.pointSize
        attrs[.font] = font(bold: style.bold, italic: style.italic, code: style.code, size: size)
        attrs[.strikethroughStyle] = style.strike ? NSUnderlineStyle.single.rawValue : nil
    }

    // MARK: Lines

    /// Makes the lines the selection touches `kind`, or plain paragraphs
    /// again when they all are `kind` already.
    func setBlock(_ kind: String, level: UInt8 = 0) {
        guard let text, let storage = text.textStorage else { return }
        text.window?.makeFirstResponder(text)
        let string = storage.string as NSString
        let lines = string.paragraphRange(for: text.selectedRange())
        var starts: [Int] = []
        string.enumerateSubstrings(in: lines, options: [.byParagraphs, .substringNotRequired]) { _, range, _, _ in
            starts.append(range.location)
        }
        if starts.isEmpty { starts = [lines.location] }
        let already = starts.allSatisfy { blockAt($0) == (kind, level) }
        let target = already ? ("paragraph", UInt8(0)) : (kind, level)
        text.undoManager?.beginUndoGrouping()
        storage.beginEditing()
        // From the last line up, so earlier offsets stay right as markers
        // come and go.
        var number = 1
        let ordered = starts.sorted()
        var numbers: [Int: Int] = [:]
        for start in ordered { numbers[start] = number; number += 1 }
        for start in ordered.reversed() {
            restyleLine(at: start, as: target, number: numbers[start] ?? 1, in: storage)
        }
        storage.endEditing()
        text.undoManager?.endUndoGrouping()
        changed()
    }

    /// Sets a link on the selection, or takes it off with an empty address.
    func link() {
        guard let text, let storage = text.textStorage else { return }
        let range = text.selectedRange()
        guard range.length > 0 else { return }
        let current = storage.attribute(.link, at: range.location, effectiveRange: nil).map { "\($0)" } ?? "https://"
        let alert = NSAlert()
        alert.messageText = tr("Add a Link")
        let field = NSTextField(string: current)
        field.frame = NSRect(x: 0, y: 0, width: 320, height: 24)
        alert.accessoryView = field
        alert.addButton(withTitle: tr("Add Link"))
        alert.addButton(withTitle: tr("Cancel"))
        guard alert.runModal() == .alertFirstButtonReturn else { return }
        let address = field.stringValue.trimmingCharacters(in: .whitespaces)
        if address.isEmpty || address == "https://" {
            storage.removeAttribute(.link, range: range)
        } else {
            storage.addAttribute(.link, value: address, range: range)
        }
        changed()
    }

    /// Puts `blocks` in where the cursor is, over any selection, as one
    /// step Undo takes back.
    func insert(_ blocks: [RichBlock]) {
        guard let text else { return }
        let range = text.selectedRange()
        let words = Self.attributed(blocks)
        guard text.shouldChangeText(in: range, replacementString: words.string) else { return }
        text.textStorage?.replaceCharacters(in: range, with: words)
        text.didChangeText()
        text.setSelectedRange(NSRange(location: range.location + words.length, length: 0))
        text.window?.makeFirstResponder(text)
        changed()
    }

    /// Takes every style and line kind off the selection.
    func clear() {
        guard let text, let storage = text.textStorage else { return }
        let range = text.selectedRange()
        guard range.length > 0 else { return }
        storage.setAttributes(Self.plainAttributes, range: range)
        changed()
    }

    /// Applies the core's Return behavior to the text and typing attributes.
    func newline() -> Bool {
        guard let text, let storage = text.textStorage else { return false }
        let at = text.selectedRange().location
        let string = storage.string as NSString
        let line = string.paragraphRange(for: NSRange(location: at, length: 0))
        let (kind, level) = blockAt(line.location)
        let content = string.substring(with: line).trimmingCharacters(in: .newlines)
        let marker = marker(kind, number: number(at: line.location))
        let empty = content == marker || content.isEmpty
        let end = line.location + (content as NSString).length
        let following = richBlockAfterReturn(
            kind: kind, level: level, empty: empty,
            atEnd: NSMaxRange(text.selectedRange()) == end
        )
        if following.kind != kind {
            if empty && (kind == "bullet" || kind == "numbered") {
                restyleLine(at: line.location, as: (following.kind, following.level), number: 1, in: storage)
                changed()
            } else {
                let attrs = Self.lineAttributes(following.kind, level: following.level)
                text.insertText(NSAttributedString(string: "\n", attributes: attrs), replacementRange: text.selectedRange())
                text.typingAttributes = attrs
            }
            return true
        }
        guard kind == "bullet" || kind == "numbered" else { return false }
        let next = "\n" + self.marker(kind, number: number(at: line.location) + 1)
        let attrs = Self.lineAttributes(kind, level: 0)
        text.insertText(NSAttributedString(string: next, attributes: attrs), replacementRange: text.selectedRange())
        return true
    }

    private func blockAt(_ location: Int) -> (String, UInt8) {
        guard let storage = text?.textStorage, location < storage.length else {
            if let typing = text?.typingAttributes[Self.blockKey] as? String { return Self.parse(typing) }
            return ("paragraph", 0)
        }
        return Self.parse(storage.attribute(Self.blockKey, at: location, effectiveRange: nil) as? String ?? "paragraph")
    }

    private func number(at location: Int) -> Int {
        guard let storage = text?.textStorage else { return 1 }
        let string = storage.string as NSString
        let line = string.paragraphRange(for: NSRange(location: location, length: 0))
        let words = string.substring(with: line)
        return Int(words.prefix { $0.isNumber }) ?? 1
    }

    private func marker(_ kind: String, number: Int) -> String {
        kind == "bullet" ? Self.bullet : kind == "numbered" ? "\(number).\t" : ""
    }

    private func restyleLine(at start: Int, as target: (String, UInt8), number: Int, in storage: NSTextStorage) {
        let string = storage.string as NSString
        var line = string.paragraphRange(for: NSRange(location: start, length: 0))
        // The old marker goes first.
        let words = string.substring(with: line)
        let old = Self.markerLength(words)
        if old > 0 {
            storage.deleteCharacters(in: NSRange(location: line.location, length: old))
            line.length -= old
        }
        let new = marker(target.0, number: number)
        if !new.isEmpty {
            storage.insert(NSAttributedString(string: new, attributes: Self.lineAttributes(target.0, level: target.1)), at: line.location)
            line.length += (new as NSString).length
        }
        let tag = Self.tag(target.0, level: target.1)
        storage.addAttribute(Self.blockKey, value: tag, range: line)
        storage.addAttribute(.paragraphStyle, value: Self.paragraph(target.0), range: line)
        storage.enumerateAttribute(.font, in: line) { value, part, _ in
            let old = value as? NSFont ?? Self.bodyFont
            let style = Self.runStyle([.font: old])
            storage.addAttribute(.font, value: Self.font(bold: style.bold || target.0 == "heading", italic: style.italic, code: style.code || target.0 == "code", size: Self.size(target.0, level: target.1)), range: part)
        }
        storage.addAttribute(.foregroundColor, value: target.0 == "quote" ? NSColor.secondaryLabelColor : NSColor.labelColor, range: line)
        text?.typingAttributes = Self.lineAttributes(target.0, level: target.1)
    }

    /// How many characters a line's list marker takes, `•\t` or `12.\t`.
    private static func markerLength(_ words: String) -> Int {
        if words.hasPrefix(bullet) { return (bullet as NSString).length }
        let digits = words.prefix { $0.isNumber }
        if !digits.isEmpty, words.dropFirst(digits.count).hasPrefix(".\t") {
            return digits.count + 2
        }
        return 0
    }

    // MARK: Between the text and the core's lines

    /// The editor's text as the core's lines.
    static func blocks(_ storage: NSAttributedString) -> [RichBlock] {
        var out: [RichBlock] = []
        let string = storage.string as NSString
        string.enumerateSubstrings(in: NSRange(location: 0, length: string.length), options: [.byParagraphs, .substringNotRequired]) { _, line, _, _ in
            let tag = line.length > 0 ? storage.attribute(blockKey, at: line.location, effectiveRange: nil) as? String : nil
            let (kind, level) = parse(tag ?? "paragraph")
            var body = line
            let skip = markerLength(string.substring(with: line))
            if kind == "bullet" || kind == "numbered" {
                body.location += skip
                body.length -= skip
            }
            var spans: [RichSpan] = []
            storage.enumerateAttributes(in: body) { attrs, part, _ in
                let words = string.substring(with: part)
                let style = runStyle(attrs)
                let link = attrs[.link].map { ($0 as? URL)?.absoluteString ?? "\($0)" }
                spans.append(RichSpan(text: words, bold: style.bold && kind != "heading", italic: style.italic, strike: style.strike, code: style.code && kind != "code", link: link, image: nil))
            }
            out.append(RichBlock(kind: kind, level: level, spans: spans))
        }
        if string.length == 0 || string.hasSuffix("\n") {
            out.append(RichBlock(kind: "paragraph", level: 0, spans: []))
        }
        return out
    }

    /// The core's lines as text for the editor.
    static func attributed(_ blocks: [RichBlock]) -> NSAttributedString {
        let out = NSMutableAttributedString()
        var number = 0
        for (index, block) in blocks.enumerated() {
            number = block.kind == "numbered" ? number + 1 : 0
            let attrs = lineAttributes(block.kind, level: block.level)
            let marker = block.kind == "bullet" ? bullet : block.kind == "numbered" ? "\(number).\t" : ""
            out.append(NSAttributedString(string: marker, attributes: attrs))
            for span in block.spans where span.image == nil {
                var run = attrs
                run[.font] = font(bold: span.bold || block.kind == "heading", italic: span.italic, code: span.code || block.kind == "code", size: size(block.kind, level: block.level))
                if span.strike { run[.strikethroughStyle] = NSUnderlineStyle.single.rawValue }
                if let link = span.link { run[.link] = link }
                out.append(NSAttributedString(string: span.text, attributes: run))
            }
            if index < blocks.count - 1 {
                out.append(NSAttributedString(string: "\n", attributes: attrs))
            }
        }
        return out
    }

    private static func lineAttributes(_ kind: String, level: UInt8) -> [NSAttributedString.Key: Any] {
        [
            .font: font(bold: kind == "heading", italic: false, code: kind == "code", size: size(kind, level: level)),
            .foregroundColor: kind == "quote" ? NSColor.secondaryLabelColor : NSColor.labelColor,
            .paragraphStyle: paragraph(kind),
            blockKey: tag(kind, level: level),
        ]
    }

    private static func paragraph(_ kind: String) -> NSParagraphStyle {
        let style = NSMutableParagraphStyle()
        switch kind {
        case "bullet", "numbered":
            style.headIndent = 22
            style.tabStops = [NSTextTab(textAlignment: .left, location: 22)]
            style.defaultTabInterval = 22
        case "quote":
            style.firstLineHeadIndent = 14
            style.headIndent = 14
        default:
            break
        }
        style.paragraphSpacing = 2
        return style
    }

    private static func size(_ kind: String, level: UInt8) -> CGFloat {
        guard kind == "heading" else { return bodyFont.pointSize }
        switch level {
        case 1: return bodyFont.pointSize + 9
        case 2: return bodyFont.pointSize + 5
        default: return bodyFont.pointSize + 2
        }
    }

    private static func tag(_ kind: String, level: UInt8) -> String {
        kind == "heading" ? "heading\(level)" : kind
    }

    private static func parse(_ tag: String) -> (String, UInt8) {
        if tag.hasPrefix("heading"), let level = UInt8(tag.dropFirst("heading".count)) {
            return ("heading", level)
        }
        return (tag, 0)
    }

    private static func font(bold: Bool, italic: Bool, code: Bool, size: CGFloat) -> NSFont {
        var font = code ? NSFont.monospacedSystemFont(ofSize: size - 1, weight: bold ? .bold : .regular) : NSFont.systemFont(ofSize: size, weight: bold ? .bold : .regular)
        if italic {
            font = NSFontManager.shared.convert(font, toHaveTrait: .italicFontMask)
        }
        return font
    }

    private static func runStyle(_ attrs: [NSAttributedString.Key: Any]) -> (bold: Bool, italic: Bool, strike: Bool, code: Bool) {
        let font = attrs[.font] as? NSFont ?? bodyFont
        let traits = font.fontDescriptor.symbolicTraits
        let strike = (attrs[.strikethroughStyle] as? Int ?? 0) != 0
        return (traits.contains(.bold), traits.contains(.italic), strike, traits.contains(.monoSpace))
    }
}

/// Formatting controls grouped by paragraph, character style, list and link.
struct FormatBar: View {
    let controller: RichController

    var body: some View {
        HStack(spacing: 2) {
            Menu {
                Button(tr("Paragraph")) { controller.setBlock("paragraph") }
                Button(tr("Heading 1")) { controller.setBlock("heading", level: 1) }
                Button(tr("Heading 2")) { controller.setBlock("heading", level: 2) }
                Button(tr("Heading 3")) { controller.setBlock("heading", level: 3) }
            } label: {
                Label(tr("Paragraph"), systemImage: "textformat.size")
            }
            .menuStyle(.borderlessButton)
            .fixedSize()
            .help(tr("Paragraph"))
            Divider().frame(height: 16).padding(.horizontal, 6)
            button("bold", "Bold (Ctrl+B)", "b", [], active: controller.style.bold) { controller.toggle(bold: true) }
            button("italic", "Italic (Ctrl+I)", "i", [], active: controller.style.italic) { controller.toggle(italic: true) }
            button("strikethrough", "Strikethrough (Ctrl+Shift+X)", "x", .shift, active: controller.style.strike) { controller.toggle(strike: true) }
            button("chevron.left.forwardslash.chevron.right", "Code (Ctrl+E)", "e", [], active: controller.style.code) { controller.toggle(code: true) }
            Divider().frame(height: 16).padding(.horizontal, 4)
            button("list.bullet", "Bulleted List (Ctrl+Shift+8)", "8", .shift) { controller.setBlock("bullet") }
            button("list.number", "Numbered List (Ctrl+Shift+7)", "7", .shift) { controller.setBlock("numbered") }
            button("text.quote", "Quote (Ctrl+Shift+9)", "9", .shift) { controller.setBlock("quote") }
            Divider().frame(height: 16).padding(.horizontal, 4)
            button("link", "Link (Ctrl+K)", "k", []) { controller.link() }
                .disabled(!controller.hasSelection)
            Button(tr("Clear Formatting"), systemImage: "textformat") { controller.clear() }
                .labelStyle(.iconOnly)
                .help(tr("Clear Formatting"))
                .disabled(!controller.hasSelection)
                .frame(width: 26, height: 22)
            Spacer()
        }
        .buttonStyle(.borderless)
        .padding(.horizontal, 20)
        .padding(.vertical, 8)
        .background(.bar)
    }

    /// Shows the formatting action with its Mac shortcut and current state.
    private func button(_ symbol: String, _ words: String, _ key: Character, _ extra: EventModifiers, active: Bool = false, run: @escaping () -> Void) -> some View {
        let name = macKeys(tr(words))
        return Button(name, systemImage: symbol, action: run)
            .keyboardShortcut(KeyEquivalent(key), modifiers: extra.union(.command))
            .labelStyle(.iconOnly)
            .help(name)
            .frame(width: 26, height: 22)
            .background(active ? Color.accentColor.opacity(0.18) : Color.clear, in: RoundedRectangle(cornerRadius: 4))
            .foregroundStyle(active ? Color.accentColor : Color.primary)
            .accessibilityAddTraits(active ? .isSelected : [])
    }
}

/// "Bold (Ctrl+B)" as a Mac names the keys: "Bold (⌘B)".
func macKeys(_ words: String) -> String {
    words
        .replacingOccurrences(of: "Ctrl+Shift+", with: "⇧⌘")
        .replacingOccurrences(of: "Ctrl+", with: "⌘")
}

/// The composer's text view, which attaches files dropped on it rather
/// than writing their paths into the message, as a text view does. Text
/// dragged in or about inside it is the text view's as before.
final class AttachingTextView: NSTextView {
    var dropped: ([URL], Int) -> Void = { _, _ in }
    var hovering: (Bool) -> Void = { _ in }
    /// The drag over the view carries files.
    private var carrying = false

    func acceptFileDrops() {
        registerForDraggedTypes(registeredDraggedTypes + FileDrop.types)
    }

    override func readSelection(from pboard: NSPasteboard) -> Bool {
        guard let type = preferredPasteboardType(from: pboard.types ?? [], restrictedToTypesFrom: nil) else { return false }
        return readSelection(from: pboard, type: type)
    }

    /// Rich text from a web page or another app goes through the core's
    /// lines on the way in, as the sent mail does. Its colors, backgrounds
    /// and fonts would otherwise stay in the editor, black on a dark
    /// window, while the mail drops them; what survives is bold, italic,
    /// strikethrough, code and links.
    override func readSelection(from pboard: NSPasteboard, type: NSPasteboard.PasteboardType) -> Bool {
        // Let AppKit decode the requested format without putting its source
        // attributes into the editor. Its preferred types can use legacy
        // names, so a list of modern pasteboard types misses those paths.
        let imported = NSTextView()
        imported.isRichText = true
        imported.importsGraphics = false
        guard imported.readSelection(from: pboard, type: type), let pasted = imported.textStorage else {
            return super.readSelection(from: pboard, type: type)
        }
        let words = RichController.attributed(RichController.blocks(pasted))
        let range = rangeForUserTextChange
        guard range.location != NSNotFound, shouldChangeText(in: range, replacementString: words.string) else { return false }
        textStorage?.replaceCharacters(in: range, with: words)
        didChangeText()
        setSelectedRange(NSRange(location: range.location + words.length, length: 0))
        typingAttributes = RichController.plainAttributes
        return true
    }

    override func draggingEntered(_ sender: NSDraggingInfo) -> NSDragOperation {
        carrying = FileDrop.carriesFiles(sender.draggingPasteboard)
        guard carrying else { return super.draggingEntered(sender) }
        hovering(true)
        return .copy
    }

    override func draggingUpdated(_ sender: NSDraggingInfo) -> NSDragOperation {
        carrying ? .copy : super.draggingUpdated(sender)
    }

    override func draggingExited(_ sender: NSDraggingInfo?) {
        guard carrying else { return super.draggingExited(sender) }
        carrying = false
        hovering(false)
    }

    override func prepareForDragOperation(_ sender: NSDraggingInfo) -> Bool {
        carrying ? true : super.prepareForDragOperation(sender)
    }

    override func performDragOperation(_ sender: NSDraggingInfo) -> Bool {
        guard carrying else { return super.performDragOperation(sender) }
        FileDrop.read(sender.draggingPasteboard) { [weak self] urls, failed in
            self?.dropped(urls, failed)
        }
        return true
    }

    override func concludeDragOperation(_ sender: NSDraggingInfo?) {
        guard carrying else { return super.concludeDragOperation(sender) }
        carrying = false
        hovering(false)
    }
}
