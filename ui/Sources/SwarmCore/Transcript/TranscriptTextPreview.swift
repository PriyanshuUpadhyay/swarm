import Foundation

/// Limits the displayed copy of tool output; copying still uses the source text.
public struct TranscriptTextPreview: Sendable, Equatable {
    public let text: String
    public let isTruncated: Bool

    public init(_ source: String) {
        let prefix = source.prefix(12_000)
        text = prefix.split(separator: "\n", omittingEmptySubsequences: false)
            .prefix(120).joined(separator: "\n")
        isTruncated = text.count < source.count
    }
}
