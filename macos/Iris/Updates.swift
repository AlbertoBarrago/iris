import Sparkle
import SwiftUI

/// Sparkle, which looks for a new Iris on the appcast in Info.plist and
/// installs it. A build without the public signing key, such as one made
/// in Xcode, has nothing to check against, so it leaves Sparkle off; only
/// scripts/macos-dmg.sh adds the key.
@MainActor
final class Updates {
    static let shared = Updates()

    private let controller: SPUStandardUpdaterController?

    private init() {
        let signed = Bundle.main.object(forInfoDictionaryKey: "SUPublicEDKey") != nil
        controller = signed
            ? SPUStandardUpdaterController(startingUpdater: true, updaterDelegate: nil, userDriverDelegate: nil)
            : nil
    }

    /// Whether this copy updates itself at all.
    var available: Bool { controller != nil }

    /// Asks now, in Sparkle's own window.
    func checkNow() {
        controller?.checkForUpdates(nil)
    }

    /// Whether Sparkle looks once a day by itself.
    var automatic: Bool {
        get { controller?.updater.automaticallyChecksForUpdates ?? false }
        set { controller?.updater.automaticallyChecksForUpdates = newValue }
    }
}
