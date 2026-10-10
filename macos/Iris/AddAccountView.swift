import AppKit
import SwiftUI

/// Asks for the Add Account sheet, opened on a provider when one was
/// picked already, as on the welcome page.
struct AddAccountRequest: Identifiable, Equatable {
    let id = UUID()
    var tile: String? = nil
    /// The account to sign in again, rather than a new one.
    var again: Int64? = nil
}

/// Add Account, step by step as the GTK dialog goes: pick a provider,
/// type the address, let Iris find the servers, sign in with a password
/// or in the browser, or type the servers by hand. The core decides
/// every step and its words (`AccountSetup`); this draws them.
struct AddAccountView: View {
    @Environment(MailModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let request: AddAccountRequest

    @State private var setup: AccountSetup?
    @State private var screen: Screen = .pick
    @State private var address = ""
    @State private var said: String?
    @State private var busy = false
    /// The tile picked and the browser signed in with, for the band's
    /// stamp.
    @State private var tile: String?
    @State private var browserUsed: String?

    enum Screen: Equatable {
        case pick
        case address(tile: String?)
        case lookup([LookupCheck])
        case password(PasswordPage, FailureCard?)
        case servers(ServerForm)
        case browser(String)
        case closed(String)
        case added(String, Int64)
    }

    var body: some View {
        VStack(spacing: 0) {
            if let band {
                PostBand(band: band, lowerAsking: lowerAsking, letters: screenName == "added")
                    .transition(.move(edge: .top).combined(with: .opacity))
            }
            Group {
                switch screen {
                case .pick: pick
                case .address(let tile): addressPage(tile)
                case .lookup(let checks): lookup(checks)
                case .password(let page, let failure):
                    PasswordStep(page: page, failure: failure, busy: busy, asksName: request.again == nil,
                                 signIn: signIn, servers: openServers)
                case .servers(let form):
                    ServersStep(form: form, setup: setup, said: said, busy: busy) { use($0) }
                case .browser(let browser):
                    BrowserStep(browser: browser, address: address, setup: setup, done: added, failed: {
                        said = $0
                        browserUsed = nil
                        screen = .address(tile: nil)
                    })
                case .closed(let words): closed(words)
                case .added(let email, let id): addedPage(email, id)
                }
            }
            .id(screenName)
            .transition(.asymmetric(insertion: .move(edge: .trailing).combined(with: .opacity), removal: .opacity))
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
            .padding(24)
            Divider()
            footer.padding(16)
        }
        .clipped()
        .animation(.smooth(duration: 0.3), value: screenName)
        .frame(width: 520, height: 720)
        .onAppear(perform: start)
    }

    // MARK: Band

    /// The screen without what it carries, so a lookup row that changes
    /// does not count as a new page.
    private var screenName: String {
        switch screen {
        case .pick: "pick"
        case .address: "address"
        case .lookup: "lookup"
        case .password: "password"
        case .servers: "servers"
        case .browser: "browser"
        case .closed: "closed"
        case .added: "added"
        }
    }

    /// The band over the page, or none over Server Settings, as in the
    /// GTK dialog.
    private var band: BandView? {
        let step: BandStep
        switch screen {
        case .pick: step = .pick
        case .address: step = .address
        case .lookup: step = .lookup
        case .password(_, let failure):
            step = failure.map { $0.unreachable ? .unreachable : .refused } ?? .found
        case .servers: return nil
        case .browser: step = .browser
        case .closed: step = .closed
        case .added: step = .added
        }
        return setup?.band(step: step, typed: address, tile: tile, browser: browserUsed)
    }

    /// The lower lookup line asks once MX has answered.
    private var lowerAsking: Bool {
        guard case .lookup(let checks) = screen, let mx = checks.first(where: \.mx) else { return false }
        return mx.status == .answered || mx.status == .nothing
    }

    // MARK: Pages

    private var pick: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text(tr("Add an account")).font(.title2.weight(.semibold))
            Text(tr("Choose where your mail lives.")).foregroundStyle(.secondary)
            ProviderTiles(setup: setup) { choose($0) }
                .padding(.top, 4)
            Button(tr("Enter server settings by hand")) {
                screen = .address(tile: "by-hand")
            }
            .buttonStyle(.link)
        }
    }

    private func addressPage(_ tile: String?) -> some View {
        VStack(alignment: .leading, spacing: 14) {
            Text(tr("Your Email Address")).font(.title2.weight(.semibold))
            Text(tr("Iris finds the servers for you.")).foregroundStyle(.secondary)
            TextField(tr("Email Address"), text: $address, prompt: Text(tr("Your email address")))
                .textFieldStyle(.roundedBorder)
                .textContentType(.emailAddress)
                .onSubmit { look(tile) }
                .onChange(of: address) { said = nil }
            if let said {
                Text(said).foregroundStyle(.red).fixedSize(horizontal: false, vertical: true)
            }
            Text(tr("Only the domain leaves this computer, never your full address."))
                .font(.caption).foregroundStyle(.secondary)
        }
    }

    private func lookup(_ checks: [LookupCheck]) -> some View {
        VStack(alignment: .leading, spacing: 14) {
            Text(tr("Looking up {domain}").replacingOccurrences(of: "{domain}", with: domain))
                .font(.title2.weight(.semibold))
            Text(tr("Only the domain leaves this computer, never your full address."))
                .foregroundStyle(.secondary)
            VStack(spacing: 0) {
                ForEach(Array(checks.enumerated()), id: \.element.title) { index, check in
                    if index > 0 { Divider().padding(.leading, 44) }
                    CheckRow(check: check)
                }
            }
            .background(.quaternary.opacity(0.35), in: RoundedRectangle(cornerRadius: 10))
            .overlay(RoundedRectangle(cornerRadius: 10).strokeBorder(.separator))
            .animation(.smooth(duration: 0.25), value: checks.map(\.status))
        }
    }

    private var domain: String {
        address.split(separator: "@").last.map(String.init) ?? address
    }

    private func closed(_ words: String) -> some View {
        VStack(alignment: .leading, spacing: 14) {
            Text(tr("No Other Mail Apps")).font(.title2.weight(.semibold))
            Text(words).fixedSize(horizontal: false, vertical: true)
            Button(tr("Use another address")) { screen = .address(tile: nil) }
            Button(tr("Choose another provider")) { screen = .pick }
        }
    }

    private func addedPage(_ email: String, _ id: Int64) -> some View {
        VStack(spacing: 14) {
            Spacer()
            Image(systemName: "checkmark.circle.fill").font(.system(size: 44)).foregroundStyle(.green)
            Text(tr("{account} is added").replacingOccurrences(of: "{account}", with: email))
                .font(.title2.weight(.semibold))
                .multilineTextAlignment(.center)
            Text(tr("Mail is downloading. You can close this window."))
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
            HStack {
                Button(tr("Add Another Account")) { reset() }
                Button(tr("Open Inbox")) {
                    model.openInbox(of: id)
                    dismiss()
                }
                .buttonStyle(.borderedProminent)
            }
            Spacer()
        }
        .frame(maxWidth: .infinity)
    }

    @ViewBuilder private var footer: some View {
        HStack {
            // The GTK dialog's back arrow, named for where it goes.
            if canGoBack {
                Button(action: back) {
                    Label(backTitle, systemImage: "chevron.left")
                }
            }
            Spacer()
            switch screen {
            case .added:
                Button(tr("Not Now")) { dismiss() }
            default:
                Button(tr("Cancel")) {
                    setup?.cancelBrowser()
                    dismiss()
                }
                .keyboardShortcut(.cancelAction)
                if case .address(let tile) = screen {
                    Button(tr("Continue")) { look(tile) }
                        .keyboardShortcut(.defaultAction)
                        .disabled(address.trimmingCharacters(in: .whitespaces).isEmpty)
                }
            }
        }
    }

    private var canGoBack: Bool {
        // Signing in again starts on its own page, with nowhere to go back.
        if request.again != nil { return false }
        return switch screen {
        case .address, .password, .servers, .closed: true
        default: false
        }
    }

    // MARK: Steps

    private func start() {
        setup = model.accountSetup()
        if let again = request.again, let setup {
            address = model.accountLook(again)?.email ?? ""
            landed(Result { try setup.signInAgain(accountId: again) })
        } else if let tile = request.tile {
            choose(tile)
        }
    }

    private func reset() {
        setup = model.accountSetup()
        address = ""
        said = nil
        tile = nil
        browserUsed = nil
        screen = .pick
    }

    private var backTitle: String {
        if case .address = screen { return tr("Choose another provider") }
        return tr("Use another address")
    }

    private func back() {
        switch screen {
        case .address:
            tile = nil
            browserUsed = nil
            screen = .pick
        default: screen = .address(tile: tile)
        }
    }

    private func choose(_ tile: String) {
        guard let setup else { return }
        self.tile = tile == "by-hand" ? nil : tile
        if let browser = setup.tiles().first(where: { $0.key == tile })?.browser {
            browserUsed = browser
            screen = .browser(browser)
        } else {
            screen = .address(tile: tile)
        }
    }

    private func look(_ tile: String?) {
        guard let setup else { return }
        switch setup.onContinue(typed: address) {
        case .say(let words):
            said = words
        case .suggest(let better, let words):
            address = better
            said = words
        case .look(let found):
            address = found
            if tile == "by-hand" {
                openServers()
                return
            }
            screen = .lookup([])
            let heard = LookupHeard { checks in
                Task { @MainActor in
                    if case .lookup = screen { screen = .lookup(checks) }
                }
            }
            Task.detached {
                let step = Result { try setup.discover(address: found, listener: heard) }
                await MainActor.run { landed(step) }
            }
        }
    }

    private func landed(_ step: Result<SetupStep, Error>) {
        switch step {
        case .failure(let error):
            said = "\(error)"
            screen = .address(tile: nil)
        case .success(.password(let page)): screen = .password(page, nil)
        case .success(.say(let words)):
            said = words
            screen = .address(tile: nil)
        case .success(.closed(let words)): screen = .closed(words)
        case .success(.browser(let browser)):
            browserUsed = browser
            screen = .browser(browser)
        case .success(.servers(let form)):
            said = form.line
            screen = .servers(form)
        }
    }

    private func openServers() {
        guard let setup else { return }
        do {
            said = nil
            screen = .servers(try setup.serverForm(address: address))
        } catch {
            said = "\(error)"
        }
    }

    private func use(_ form: ServerForm) {
        guard let setup else { return }
        do {
            screen = .password(try setup.useServers(form: form), nil)
            said = nil
        } catch {
            said = "\(error)"
        }
    }

    private func signIn(_ password: String, _ name: String) {
        guard let setup else { return }
        busy = true
        Task.detached {
            let answer = Result { try setup.signIn(password: password, name: name) }
            await MainActor.run {
                busy = false
                switch answer {
                case .success(.added(let id, let email)): added(email, id)
                case .success(.failed(let card, let page)): screen = .password(page, card)
                case .failure(let error):
                    if case .password(let page, _) = screen {
                        screen = .password(page, FailureCard(line: "\(error)", title: tr("Could not sign in"), body: "\(error)",
                                                             said: nil, links: [], copy: nil, unreachable: false))
                    }
                }
            }
        }
    }

    private func added(_ email: String, _ id: Int64) {
        if request.again != nil {
            model.signedInAgain(email)
            dismiss()
            return
        }
        model.accountAdded()
        screen = .added(email, id)
    }
}

