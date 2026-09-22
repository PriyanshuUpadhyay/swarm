import TranscriptTool

/// The text rows a chat can draw from typed transcript events.
public struct TranscriptRow: Sendable, Hashable {
    public enum Kind: Sendable, Hashable {
        case user, assistant, thought
    }

    public var kind: Kind
    public var text: String
    public var eventID: String
}

public enum TranscriptRowBuilder {
    public static func rows(from events: some Sequence<TranscriptEvent>) -> [TranscriptRow] {
        var rows: [TranscriptRow] = []
        for event in events {
            let row: TranscriptRow
            switch event {
            case .userMessageChunk(let text, let meta):
                row = TranscriptRow(kind: .user, text: text, eventID: meta.uuid)
            case .agentMessageChunk(let text, let meta):
                row = TranscriptRow(kind: .assistant, text: text, eventID: meta.uuid)
            case .agentThoughtChunk(let text, let meta):
                row = TranscriptRow(kind: .thought, text: text, eventID: meta.uuid)
            default:
                continue
            }
            if !row.eventID.isEmpty, let last = rows.last,
               last.kind == row.kind, last.eventID == row.eventID {
                rows[rows.count - 1].text += row.text
            } else {
                rows.append(row)
            }
        }
        return rows
    }
}
