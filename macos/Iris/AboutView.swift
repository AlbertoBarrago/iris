import AppKit
import SwiftUI

/// Iris's About window: the winged envelope, who made it, what it is
/// built on, a short poem for the messenger it is named after, and a
/// star to leave on GitHub, as the GTK About window has most of it.
struct AboutView: View {
    @Environment(\.openURL) private var openURL
    private let words = aboutWords()

    var body: some View {
        ScrollView {
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

                VStack(spacing: 0) {
                    ForEach(Array(links.enumerated()), id: \.offset) { index, link in
                        if index > 0 { Divider() }
                        Button {
                            if let url = URL(string: link.url) { openURL(url) }
                        } label: {
                            HStack {
                                VStack(alignment: .leading, spacing: 1) {
                                    Text(link.title)
                                    if !link.subtitle.isEmpty {
                                        Text(link.subtitle).font(.caption).foregroundStyle(.secondary)
                                    }
                                }
                                Spacer()
                                Image(systemName: "arrow.up.right.square").foregroundStyle(.secondary)
                            }
                            .padding(.horizontal, 12)
                            .padding(.vertical, 8)
                            .contentShape(Rectangle())
                        }
                        .buttonStyle(.plain)
                    }
                }
                .background(.quaternary.opacity(0.5), in: RoundedRectangle(cornerRadius: 10))
                .padding(.top, 20)
            }
            .padding(24)
        }
        .frame(width: 400, height: 640)
    }

    /// The links, with Penguin Mail's beside the licence at the end: Iris
    /// is a modified Penguin Mail, and the credit sits with the terms that
    /// ask for it rather than under the name.
    private var links: [AboutLink] {
        words.links + [AboutLink(
            title: words.basedOn.replacingOccurrences(of: "{app}", with: words.upstream),
            subtitle: "",
            url: words.upstreamUrl
        )]
    }

    private var version: String {
        Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? ""
    }
}