/// The provider tiles, two to a row.
private struct ProviderTiles: View {
    let setup: AccountSetup?
    let choose: (String) -> Void

    var body: some View {
        let tiles = setup?.tiles() ?? []
        LazyVGrid(columns: [GridItem(.flexible()), GridItem(.flexible())], spacing: 10) {
            ForEach(tiles, id: \.key) { tile in
                Button { choose(tile.key) } label: {
                    HStack(spacing: 12) {
                        ProviderMark(stamp: tile.stamp)
                        VStack(alignment: .leading, spacing: 2) {
                            HStack(spacing: 4) {
                                Text(tile.title).font(.headline)
                                if tile.browser != nil {
                                    Image(systemName: "globe").font(.caption).foregroundStyle(.tertiary)
                                }
                            }
                            Text(tile.subtitle).font(.caption).foregroundStyle(.secondary)
                        }
                        Spacer(minLength: 0)
                    }
                }
                .buttonStyle(TileButtonStyle())
                .accessibilityLabel(tile.described)
            }
        }
    }
}

/// A tile's card: lifts a little under the pointer and sinks when
/// pressed.
private struct TileButtonStyle: ButtonStyle {
    @State private var hovering = false

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .frame(maxWidth: .infinity, minHeight: 44, alignment: .leading)
            .padding(12)
            .background(.background, in: RoundedRectangle(cornerRadius: 12, style: .continuous))
            .overlay(RoundedRectangle(cornerRadius: 12, style: .continuous).strokeBorder(.separator))
            .shadow(color: .black.opacity(hovering ? 0.12 : 0), radius: 6, y: 2)
            .scaleEffect(configuration.isPressed ? 0.97 : hovering ? 1.02 : 1)
            .animation(.snappy(duration: 0.18), value: hovering)
            .animation(.snappy(duration: 0.12), value: configuration.isPressed)
            .contentShape(Rectangle())
            .onHover { hovering = $0 }
    }
}

