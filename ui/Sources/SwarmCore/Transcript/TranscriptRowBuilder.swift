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
        case user, assistant, thought, toolUse, toolResult, diff, permission, error, notice, system, result
    }

    public var kind: Kind
    public var text: String
    public var eventID: String
    public var detail: String? = nil
    public var diff: TranscriptDiff? = nil
    public var tool: TranscriptToolActivity? = nil
    public var toolStatus: ToolStatus? = nil
    public var endsTurn = false
    public var id: String { eventID }

    public init(kind: Kind, text: String, eventID: String) {
        self.kind = kind
        self.text = text
        self.eventID = eventID
    }

    public func label(chair: String?) -> String {
        switch kind {
        case .user: "You"
        case .assistant: chair?.capitalized ?? "Chair"
        case .thought: "Thinking"
        case .toolUse: "Tool"
        case .toolResult: "Result"
        case .diff: "Changes"
        case .permission: "Permission"
        case .error: "Error"
        case .notice: "Notice"
        case .system: "System"
        case .result: "Turn"
        }
    }

    public var isHiddenByDefault: Bool {
        (kind == .notice && (text.hasPrefix("hook_success")
            || text.hasPrefix("title:") || text.hasPrefix("model:")))
            || (kind == .system && text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
    }

    public var printLine: String {
        let first = text.components(separatedBy: .newlines).first ?? ""
        return "\(kind.rawValue) \(String(first.prefix(80)))"
    }
}

public enum TranscriptRowBuilder {
    public static func rows(from records: some Sequence<TranscriptRecord>, indexOffset: Int = 0) -> [TranscriptRow] {
        rows(from: records.map(\.event), indexOffset: indexOffset)
    }

    public static func rows(from events: some Sequence<TranscriptEvent>, indexOffset: Int = 0) -> [TranscriptRow] {
        let events = Array(events)
        let scopes = scopes(for: events)
        let calls = events.enumerated().compactMap { index, event -> ToolCall? in
            guard case .toolCall(let id, _, _, _, let meta) = event else { return nil }
            return ToolCall(index: index, id: id, session: meta.sessionID, scope: scopes[index])
        }
        var updates: [Int: [Int]] = [:]
        var diffs: [Int: [Int]] = [:]
        var joined = Set<Int>()
        for (index, event) in events.enumerated() {
            let id: String
            let session: String
            let isUpdate: Bool
            switch event {
            case .toolCallUpdate(let callID, _, _, let meta):
                (id, session, isUpdate) = (callID, meta.sessionID, true)
            case .toolDiff(let diff, let meta):
                (id, session, isUpdate) = (diff.toolCallID, meta.sessionID, false)
            default:
                continue
            }
            guard !id.isEmpty else { continue }
            let matches = calls.filter {
                $0.id == id && $0.scope == scopes[index]
                    && (session.isEmpty || $0.session.isEmpty || $0.session == session)
            }
            guard matches.count == 1, let call = matches.first else { continue }
            if isUpdate { updates[call.index, default: []].append(index) }
            else { diffs[call.index, default: []].append(index) }
            joined.insert(index)
        }

        var endings: [Int: TurnEndedReason] = [:]
        for (index, event) in events.enumerated() {
            if case .turnEnded(_, let reason, _) = event { endings[scopes[index]] = reason }
        }

        var rows: [TranscriptRow] = []
        var usedIDs = Set<String>()
        var lastSourceID: String?
        for (index, event) in events.enumerated() {
            if joined.contains(index) {
                lastSourceID = nil
                continue
            }
            guard var row = row(from: event, index: index + indexOffset) else { continue }
            if case .toolCall(_, let name, let input, let status, _) = event {
                let relatedUpdates = (updates[index] ?? []).sorted()
                let lastUpdate = relatedUpdates.last.map { events[$0] }
                let relatedDiffs = (diffs[index] ?? []).sorted().compactMap { offset -> TranscriptDiff? in
                    guard case .toolDiff(let diff, _) = events[offset] else { return nil }
                    return diff
                }
                var output: String?
                var finalStatus = status
                if case .toolCallUpdate(_, let updateStatus, let content, _) = lastUpdate {
                    output = content
                    finalStatus = updateStatus
                }
                let state: TranscriptToolActivity.State
                switch finalStatus {
                case .failed: state = .failed
                case .completed where lastUpdate != nil: state = .finished
                default:
                    switch endings[scopes[index]] {
                    case .aborted: state = .interrupted
                    case .completed: state = .unreported
                    case nil: state = finalStatus == .completed ? .finished : .waiting
                    }
                }
                row.tool = TranscriptToolActivity(
                    name: name, input: input, output: output, diffs: relatedDiffs,
                    state: state, command: TranscriptToolActivity.command(in: input, name: name),
                    path: TranscriptToolActivity.path(in: input)
                )
            }
            if let last = rows.last, last.kind == row.kind, lastSourceID == row.eventID,
               row.kind == .user || row.kind == .assistant || row.kind == .thought {
                rows[rows.count - 1].text += row.text
                continue
            }
            lastSourceID = row.eventID
            let sourceID = row.eventID
            var suffix = 0
            while usedIDs.contains(row.eventID) {
                suffix += 1
                row.eventID = "\(sourceID)#\(index + indexOffset)-\(suffix)"
            }
            usedIDs.insert(row.eventID)
            rows.append(row)
        }
        return rows
    }

    private struct ToolCall {
        let index: Int
        let id: String
        let session: String
        let scope: Int
    }

    private static func scopes(for events: [TranscriptEvent]) -> [Int] {
        var scope = 0
        var sawTool = false
        var scopes: [Int] = []
        for event in events {
            switch event {
            case .page, .turnStarted:
                scope += 1
                sawTool = false
            case .userMessageChunk where sawTool:
                scope += 1
                sawTool = false
            default:
                break
            }
            scopes.append(scope)
            switch event {
            case .toolCall, .toolCallUpdate, .toolDiff: sawTool = true
            case .turnEnded:
                scope += 1
                sawTool = false
            default: break
            }
        }
        return scopes
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
        case .toolCall(let id, let name, let input, let status, _):
            row = TranscriptRow(
                kind: .toolUse, text: toolSummary(name: name, input: input), eventID: id + ":call"
            )
            row.detail = input.compactJSON
            row.toolStatus = status
        case .toolCallUpdate(let id, let status, let content, _):
            row = TranscriptRow(kind: .toolResult, text: content, eventID: id + ":result")
            row.toolStatus = status
        case .toolDiff(let diff, _):
            let added = diff.hunks.reduce(0) { $0 + $1.lines.filter { $0.hasPrefix("+") }.count }
            let removed = diff.hunks.reduce(0) { $0 + $1.lines.filter { $0.hasPrefix("-") }.count }
            row = TranscriptRow(
                kind: .diff, text: "\((diff.path as NSString).lastPathComponent) · +\(added) −\(removed)",
                eventID: diff.toolCallID + ":diff"
            )
            row.diff = diff
            row.detail = diff.path
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
        let fields: [String: JSONElement] = if case .object(let value) = input { value } else { [:] }
        let description = ["description", "Description", "toolSummary"].compactMap { key -> String? in
            if case .string(let value) = fields[key] { return value }
            return nil
        }.first
        let command = TranscriptToolActivity.command(in: input, name: name)
        let path = TranscriptToolActivity.path(in: input).map { ($0 as NSString).lastPathComponent }
        let detail = [description, command?.components(separatedBy: .newlines).first, path]
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
