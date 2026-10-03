import Foundation
import TranscriptTool

/// The text rows a chat can draw from typed transcript events.
public struct TranscriptRow: Sendable, Hashable, Identifiable {
    public enum Kind: String, Sendable, Hashable {
        case user, assistant, thought, toolUse, toolResult, diff, question, error, notice, system, result
        /// A `!` command the owner ran in Claude Code's shell mode, with its output.
        case shell
        /// A line between two conversations of one chat; `text` is its caption and `detail` its time.
        case divider
    }

    public var kind: Kind
    public var text: String
    public var eventID: String
    public var detail: String? = nil
    public var diff: TranscriptDiff? = nil
    public var tool: TranscriptToolActivity? = nil
    public var toolStatus: ToolStatus? = nil
    /// The parser's `system_message` kind, for `.system` rows.
    public var systemKind: String? = nil
    public var shell: TranscriptShellRun? = nil
    /// Set on `.system` rows whose `systemKind` is "command".
    public var command: TranscriptCommandChip? = nil
    public var endsTurn = false
    /// A row that is not the user's but starts an agent turn, such as a background task's end.
    public var startsTurn = false
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
        case .question: "Question"
        case .error: "Error"
        case .notice: "Notice"
        case .system: "System"
        case .shell: "Shell"
        case .result: "Turn"
        case .divider: "Context cleared"
        }
    }

    public var isHiddenByDefault: Bool {
        (kind == .notice && (text.hasPrefix("hook_success")
            || text.hasPrefix("title:") || text.hasPrefix("model:")))
            || (kind == .system && text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
            || (kind == .system && systemKind == "injected")
    }

    public var printLine: String {
        if kind == .shell { return "\(kind.rawValue) \(text)" }
        let first = text.components(separatedBy: .newlines).first ?? ""
        return "\(kind.rawValue) \(String(first.prefix(80)))"
    }
}

/// A slash command the owner typed, as Claude Code echoes it into the session log.
public struct TranscriptCommandChip: Hashable, Sendable {
    /// "/flow", from `<command-name>`.
    public var name: String
    /// From `<command-args>`; "" when absent.
    public var arguments: String
    public var skillBody: String?
    /// The linked `<local-command-stdout>` text, unwrapped and cleaned.
    public var output: String?

    public init(name: String, arguments: String, skillBody: String? = nil, output: String? = nil) {
        self.name = name
        self.arguments = arguments
        self.skillBody = skillBody
        self.output = output
    }

    /// Reads `<command-name>` and `<command-args>`; `<command-message>` repeats the name and is ignored.
    public init(commandText text: String) {
        self.init(
            name: ShellRecord.tagged("command-name", in: text)?.trimmingCharacters(in: .whitespacesAndNewlines) ?? "",
            arguments: ShellRecord.tagged("command-args", in: text)?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        )
    }

    /// The text between `<local-command-stdout>` and `<local-command-stderr>` tags, joined and cleaned.
    static func output(fromCommandOutput text: String) -> String {
        let parts = ["local-command-stdout", "local-command-stderr"].compactMap { ShellRecord.tagged($0, in: text) }
        return ShellRecord.clean(parts.isEmpty ? text : parts.filter { !$0.isEmpty }.joined(separator: "\n"))
    }
}

// ShellRecord.swift declares the run Equatable only, and TranscriptRow is Hashable.
public enum TranscriptRowBuilder {
    public static func rows(from records: some Sequence<TranscriptRecord>, indexOffset: Int = 0) -> [TranscriptRow] {
        rows(from: records.map(\.event), indexOffset: indexOffset)
    }

