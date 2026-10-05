import Foundation
import TranscriptTool

public struct RawTranscriptEntry: Sendable, Hashable, Identifiable {
    public var index: Int
    public var rawLine: String
    public var rowKind: String
    public var sessionID: String = ""
    public var id: String { Self.id(sessionID: sessionID, index: index) }
    /// A row's `sourceIDs` use the same form, so Show Source finds its entries.
    public static func id(sessionID: String = "", index: Int) -> String { "\(sessionID)raw-\(index)" }
    /// Pretty-printed on read, because a rebuild makes every entry and the raw view is usually closed.
    public var displayText: String { TranscriptDebugData.prettyJSON(rawLine) }
}

/// The raw entries behind one row, for Show Source.
public enum TranscriptSource {
    /// The entries whose id is one of the row's `sourceIDs`, in log order. O(entries).
    public static func entries(for row: TranscriptRow, in raw: [RawTranscriptEntry]) -> [RawTranscriptEntry] {
        guard !row.sourceIDs.isEmpty else { return [] }
        let wanted = Set(row.sourceIDs)
        return raw.filter { wanted.contains($0.id) }
    }
}

public enum TranscriptDebugData {
    public static func entries(from records: some Sequence<TranscriptRecord>, indexOffset: Int = 0) -> [RawTranscriptEntry] {
        records.enumerated().map { index, record in
            let row = TranscriptRowBuilder.row(from: record.event, index: index + indexOffset)
            let kind = if let row, row.isHiddenByDefault { "hidden" }
                else if let row { row.kind.rawValue }
                else { "no row" }
            return RawTranscriptEntry(
                index: index + indexOffset, rawLine: record.rawLine, rowKind: kind
            )
        }
    }

    public static func sessionJSON(session: SwarmSession, agents: [SwarmAgent]) -> String {
        prettyJSON(SessionData(session: session, agents: agents))
    }

    public static func printText(
        session: SwarmSession, agents: [SwarmAgent], entries: [RawTranscriptEntry]
    ) -> String {
        let records = entries.map { "[\($0.index)] \($0.rowKind)\n\($0.displayText)" }
        return ([sessionJSON(session: session, agents: agents)] + records).joined(separator: "\n\n")
    }

    static func prettyJSON(_ line: String) -> String {
        guard let data = line.data(using: .utf8),
              let object = try? JSONSerialization.jsonObject(with: data),
              let pretty = try? JSONSerialization.data(
                withJSONObject: object, options: [.prettyPrinted, .sortedKeys, .withoutEscapingSlashes]
              ) else { return line }
        return String(decoding: pretty, as: UTF8.self)
    }

    private static func prettyJSON<T: Encodable>(_ value: T) -> String {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys, .withoutEscapingSlashes]
        guard let data = try? encoder.encode(value) else { return "{}" }
        return String(decoding: data, as: UTF8.self)
    }

    private struct SessionData: Encodable {
        var session: SwarmSession
        var agents: [SwarmAgent]
    }
}
