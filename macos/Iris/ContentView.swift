import AppKit
import SwiftUI

/// The mail window: the sidebar, the list of the mailbox picked, and the
/// open conversation, laid out as the GTK app lays them out.
struct ContentView: View {
    @Environment(MailModel.self) private var model
    @Environment(AssistantModel.self) private var assistant
    @Environment(\.undoManager) private var undoManager
    @Environment(\.openWindow) private var openWindow

    var body: some View {
        Group {
            if model.noAccounts {
                WelcomeView()
            } else {
                mail
            }
        }
        .alert(model.erasing?.question.heading ?? "", isPresented: Binding(
            get: { model.erasing != nil },
            set: { if !$0 { model.erasing = nil } }
        ), presenting: model.erasing) { request in
            Button(tr("Cancel"), role: .cancel) {}
            Button(request.question.verb, role: .destructive) { model.erase(request) }
        } message: { request in
            Text(request.question.body)
        }
        .sheet(item: Bindable(model).accountSheet) { sheet in
            switch sheet {
            case .autoReply(let account): AutoReplyView(account: account)
            case .rules(let account): RulesView(account: account)
            case .hideMyEmail(let account): HideMyEmailView(account: account)
            }
        }
        .sheet(item: Bindable(model).contactCard) { card in
            ContactCardView(card: card)
        }
        .sheet(item: Bindable(model).addAccount) { request in
            AddAccountView(request: request).environment(model)
        }
    }

    private var mail: some View {
        Group {
            // Mail and Calendar, the two spaces the switch at the top of
            // the sidebar flips between, as the GTK window's.
            if model.space == .calendar {
                NavigationSplitView {
                    CalendarSidebar()
                        .navigationSplitViewColumnWidth(min: 220, ideal: 260, max: 340)
                } detail: {
                    CalendarPage()
                }
            } else {
                NavigationSplitView {
                    SidebarView()
                        .navigationSplitViewColumnWidth(min: 220, ideal: 260, max: 340)
                } content: {
                    ThreadListView()
                        .navigationSplitViewColumnWidth(min: 340, ideal: 440, max: 560)
                } detail: {
                    ConversationView()
                }
            }
        }
        // The assistant takes room of its own on the right: the window
        // grows by the pane's width as it opens, so the list and the
        // conversation keep theirs, and gives it back as it closes.
        .inspector(isPresented: Bindable(assistant).shown) {
            AssistantPane()
                .inspectorColumnWidth(min: AssistantPane.width, ideal: AssistantPane.width, max: 560)
        }
        .onChange(of: assistant.shown) { _, shown in
            WindowRoom.make(shown, width: AssistantPane.width)
        }
        .overlay(alignment: .topTrailing) {
            if let toast = model.toast {
                ToastView(toast: toast)
                    .padding(.top, 8)
                    .padding(.trailing, 16)
                    .transition(.move(edge: .trailing).combined(with: .opacity))
            }
        }
        .animation(.snappy, value: model.toast)
        .onAppear { model.undoManager = undoManager }
        .onChange(of: model.composeAsked) {
            if let request = model.composeAsked {
                openWindow(id: "compose", value: request)
                model.composeAsked = nil
            }
        }
        .onChange(of: model.windowAsked) {
            if let key = model.windowAsked {
                openWindow(id: "conversation", value: key)
                model.windowAsked = nil
            }
        }
        .onChange(of: undoManager) { model.undoManager = undoManager }
        .alert("Iris could not read the mail", isPresented: .constant(model.problem != nil)) {
            Button("OK") {}
        } message: {
            Text(model.problem ?? "")
        }
    }
}

/// Grows the key window on the right by `width` while the assistant is
/// open, and gives the room back when it closes. Near the screen's edge
/// the window moves left as much as it has to.
@MainActor
enum WindowRoom {
    private static var given: CGFloat = 0

    static func make(_ open: Bool, width: CGFloat) {
        guard let window = NSApp.keyWindow ?? NSApp.mainWindow, !window.styleMask.contains(.fullScreen) else { return }
        var frame = window.frame
        if open {
            guard given == 0, let screen = window.screen?.visibleFrame else { return }
            let room = min(width, screen.width - frame.width)
            guard room > 0 else { return }
            frame.size.width += room
            if frame.maxX > screen.maxX { frame.origin.x = screen.maxX - frame.width }
            given = room
        } else {
            guard given > 0 else { return }
            frame.size.width -= given
            given = 0
        }
        window.setFrame(frame, display: true, animate: true)
    }
}