    public static func rows(from events: some Sequence<TranscriptEvent>, indexOffset: Int = 0) -> [TranscriptRow] {
        let events = Array(events)
        let scopes = scopes(for: events)
        func anchors(_ pick: (TranscriptEvent) -> (id: String, meta: Meta)?) -> [Anchor] {
            events.enumerated().compactMap { index, event in
                pick(event).map { Anchor(index: index, id: $0.id, session: $0.meta.sessionID, scope: scopes[index]) }
            }
        }
        let calls = anchors { if case .toolCall(let id, _, _, _, let meta) = $0 { (id, meta) } else { nil } }
        let shellInputs = anchors { if case .systemMessage("shell_input", _, let meta) = $0 { (meta.uuid, meta) } else { nil } }
        let commands = anchors { if case .systemMessage("command", _, let meta) = $0 { (meta.uuid, meta) } else { nil } }
        /// The one anchor with this id in the event's scope and a compatible session; none when two match.
        func match(_ id: String?, for index: Int, session: String, in anchors: [Anchor]) -> Int? {
            guard let id, !id.isEmpty else { return nil }
            let matches = anchors.filter {
                $0.id == id && $0.scope == scopes[index]
                    && (session.isEmpty || $0.session.isEmpty || $0.session == session)
            }
            return matches.count == 1 ? matches[0].index : nil
        }
        var updates: [Int: [Int]] = [:]
        var diffs: [Int: [Int]] = [:]
        // System records that fold into another event's row: shell output, skill bodies, command output.
        var attached: [Int: [Int]] = [:]
        var joined = Set<Int>()
        for (index, event) in events.enumerated() {
            switch event {
            case .toolCallUpdate(let callID, _, _, let meta):
                guard let call = match(callID, for: index, session: meta.sessionID, in: calls) else { continue }
                updates[call, default: []].append(index)
            case .toolDiff(let diff, let meta):
                guard let call = match(diff.toolCallID, for: index, session: meta.sessionID, in: calls) else { continue }
                diffs[call, default: []].append(index)
            case .systemMessage("shell_output", _, let meta):
                guard let input = match(meta.parentUUID, for: index, session: meta.sessionID, in: shellInputs),
                      attached[input] == nil else { continue }
                attached[input] = [index]
            case .systemMessage("skill_body", _, let meta):
                guard let target = match(meta.parentUUID, for: index, session: meta.sessionID, in: commands)
                    ?? match(meta.sourceToolUseID, for: index, session: meta.sessionID, in: calls)
                else { continue }
                attached[target, default: []].append(index)
            case .systemMessage("command_output", _, let meta):
                guard let command = match(meta.parentUUID, for: index, session: meta.sessionID, in: commands)
                else { continue }
                attached[command, default: []].append(index)
            default:
                continue
            }
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
            if case .toolCall(_, let name, let input, let status, let callMeta) = event {
                let relatedUpdates = (updates[index] ?? []).sorted()
                let lastUpdate = relatedUpdates.last.map { events[$0] }
                let relatedDiffs = (diffs[index] ?? []).sorted().compactMap { offset -> TranscriptDiff? in
                    guard case .toolDiff(let diff, _) = events[offset] else { return nil }
                    return diff
                }
                var output: String?
                var finalStatus = status
                var duration: Double?
                if case .toolCallUpdate(_, let updateStatus, let content, let updateMeta) = lastUpdate {
                    output = content
                    finalStatus = updateStatus
                    duration = TranscriptToolActivity.duration(from: callMeta.timestamp, to: updateMeta.timestamp)
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
                    path: TranscriptToolActivity.path(in: input), duration: duration
                )
            }
            for source in attached[index] ?? [] {
                guard case .systemMessage(let kind, let text, _) = events[source] else { continue }
                switch kind {
                case "shell_output":
                    row = shellRow(ShellRecord.run(command: row.shell?.command, outputText: text), eventID: row.eventID)
                case "skill_body" where row.tool != nil:
                    row.tool?.skillBody = text
                case "skill_body":
                    row.command?.skillBody = text
                    row.startsTurn = true
                default:
                    row.command?.output = TranscriptCommandChip.output(fromCommandOutput: text)
                }
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

    private struct Anchor {
        let index: Int
        let id: String
        let session: String
        let scope: Int
    }

    static func taskNotificationSummary(_ text: String) -> String {
        for tag in ["summary", "status"] {
            if let open = text.range(of: "<\(tag)>"),
               let close = text.range(of: "</\(tag)>", range: open.upperBound..<text.endIndex) {
                let value = text[open.upperBound..<close.lowerBound].trimmingCharacters(in: .whitespacesAndNewlines)
                if !value.isEmpty { return tag == "summary" ? value : "Background task \(value)" }
            }
        }
        return "Background task finished"
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
                kind: .question, text: questions.map(\.question).joined(separator: "\n"),
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
        case .systemMessage("queued_prompt", let text, let meta):
            row = TranscriptRow(kind: .user, text: text, eventID: key(meta, "queued", index))
        case .systemMessage(_, let text, let meta)
            where text.drop(while: \.isWhitespace).hasPrefix("<task-notification>"):
            // Claude writes a background task's end as a user record or a queued command; show its
            // summary line, not the XML.
            row = TranscriptRow(
                kind: .notice, text: taskNotificationSummary(text), eventID: key(meta, "task", index)
            )
            row.startsTurn = true
        case .systemMessage("shell_input", let text, let meta):
            row = shellRow(
                ShellRecord.run(command: ShellRecord.command(fromInput: text), outputText: nil),
                eventID: key(meta, "shell", index)
            )
        case .systemMessage("shell_output", let text, let meta):
            row = shellRow(ShellRecord.run(command: nil, outputText: text), eventID: key(meta, "shell", index))
        case .systemMessage("interrupted", _, let meta):
            row = TranscriptRow(kind: .notice, text: "Interrupted", eventID: key(meta, "interrupted", index))
            row.endsTurn = true
        case .systemMessage(let kind, let text, let meta):
            row = TranscriptRow(kind: .system, text: text, eventID: key(meta, "system", index))
            row.systemKind = kind
            if kind == "command" { row.command = TranscriptCommandChip(commandText: text) }
        case .sessionInfo(SessionInfoKind.agentName.rawValue, _, _):
            return nil
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

    private static func shellRow(_ run: TranscriptShellRun, eventID: String) -> TranscriptRow {
        let text = [run.command.map { "$ \($0)" }, run.output].compactMap { $0 }.filter { !$0.isEmpty }
        var row = TranscriptRow(kind: .shell, text: text.joined(separator: "\n"), eventID: eventID)
        row.shell = run
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
    /// A turn runs from the last row that starts one until a turn-ended row follows it.
    public static func isActive(_ rows: [TranscriptRow]) -> Bool {
        guard let start = rows.lastIndex(where: { $0.kind == .user || $0.startsTurn }) else { return false }
        return !rows[start...].contains(where: \.endsTurn)
    }
}
