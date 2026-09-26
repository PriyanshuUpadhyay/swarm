import Foundation

/// A saved edit's patch, separate from the tool result's status and text.
public struct TranscriptDiff: Sendable, Hashable, Decodable {
    public struct Hunk: Sendable, Hashable, Decodable {
        public let oldStart: Int
        public let oldLines: Int
        public let newStart: Int
        public let newLines: Int
        public let lines: [String]

        enum CodingKeys: String, CodingKey {
            case oldStart = "old_start", oldLines = "old_lines"
            case newStart = "new_start", newLines = "new_lines"
            case lines
        }
    }

    public let toolCallID: String
    public let path: String
    public let hunks: [Hunk]

    enum CodingKeys: String, CodingKey {
        case toolCallID = "tool_call_id"
        case path, hunks
    }
}
