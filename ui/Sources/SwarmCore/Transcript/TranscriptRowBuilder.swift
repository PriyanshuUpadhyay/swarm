import Foundation
import TranscriptTool

/// The text rows a chat can draw from typed transcript events.
public struct TranscriptRow: Sendable, Hashable, Identifiable {
    public enum Kind: String, Sendable, Hashable {
        case user, assistant, thought, toolUse, toolResult, permission, error, notice, system, result
    }

    public var kind: Kind
    public var text: String
    public var eventID: String
    public var id: String { eventID }

    public var printLine: String {
        let first = text.components(separatedBy: .newlines).first ?? ""
        return "\(kind.rawValue) \(String(first.prefix(80)))"
    }
}

public enum TranscriptRowBuilder {
    public static func rows(from events: some Sequence<TranscriptEvent>) -> [TranscriptRow] {
        var rows: [TranscriptRow] = []
        for (index, event) in events.enumerated() {
            var row: TranscriptRow
            switch event {
            case .userMessageChunk(let text, let meta):
                row = TranscriptRow(kind: .user, text: text, eventID: meta.uuid)
            case .agentMessageChunk(let text, let meta):
                row = TranscriptRow(kind: .assistant, text: text, eventID: meta.uuid)
            case .agentThoughtChunk(let text, let meta):
                row = TranscriptRow(kind: .thought, text: text, eventID: meta.uuid)
            case .toolCall(let id, let name, let input, _, _):
                row = TranscriptRow(kind: .toolUse, text: "\(name) \(input.compactJSON)", eventID: id + ":call")
            case .toolCallUpdate(let id, _, let content, _):
                row = TranscriptRow(kind: .toolResult, text: content, eventID: id + ":result")
            case .elicitation(let id, let questions, _):
                row = TranscriptRow(kind: .permission, text: questions.map(\.question).joined(separator: "\n"), eventID: id + ":ask")
            case .elicitationResult(let id, let answers, _):
                row = TranscriptRow(kind: .result, text: answers.map(\.answer).joined(separator: "\n"), eventID: id + ":answer")
            case .permissionDecision(_, let id, let decision, _):
                row = TranscriptRow(kind: .result, text: decision, eventID: id + ":decision")
            case .error(let message, let meta):
                row = TranscriptRow(kind: .error, text: message, eventID: key(meta, "error", index))
            case .systemMessage(_, let text, let meta):
                row = TranscriptRow(kind: .system, text: text, eventID: key(meta, "system", index))
            case .sessionInfo(let kind, let value, let meta):
                row = TranscriptRow(kind: .notice, text: "\(kind): \(value)", eventID: key(meta, "info", index))
            case .hookResult(let kind, _, let name, _, _, let meta):
                row = TranscriptRow(kind: .notice, text: "\(kind): \(name)", eventID: key(meta, "hook", index))
            case .turnEnded(_, let reason, let meta):
                row = TranscriptRow(kind: .result, text: reason.rawValue, eventID: key(meta, "result", index))
            case .image(let role, let mediaType, let meta):
                row = TranscriptRow(kind: .notice, text: "\(role) image (\(mediaType))", eventID: key(meta, "image", index))
            case .unknown(let raw, let meta):
                row = TranscriptRow(kind: .notice, text: raw, eventID: meta.map { key($0, "unknown", index) } ?? "event-\(index)")
            default:
                continue
            }
            if row.eventID.isEmpty || row.eventID.first == ":" { row.eventID = "event-\(index)" }
            if let last = rows.last,
               last.kind == row.kind, last.eventID == row.eventID {
                if row.kind == .toolResult {
                    rows[rows.count - 1] = row
                } else if row.kind == .user || row.kind == .assistant || row.kind == .thought {
                    rows[rows.count - 1].text += row.text
                }
            } else {
                rows.append(row)
            }
        }
        return rows
    }

    private static func key(_ meta: Meta, _ kind: String, _ index: Int) -> String {
        meta.uuid.isEmpty ? "event-\(index)" : "\(meta.uuid):\(kind)"
    }
}
