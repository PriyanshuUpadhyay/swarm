import Foundation
import TranscriptTool

public enum TranscriptTail {
    public static func follows(current: Bool, atBottom: Bool, userScrolled: Bool) -> Bool {
        userScrolled ? atBottom : current
    }
}

/// The text rows a chat can draw from typed transcript events.
public struct TranscriptRow: Sendable, Hashable, Identifiable {
    public enum Kind: String, Sendable, Hashable {
        case user, assistant, thought, toolUse, toolResult, permission, error, notice, system, result
    }

    public var kind: Kind
    public var text: String
    public var eventID: String
    public var detail: String? = nil
    public var endsTurn = false
    public var id: String { eventID }

    public func label(chair: String?) -> String {
        switch kind {
        case .user: "You"
        case .assistant: chair?.capitalized ?? "Chair"
        case .thought: "Thinking"
        case .toolUse: "Tool"
        case .toolResult: "Result"
        case .permission: "Permission"
        case .error: "Error"
        case .notice: "Notice"
        case .system: "System"
        case .result: "Turn"
        }
    }

    public var isHiddenByDefault: Bool {
        (kind == .notice && (text.hasPrefix("hook_success") || text.hasPrefix("title:")))
            || (kind == .system && text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
    }

    public var printLine: String {
        let first = text.components(separatedBy: .newlines).first ?? ""
        return "\(kind.rawValue) \(String(first.prefix(80)))"
    }
}

public enum TranscriptRowBuilder {
    public static func rows(from records: some Sequence<TranscriptRecord>) -> [TranscriptRow] {
        rows(from: records.compactMap { record in
            if case .page = record.event { return nil }
            return record.event
        })
    }

    public static func rows(from events: some Sequence<TranscriptEvent>) -> [TranscriptRow] {
        var rows: [TranscriptRow] = []
        for (index, event) in events.enumerated() {
            guard let row = row(from: event, index: index) else { continue }
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

    public static func row(from event: TranscriptEvent, index: Int) -> TranscriptRow? {
        var row: TranscriptRow
        switch event {
        case .userMessageChunk(let text, let meta):
            row = TranscriptRow(kind: .user, text: text, eventID: meta.uuid)
        case .agentMessageChunk(let text, let meta):
            row = TranscriptRow(kind: .assistant, text: text, eventID: meta.uuid)
        case .agentThoughtChunk(let text, let meta):
            row = TranscriptRow(kind: .thought, text: text, eventID: meta.uuid)
        case .toolCall(let id, let name, let input, _, _):
            row = TranscriptRow(
                kind: .toolUse, text: toolSummary(name: name, input: input), eventID: id + ":call"
            )
            row.detail = input.compactJSON
        case .toolCallUpdate(let id, _, let content, _):
            row = TranscriptRow(kind: .toolResult, text: content, eventID: id + ":result")
        case .elicitation(let id, let questions, _):
            row = TranscriptRow(
                kind: .permission, text: questions.map(\.question).joined(separator: "\n"),
                eventID: id + ":ask"
            )
        case .elicitationResult(let id, let answers, _):
            row = TranscriptRow(
                kind: .result, text: answers.map(\.answer).joined(separator: "\n"),
                eventID: id + ":answer"
            )
        case .permissionDecision(_, let id, let decision, _):
            row = TranscriptRow(kind: .result, text: decision, eventID: id + ":decision")
        case .error(let message, let meta):
            row = TranscriptRow(kind: .error, text: message, eventID: key(meta, "error", index))
        case .systemMessage(_, let text, let meta):
            row = TranscriptRow(kind: .system, text: text, eventID: key(meta, "system", index))
        case .sessionInfo(let kind, let value, let meta):
            row = TranscriptRow(
                kind: .notice, text: "\(kind): \(value)", eventID: key(meta, "info", index)
            )
        case .hookResult(let kind, _, let name, _, _, let meta):
            row = TranscriptRow(
                kind: .notice, text: "\(kind): \(name)", eventID: key(meta, "hook", index)
            )
        case .turnEnded(_, let reason, let meta):
            row = TranscriptRow(
                kind: .result, text: reason.rawValue, eventID: key(meta, "result", index)
            )
            row.endsTurn = true
        case .image(let role, let mediaType, let meta):
            row = TranscriptRow(
                kind: .notice, text: "\(role) image (\(mediaType))",
                eventID: key(meta, "image", index)
            )
        case .unknown(let raw, let meta):
            row = TranscriptRow(
                kind: .notice, text: raw,
                eventID: meta.map { key($0, "unknown", index) } ?? "event-\(index)"
            )
        default:
            return nil
        }
        if row.eventID.isEmpty || row.eventID.first == ":" { row.eventID = "event-\(index)" }
        return row
    }

    private static func key(_ meta: Meta, _ kind: String, _ index: Int) -> String {
        meta.uuid.isEmpty ? "event-\(index)" : "\(meta.uuid):\(kind)"
    }

    private static func toolSummary(name: String, input: JSONElement) -> String {
        guard case .object(let fields) = input else { return name }
        let description: String? = if case .string(let text) = fields["description"] { text } else { nil }
        let command: String? = if case .string(let text) = fields["command"] { text } else { nil }
        let detail = [description, command?.components(separatedBy: .newlines).first]
            .compactMap { $0?.trimmingCharacters(in: .whitespacesAndNewlines) }
            .first { !$0.isEmpty }
        return detail.map { "\(name) · \($0)" } ?? name
    }
}

public enum ChairTurn {
    /// A turn runs from the user's last message until a turn-ended row follows it.
    public static func isActive(_ rows: [TranscriptRow]) -> Bool {
        guard let lastUser = rows.lastIndex(where: { $0.kind == .user }) else { return false }
        return !rows[lastUser...].contains(where: \.endsTurn)
    }
}
