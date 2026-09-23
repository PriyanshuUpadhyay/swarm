import Foundation
import TranscriptTool

public struct RawTranscriptEntry: Sendable, Hashable, Identifiable {
    public var index: Int
    public var rawLine: String
    public var displayText: String
    public var rowKind: String
    public var id: String { "raw-\(index)" }
}

public enum TranscriptDebugData {
    public static func entries(from records: some Sequence<TranscriptRecord>) -> [RawTranscriptEntry] {
        records.enumerated().map { index, record in
            let row = TranscriptRowBuilder.row(from: record.event, index: index)
            let kind = if let row, row.isHiddenByDefault { "hidden" }
                else if let row { row.kind.rawValue }
                else { "no row" }
            return RawTranscriptEntry(
                index: index, rawLine: record.rawLine,
                displayText: prettyJSON(record.rawLine), rowKind: kind
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

    private static func prettyJSON(_ line: String) -> String {
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