/// One lookup question: a ring while it waits, a spinner while it asks,
/// a tick once something came back.
private struct CheckRow: View {
    let check: LookupCheck

    var body: some View {
        HStack(spacing: 12) {
            ZStack {
                switch check.status {
                case .asking:
                    ProgressView().controlSize(.small).scaleEffect(0.8)
                        .transition(.opacity)
                case .answered:
                    Image(systemName: "checkmark.circle.fill")
                        .font(.system(size: 16))
                        .foregroundStyle(.green)
                        .transition(.scale(scale: 0.4).combined(with: .opacity))
                case .notListed:
                    Circle().strokeBorder(.tertiary, style: StrokeStyle(lineWidth: 1.6, dash: [2.5, 2]))
                        .frame(width: 16, height: 16)
                case .waiting, .nothing:
                    Circle().strokeBorder(.quaternary, lineWidth: 1.6)
                        .frame(width: 16, height: 16)
                        .transition(.opacity)
                }
            }
            .frame(width: 20, height: 20)
            Text(check.title).fontWeight(.medium)
            Spacer()
            Text(check.state)
                .font(.callout.weight(.medium))
                .foregroundStyle(check.status == .asking ? AnyShapeStyle(.tint) : AnyShapeStyle(.secondary))
                .contentTransition(.opacity)
        }
        .padding(.horizontal, 14)
        .frame(minHeight: 40)
        .accessibilityElement(children: .combine)
    }
}

