import SwiftUI
import Translation

/// Translates the open message with the Mac's own Translation framework,
/// on this computer: the pieces the core cut the message's prose into go
/// to a `TranslationSession` and come back in order. The first time a
/// pair of languages is asked for, macOS offers to download it.
struct AppleTranslator: ViewModifier {
    @Environment(MailModel.self) private var model
    @State private var configuration: TranslationSession.Configuration?

    func body(content: Content) -> some View {
        content
            .translationTask(configuration) { session in
                guard let job = model.appleJob else { return }
                // The session belongs to this task alone; it is handed to the
                // helper that runs it and is not used here again.
                nonisolated(unsafe) let session = session
                do {
                    let pieces = try await Self.translate(job.job.pieces, with: session)
                    model.appleTranslated(job, pieces: pieces)
                } catch {
                    model.appleFailed(job, error)
                }
            }
            .onChange(of: model.appleJob) { _, job in
                guard let job else { return }
                let source = job.job.from.isEmpty ? nil : Locale.Language(identifier: job.job.from)
                let target = Locale.Language(identifier: job.job.into)
                if configuration?.source == source, configuration?.target == target {
                    // The same pair again: the task runs once more on a
                    // configuration it has been told is stale.
                    configuration?.invalidate()
                } else {
                    configuration = TranslationSession.Configuration(source: source, target: target)
                }
            }
    }

    /// `pieces` translated in one batch, in order, `nil` for one the
    /// translator gave nothing back for.
    nonisolated private static func translate(_ pieces: [String], with session: TranslationSession) async throws -> [String?] {
        let requests = pieces.enumerated().map { index, piece in
            TranslationSession.Request(sourceText: piece, clientIdentifier: String(index))
        }
        let responses = try await session.translations(from: requests)
        var out = [String?](repeating: nil, count: pieces.count)
        for response in responses {
            if let index = response.clientIdentifier.flatMap(Int.init), out.indices.contains(index) {
                out[index] = response.targetText
            }
        }
        return out
    }
}
