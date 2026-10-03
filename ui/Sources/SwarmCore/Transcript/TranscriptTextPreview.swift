import Foundation

/// Limits the displayed copy of tool output; copying still uses the source text.
public struct TranscriptTextPreview: Sendable, Equatable {
    public let text: String
    public let isTruncated: Bool
    /// Lines in the whole source; an empty source is one empty line, and a final "\n" ends the last
    /// line instead of starting an empty one.
    public let lineCount: Int
    /// Lines of the source that `text` leaves out; a line the character cap cuts counts as shown.
    public let hiddenLineCount: Int

    public init(_ source: String, lineLimit: Int = 120) {
        let prefix = source.prefix(12_000)
        // Lines end at byte 10, not at the Character "\n", because "\r\n" is one Character, and a
        // byte scan keeps the count cheap on output of many megabytes.
        var lines = prefix.utf8.split(separator: 10, omittingEmptySubsequences: false)
        if lines.count > 1, lines.last?.isEmpty == true { lines.removeLast() }
        let kept = min(lineLimit, lines.count)
        text = kept < lines.count
            ? lines.prefix(kept).map { String(decoding: $0, as: UTF8.self) }.joined(separator: "\n")
            : String(prefix)
        isTruncated = prefix.endIndex != source.endIndex || kept < lines.count
        lineCount = source.utf8.count { $0 == 10 } + (source.utf8.last == 10 ? 0 : 1)
        hiddenLineCount = lineCount - kept
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
