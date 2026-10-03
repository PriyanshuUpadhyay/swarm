import Foundation

/// Limits the displayed copy of tool output; copying still uses the source text.
public struct TranscriptTextPreview: Sendable, Equatable {
    public let text: String
    public let isTruncated: Bool
    /// Lines of the source past `lineLimit`; 0 when only the character cap cut the preview.
    public let hiddenLineCount: Int

    public init(_ source: String, lineLimit: Int = 120) {
        let prefix = source.prefix(12_000)
        text = prefix.split(separator: "\n", omittingEmptySubsequences: false)
            .prefix(lineLimit).joined(separator: "\n")
        isTruncated = prefix.endIndex != source.endIndex || text != String(prefix)
        hiddenLineCount = max(0, source.count { $0 == "\n" } + 1 - lineLimit)
    }
}

/// Small pieces let the transcript create only the visible text views while scrolling.
public struct TranscriptTextChunks: Sendable, Equatable {
    public let pieces: [String]

    public init(_ source: String, limit: Int = 4_096) {
        precondition(limit > 0)
        var pieces: [String] = []
        var current = ""
        var count = 0
        let lines = source.split(separator: "\n", omittingEmptySubsequences: false)
        for (index, line) in lines.enumerated() {
            let value = String(line) + (index < lines.count - 1 ? "\n" : "")
            let length = value.count
            if !current.isEmpty, count + length > limit {
                pieces.append(current)
                current = ""
                count = 0
            }
            var rest = value[...]
            while let end = rest.index(rest.startIndex, offsetBy: limit, limitedBy: rest.endIndex),
                  end < rest.endIndex {
                pieces.append(String(rest[..<end]))
                rest = rest[end...]
            }
            current.append(contentsOf: rest)
            count += rest.count
        }
        if !current.isEmpty { pieces.append(current) }
        self.pieces = pieces
    }
}