/// The password page: where the servers were found, the password, the
/// person's name, and what went wrong last time.
private struct PasswordStep: View {
    let page: PasswordPage
    let failure: FailureCard?
    let busy: Bool
    /// A new account asks for the person's name; one signing in again
    /// keeps the one it has.
    let asksName: Bool
    let signIn: (String, String) -> Void
    let servers: () -> Void
    @State private var password = ""
    @State private var name = ""
    @State private var confirmed = false

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(tr("Sign in to {domain}").replacingOccurrences(of: "{domain}", with: domain))
                .font(.title2.weight(.semibold))
            Text(page.address).foregroundStyle(.secondary)
            Text(page.found).font(.callout)
            HStack {
                Text(page.servers).font(.caption).foregroundStyle(.secondary)
                Spacer()
                Button(tr("Server Settings"), action: servers).buttonStyle(.link).font(.caption)
            }
            if page.confirm {
                Toggle(tr("Use these servers"), isOn: $confirmed)
            }
            SecureField(page.field, text: $password)
                .textFieldStyle(.roundedBorder)
                .onSubmit(go)
            if let hint = page.hint {
                VStack(alignment: .leading, spacing: 2) {
                    Text(hint).font(.caption)
                    if let link = page.hintLink, let url = URL(string: link.url) {
                        Link(link.label, destination: url).font(.caption)
                    }
                }
            }
            if asksName {
                TextField(tr("Your Name"), text: $name).textFieldStyle(.roundedBorder)
            }
            if let failure {
                FailureView(card: failure)
            }
            HStack {
                Spacer()
                if busy {
                    ProgressView().controlSize(.small)
                    Text(tr("Signing In…")).foregroundStyle(.secondary)
                }
                Button(tr("Sign In"), action: go)
                    .buttonStyle(.borderedProminent)
                    .keyboardShortcut(.defaultAction)
                    .disabled(busy || password.isEmpty || (page.confirm && !confirmed))
            }
        }
    }

    private var domain: String {
        page.address.split(separator: "@").last.map(String.init) ?? page.address
    }

    private func go() {
        guard !busy, !password.isEmpty, !page.confirm || confirmed else { return }
        signIn(password, name)
    }
}

