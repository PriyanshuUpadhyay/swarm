import SwiftUI
import SwarmCore

private struct TranscriptSearchQueryKey: EnvironmentKey {
    static let defaultValue = ""
}

extension EnvironmentValues {
    var transcriptSearchQuery: String {
        get { self[TranscriptSearchQueryKey.self] }
        set { self[TranscriptSearchQueryKey.self] = newValue }
    }
}

/// The large text path prepares pieces away from the main actor and creates views as they appear.
struct TranscriptBoundedTextView: View {
    let text: String
    var emptyText = ""
    @Environment(\.transcriptSearchQuery) private var searchQuery
    @State private var chunks: TranscriptTextChunks?

    var body: some View {
        Group {
            if text.isEmpty || text.index(text.startIndex, offsetBy: 4_096, limitedBy: text.endIndex) == nil {
                ScrollView(.horizontal, showsIndicators: true) {
                    Text(verbatim: text.isEmpty ? emptyText : text)
                        .fixedSize(horizontal: true, vertical: true)
                        .padding(DesignTokens.Spacing.s)
                }
            } else if let chunks {
                ScrollViewReader { proxy in
                    ScrollView([.horizontal, .vertical], showsIndicators: true) {
                        LazyVStack(alignment: .leading, spacing: 0) {
                            ForEach(chunks.pieces.indices, id: \.self) { index in
                                Text(verbatim: chunks.pieces[index])
                                    .fixedSize(horizontal: true, vertical: true)
                                    .id(index)
                            }
                        }
                        .padding(DesignTokens.Spacing.s)
                    }
                    .frame(height: DesignTokens.Size.outputPreview)
                    .task(id: searchQuery) {
                        guard !searchQuery.isEmpty else { return }
                        let query = searchQuery
                        let index = await Task.detached {
                            chunks.pieces.firstIndex { $0.localizedCaseInsensitiveContains(query) }
                        }.value
                        guard !Task.isCancelled, let index else { return }
                        proxy.scrollTo(index, anchor: .center)
                    }
                }
            } else {
                ProgressView("Preparing text…")
                    .frame(height: DesignTokens.Size.collapsedOutput, alignment: .leading)
            }
        }
        .font(DesignTokens.mono)
        .textSelection(.enabled)
        .task(id: text) {
            chunks = nil
            guard text.index(text.startIndex, offsetBy: 4_096, limitedBy: text.endIndex) != nil else { return }
            let source = text
            let prepared = await Task.detached(priority: .userInitiated) {
                TranscriptTextChunks(source)
            }.value
            guard !Task.isCancelled else { return }
            chunks = prepared
        }
    }
}
