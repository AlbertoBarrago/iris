import AppKit

/// The suggestions under the search field, in a panel of their own that
/// never becomes key. A SwiftUI popover is a window that can take the
/// keys: with keyboard navigation on, or on macOS 27, it took them from
/// the field while the person typed, and letters went missing. This panel
/// leaves the keys with the field, which moves the mark with the arrow
/// keys and picks with Return; a click on a row picks it too.
@MainActor
final class SearchSuggestionPanel {
    var picked: (SearchSuggestion) -> Void = { _ in }

    private let window = SuggestionWindow()
    private let list = NSStackView()
    private var suggestions: [SearchSuggestion] = []
    private var markedIndex: Int?

    private static let rowHeight: CGFloat = 24
    private static let inset: CGFloat = 6
    private static let minimumWidth: CGFloat = 260

    init() {
        let background = NSVisualEffectView()
        background.material = .menu
        background.state = .active
        background.wantsLayer = true
        background.layer?.cornerRadius = 8
        background.layer?.masksToBounds = true

        list.orientation = .vertical
        list.spacing = 0
        list.alignment = .leading
        list.edgeInsets = NSEdgeInsets(top: Self.inset, left: 0, bottom: Self.inset, right: 0)
        list.translatesAutoresizingMaskIntoConstraints = false
        list.setAccessibilityRole(.list)
        list.setAccessibilityLabel(tr("Search suggestions"))
        background.addSubview(list)
        NSLayoutConstraint.activate([
            list.leadingAnchor.constraint(equalTo: background.leadingAnchor),
            list.trailingAnchor.constraint(equalTo: background.trailingAnchor),
            list.topAnchor.constraint(equalTo: background.topAnchor),
        ])
        window.contentView = background
    }

    var isShowing: Bool { window.isVisible }

    /// The suggestion the arrow keys marked, which Return picks.
    var marked: SearchSuggestion? {
        guard isShowing, let markedIndex, suggestions.indices.contains(markedIndex) else { return nil }
        return suggestions[markedIndex]
    }

    /// Shows `suggestions` under `field`, keeping the mark on the same
    /// suggestion when it is still there.
    func show(_ suggestions: [SearchSuggestion], under field: NSView) {
        guard let parent = field.window else { return }
        if suggestions != self.suggestions {
            let kept = marked.flatMap { suggestions.firstIndex(of: $0) }
            self.suggestions = suggestions
            markedIndex = kept
            rebuild()
        }
        let fieldFrame = parent.convertToScreen(field.convert(field.bounds, to: nil))
        let width = max(fieldFrame.width, Self.minimumWidth)
        let height = CGFloat(suggestions.count) * Self.rowHeight + 2 * Self.inset
        window.setFrame(NSRect(x: fieldFrame.minX, y: fieldFrame.minY - height - 4, width: width, height: height), display: true)
        if window.parent !== parent {
            window.parent?.removeChildWindow(window)
            parent.addChildWindow(window, ordered: .above)
        }
        window.orderFront(nil)
    }

    func close() {
        window.parent?.removeChildWindow(window)
        window.orderOut(nil)
        suggestions = []
        markedIndex = nil
    }

    /// Moves the mark `step` rows, from no mark to the first or last row.
    func mark(by step: Int) {
        guard !suggestions.isEmpty else { return }
        let next = markedIndex.map { $0 + step } ?? (step > 0 ? 0 : suggestions.count - 1)
        markedIndex = min(max(next, 0), suggestions.count - 1)
        drawMark()
        if let marked {
            // The keys stay in the field, so VoiceOver hears the row the
            // mark moved to from an announcement.
            NSAccessibility.post(
                element: list,
                notification: .announcementRequested,
                userInfo: [.announcement: marked.label, .priority: NSAccessibilityPriorityLevel.high.rawValue]
            )
        }
    }

    private func rebuild() {
        list.arrangedSubviews.forEach { $0.removeFromSuperview() }
        for (index, suggestion) in suggestions.enumerated() {
            let row = SuggestionRow(label: suggestion.label)
            row.clicked = { [weak self] in
                guard let self, self.suggestions.indices.contains(index) else { return }
                self.picked(self.suggestions[index])
            }
            row.hovered = { [weak self] in
                self?.markedIndex = index
                self?.drawMark()
            }
            list.addArrangedSubview(row)
            NSLayoutConstraint.activate([
                row.widthAnchor.constraint(equalTo: list.widthAnchor),
                row.heightAnchor.constraint(equalToConstant: Self.rowHeight),
            ])
        }
        drawMark()
    }

    private func drawMark() {
        for (index, view) in list.arrangedSubviews.enumerated() {
            (view as? SuggestionRow)?.isMarked = index == markedIndex
        }
    }
}

/// A borderless panel that refuses to become key or main, so the window
/// under it keeps the keys.
private final class SuggestionWindow: NSPanel {
    init() {
        super.init(contentRect: .zero, styleMask: [.borderless, .nonactivatingPanel], backing: .buffered, defer: true)
        isOpaque = false
        backgroundColor = .clear
        hasShadow = true
        level = .popUpMenu
        // A child window would otherwise stay over another app's windows.
        hidesOnDeactivate = true
        becomesKeyOnlyIfNeeded = true
    }

    override var canBecomeKey: Bool { false }
    override var canBecomeMain: Bool { false }
}

/// One suggestion: its words, a highlight while marked, a click picks it.
private final class SuggestionRow: NSView {
    var clicked: () -> Void = {}
    var hovered: () -> Void = {}
    var isMarked = false {
        didSet {
            guard isMarked != oldValue else { return }
            highlight.isHidden = !isMarked
            text.textColor = isMarked ? .selectedMenuItemTextColor : .labelColor
        }
    }

    private let text: NSTextField
    private let highlight = NSView()

    init(label: String) {
        text = NSTextField(labelWithString: label)
        super.init(frame: .zero)
        translatesAutoresizingMaskIntoConstraints = false
        highlight.wantsLayer = true
        highlight.layer?.backgroundColor = NSColor.selectedContentBackgroundColor.cgColor
        highlight.layer?.cornerRadius = 4
        highlight.isHidden = true
        text.lineBreakMode = .byTruncatingTail
        text.translatesAutoresizingMaskIntoConstraints = false
        highlight.translatesAutoresizingMaskIntoConstraints = false
        addSubview(highlight)
        addSubview(text)
        NSLayoutConstraint.activate([
            highlight.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 5),
            highlight.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -5),
            highlight.topAnchor.constraint(equalTo: topAnchor),
            highlight.bottomAnchor.constraint(equalTo: bottomAnchor),
            text.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 10),
            text.trailingAnchor.constraint(lessThanOrEqualTo: trailingAnchor, constant: -10),
            text.centerYAnchor.constraint(equalTo: centerYAnchor),
        ])
        setAccessibilityElement(true)
        setAccessibilityRole(.button)
        setAccessibilityLabel(label)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("init(coder:) is not used") }

    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        trackingAreas.forEach(removeTrackingArea)
        addTrackingArea(NSTrackingArea(rect: bounds, options: [.mouseEnteredAndExited, .activeAlways], owner: self))
    }

    override func mouseEntered(with event: NSEvent) { hovered() }

    // The panel is never key, so the first click has to count.
    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }

    override func mouseDown(with event: NSEvent) {}

    override func mouseUp(with event: NSEvent) {
        if bounds.contains(convert(event.locationInWindow, from: nil)) { clicked() }
    }

    override func accessibilityPerformPress() -> Bool {
        clicked()
        return true
    }
}