/// What a failed sign-in says, with the pages and the command that help.
private struct FailureView: View {
    let card: FailureCard

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Label(card.title, systemImage: "exclamationmark.triangle.fill")
                .font(.headline)
                .foregroundStyle(.orange)
            Text(card.body).fixedSize(horizontal: false, vertical: true)
            if let said = card.said {
                Text(said).font(.caption.monospaced()).foregroundStyle(.secondary).textSelection(.enabled)
            }
            ForEach(card.links, id: \.url) { link in
                if let url = URL(string: link.url) { Link(link.label, destination: url).font(.caption) }
            }
            if let copy = card.copy {
                Button(tr("Copy Command")) {
                    NSPasteboard.general.clearContents()
                    NSPasteboard.general.setString(copy, forType: .string)
                }
                .controlSize(.small)
            }
        }
        .padding(10)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(.orange.opacity(0.08), in: RoundedRectangle(cornerRadius: 8))
        .accessibilityElement(children: .combine)
        .accessibilityLabel(card.line)
    }
}

/// Server Settings: the incoming server, IMAP or POP3, and the outgoing
/// one, each with its port, security and user name.
private struct ServersStep: View {
    @State var form: ServerForm
    let setup: AccountSetup?
    let said: String?
    let busy: Bool
    let use: (ServerForm) -> Void

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 12) {
                Text(tr("Server Settings")).font(.title2.weight(.semibold))
                if let said {
                    Text(said).font(.callout).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
                }
                Form {
                    Section(tr("Incoming Mail")) {
                        Picker(tr("Protocol"), selection: Binding(
                            get: { form.protocol },
                            set: { if let setup { form = setup.switchProtocol(form: form, protocol: $0) } }
                        )) {
                            Text("IMAP").tag("imap")
                            Text("POP3").tag("pop3")
                        }
                        fields($form.incoming, outgoing: false, user: tr("Incoming User Name"))
                    }
                    Section(tr("Outgoing Mail")) {
                        fields($form.outgoing, outgoing: true, user: tr("Outgoing User Name"))
                    }
                    Text(tr("Leave them empty to sign in with your address."))
                        .font(.caption).foregroundStyle(.secondary)
                }
                .formStyle(.grouped)
                HStack {
                    Spacer()
                    Button(tr("Use These Settings")) { use(form) }
                        .buttonStyle(.borderedProminent)
                        .keyboardShortcut(.defaultAction)
                        .disabled(busy)
                }
            }
        }
    }

    @ViewBuilder private func fields(_ server: Binding<ServerFields>, outgoing: Bool, user: String) -> some View {
        TextField(tr("Server"), text: server.host)
        TextField(tr("Port"), value: server.port, format: .number.grouping(.never))
        Picker(tr("Security"), selection: Binding(
            get: { server.wrappedValue.security },
            set: { security in
                let port = setup?.portAfterSecurity(outgoing: outgoing, protocol: form.protocol,
                                                    port: server.wrappedValue.port, security: security)
                server.wrappedValue.security = security
                if let port { server.wrappedValue.port = port }
            }
        )) {
            Text("TLS").tag("tls")
            Text("STARTTLS").tag("starttls")
        }
        TextField(user, text: server.user)
    }
}

