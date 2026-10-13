import AppKit
import SwiftUI

/// Iris's About window: the winged envelope, who made it, what it is
/// built on, a short poem for the messenger it is named after, a star
/// to leave on GitHub, and the credit and licence the GPL asks for.
struct AboutView: View {
    @Environment(\.openURL) private var openURL
    private let words = aboutWords()

    var body: some View {
        VStack(spacing: 6) {
            Image(nsImage: NSApp.applicationIconImage)
                .resizable()
                .frame(width: 112, height: 112)
                .shadow(color: .black.opacity(0.2), radius: 6, y: 3)
                .accessibilityHidden(true)
                .padding(.bottom, 6)
            Text(words.name).font(.largeTitle.weight(.bold))
            Link(words.author, destination: URL(string: words.authorUrl)!)
            Text(versionWords(version: version))
                .foregroundStyle(.secondary)
                .padding(.top, 4)
            Text(words.comments)
                .multilineTextAlignment(.center)
                .padding(.top, 10)

            VStack(spacing: 3) {
                ForEach(words.poem, id: \.self) { line in
                    Text(line).italic()
                }
            }
            .font(.callout)
            .foregroundStyle(.secondary)
            .multilineTextAlignment(.center)
            .padding(.vertical, 14)
            .padding(.horizontal, 18)
            .frame(maxWidth: .infinity)
            .background(.quaternary.opacity(0.5), in: RoundedRectangle(cornerRadius: 12))
            .padding(.top, 14)

            VStack(spacing: 8) {
                Text(words.starLine)
                    .multilineTextAlignment(.center)
                    .font(.callout)
                Button {
                    if let url = URL(string: words.starUrl) { openURL(url) }
                } label: {
                    Label(words.starButton, systemImage: "star.fill")
                }
                .buttonStyle(.borderedProminent)
                .buttonBorderShape(.capsule)
                .tint(.yellow)
                .foregroundStyle(.black)
            }
            .padding(.top, 16)

            HStack(spacing: 6) {
                Link(basedOn, destination: URL(string: words.upstreamUrl)!)
                Text(verbatim: "·").accessibilityHidden(true)
                Link(words.licenseName, destination: URL(string: words.licenseUrl)!)
            }
            .font(.caption)
            .foregroundStyle(.secondary)
            .padding(.top, 18)
        }
        .padding(24)
        .frame(width: 400)
    }

    /// The credit Penguin Mail is owed, set beside the licence: Iris is a
    /// modified Penguin Mail, and the credit sits with the terms that ask
    /// for it rather than under the name.
    private var basedOn: String {
        words.basedOn.replacingOccurrences(of: "{app}", with: words.upstream)
    }

    private var version: String {
        Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? ""
    }
}