/// Google's or Microsoft's sign-in, in the browser: the page opens at
/// once, and this waits up to five minutes, counting down.
private struct BrowserStep: View {
    let browser: String
    let address: String
    let setup: AccountSetup?
    let done: (String, Int64) -> Void
    let failed: (String) -> Void
    @State private var url: String?
    @State private var left: UInt32 = 300
    @State private var timer: Timer?

    var body: some View {
        VStack(spacing: 14) {
            Spacer()
            Image(systemName: "safari").font(.system(size: 40)).foregroundStyle(.secondary)
            Text(tr("Sign in with {provider} in your browser").replacingOccurrences(of: "{provider}", with: provider))
                .font(.title2.weight(.semibold))
                .multilineTextAlignment(.center)
            Text(tr("Waiting for your browser")).foregroundStyle(.secondary)
            if let setup {
                Text(setup.timeLeft(seconds: left)).monospacedDigit().foregroundStyle(.secondary)
            }
            Text(tr("Iris stops waiting after five minutes.")).font(.caption).foregroundStyle(.secondary)
            HStack {
                Button(tr("Open Page Again")) { if let url { open(url) } }.disabled(url == nil)
                Button(tr("Copy Link")) {
                    guard let url else { return }
                    NSPasteboard.general.clearContents()
                    NSPasteboard.general.setString(url, forType: .string)
                }
                .disabled(url == nil)
            }
            Spacer()
        }
        .frame(maxWidth: .infinity)
        .onAppear(perform: start)
        .onDisappear {
            timer?.invalidate()
            setup?.cancelBrowser()
        }
    }

    private var provider: String { browser == "microsoft" ? "Microsoft" : "Google" }

    private func start() {
        guard let setup else { return }
        left = setup.browserWaitSeconds()
        timer = Timer.scheduledTimer(withTimeInterval: 1, repeats: true) { _ in
            Task { @MainActor in if left > 0 { left -= 1 } }
        }
        let opener = PageOpener { found in
            Task { @MainActor in
                url = found
                open(found)
            }
        }
        let expected = address.isEmpty ? nil : address
        let browser = browser
        Task.detached {
            let answer = Result { try setup.signInInBrowser(browser: browser, expected: expected, opener: opener) }
            await MainActor.run {
                timer?.invalidate()
                switch answer {
                case .success(.added(let id, let email)): done(email, id)
                case .success(.failed(let card, _)): failed(card.line)
                case .failure(let error): failed("\(error)")
                }
            }
        }
    }

    private func open(_ url: String) {
        if let page = URL(string: url) { NSWorkspace.shared.open(page) }
    }
}

/// Hands the lookup rows from the core's thread to the sheet.
private final class LookupHeard: LookupListener, @unchecked Sendable {
    let heard: @Sendable ([LookupCheck]) -> Void
    init(_ heard: @escaping @Sendable ([LookupCheck]) -> Void) { self.heard = heard }
    func changed(checks: [LookupCheck]) { heard(checks) }
}

/// Opens the provider's page as the core hands it over.
private final class PageOpener: BrowserOpener, @unchecked Sendable {
    let opened: @Sendable (String) -> Void
    init(_ opened: @escaping @Sendable (String) -> Void) { self.opened = opened }
    func openPage(url: String) { opened(url) }
}

/// What the window shows while the store has no account: the providers,
/// one click from Add Account.
struct WelcomeView: View {
    @Environment(MailModel.self) private var model

    var body: some View {
        let setup = model.accountSetup()
        VStack(spacing: 18) {
            if let band = setup?.band(step: .pick, typed: "", tile: nil, browser: nil) {
                PostBand(band: band, scale: 1.25)
                    .padding(.horizontal, -32)
                    .padding(.top, -32)
            }
            Spacer()
            Text(tr("Add your first account")).font(.largeTitle.weight(.semibold))
            Text(tr("Choose where your mail lives.")).foregroundStyle(.secondary)
            ProviderTiles(setup: setup) { tile in
                model.addAccount = AddAccountRequest(tile: tile)
            }
            .frame(maxWidth: 460)
            Button(tr("Enter server settings by hand")) {
                model.addAccount = AddAccountRequest(tile: "by-hand")
            }
            .buttonStyle(.link)
            Spacer()
        }
        .padding(32)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}
