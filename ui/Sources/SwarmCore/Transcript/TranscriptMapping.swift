import Foundation

/// One drawn row, before it is given an id, a sequence number and a session.
///
/// `kind` chooses the view. `payload` is one line in the Claude envelope, which is the shape every
/// renderer downstream reads, whichever provider wrote it. `refID` ties a tool result to its call.
public struct TranscriptBlock: Sendable, Hashable {
    public var kind: MessageKind
    public var payload: Data
    public var refID: String?

    public init(kind: MessageKind, payload: Data, refID: String? = nil) {
        self.kind = kind
        self.payload = payload
        self.refID = refID
    }
}

/// Every rule that turns one line of a provider log into rows, in one place.
///
/// It is one type because these rules are the part that moves. A provider adds a record type every
/// few weeks, and the same edit used to be made in two files that had drifted apart. Nothing here
/// reads a file, holds a session, or draws anything, so a rule can be read and changed without the
/// reader, the store or the view around it.
///
/// The output vocabulary is `MessageKind` plus the Claude envelope, and `schemaJSON` publishes it
/// with the drop lists, so another renderer, in a browser or in Go, can draw the same rows without
/// rediscovering which records are noise.
public enum TranscriptMapping {
    /// The brief is drawn above the conversation rather than inside it, so it is not a block.
    public enum Reading: Sendable, Hashable {
        case brief(String)
        case block(TranscriptBlock)
    }

    /// Whether the first user text of a file is the brief above the chat or an ordinary row.
    public enum UserText: Sendable, Hashable {
        case brief
        case row
    }

    // MARK: - Claude Code

    /// Record types that carry no account of the conversation, so they never become a row.
    ///
    /// Measured across 446 captures: these are 61% of all lines, led by `attachment` at 27% and
    /// `last-prompt`, `atis-latch`, `mode` and `permission-mode` at about 4.5% each. Every one of
    /// them is app state the pane already shows elsewhere or does not show at all. They are
    /// dropped HERE rather than hidden in the view so that they never take a place under
    /// `rowLimit`, which counts rows and not lines.
    ///
    /// Anything not on this list becomes a row, even a type Swarm has never seen. That is the
    /// point: two new types appeared in one month of captures, `file-history-delta` and
    /// `pr-link`, and a type nobody has written code for must still be visible.
    public static let claudeDeniedTypes: Set<String> = [
        "agent-name",
        "ai-title",
        "artifact-autoreact-ledger",
        "artifact-comment-monitor",
        "atis-latch",
        "attachment",
        "bridge-session",
        "continued-in",
        "cost-state",
        "custom-title",
        "file-history-delta",
        "file-history-snapshot",
        "frame-link",
        "history-suppression",
        "last-prompt",
        "mode",
        "permission-mode",
        "queue-operation",
    ]

    /// The openings of a user line the reader never typed.
    public static let claudeScaffoldingPrefixes = [
        "<local-command-caveat>",
        "<local-command-stdout>",
        "<command-name>",
        "<command-message>",
        "<command-args>",
        "<system-reminder>",
        "<task-notification>",
    ]

    /// A user line the reader never typed: a slash command's echo and output, a reminder Claude
    /// Code injected, or a subagent's finish notice.
    ///
    /// These used to be dropped, which is why `/compact` left nothing behind. They are 4% of user
    /// lines across 446 captures, so drawing each as one collapsed row costs almost nothing and
    /// stops the chat from losing a turn the reader can see happening.
    public static func claudeScaffolding(_ json: JSONValue) -> Bool {
        guard json["type"]?.stringValue == "user" else { return false }
        if json["isMeta"]?.boolValue == true { return true }
        guard let content = json["message"]?["content"] else { return false }
        let text = content.stringValue ?? content.arrayValue?
            .compactMap { $0["text"]?.stringValue }
            .first
        guard let text else { return false }
        return claudeScaffoldingPrefixes.contains { text.hasPrefix($0) }
    }

    public static func claude(
        _ json: JSONValue, raw: Data, userText: UserText
    ) -> [Reading] {
        guard let type = json["type"]?.stringValue else { return [] }
        // Anything the reader does not know becomes one opaque row rather than nothing. A dropped
        // line is a feature that vanished from the chat with no trace that it happened; an opaque
        // row says the provider sent something and keeps its bytes for the reader to open.
        guard type == "user" || type == "assistant" else {
            return claudeDeniedTypes.contains(type)
                ? []
                : [.block(TranscriptBlock(kind: .system, payload: raw))]
        }
        guard let message = json["message"] else {
            return [.block(TranscriptBlock(kind: .system, payload: raw))]
        }
        let isUser = type == "user"

        // The first user line of a file is the brief, and it arrives as a bare string rather than
        // as blocks. An assistant message shaped the same way is the thing it said.
        if let content = message["content"]?.stringValue {
            let body = content.trimmingCharacters(in: .whitespacesAndNewlines)
            guard !body.isEmpty else { return [] }
            if isUser {
                return userText == .brief
                    ? [.brief(body)]
                    : [.block(TranscriptBlock(kind: .user, payload: raw))]
            }
            guard let payload = oneBlockLine(json, holding: .object([
                "type": .string("text"), "text": .string(body),
            ])) else { return [] }
            return [.block(TranscriptBlock(kind: .assistantText, payload: payload))]
        }

        let blocks = message["content"]?.arrayValue ?? []
        return blocks.compactMap { block in
            claude(
                block: block, in: json, raw: raw, isOnlyBlock: blocks.count == 1,
                isUser: isUser, userText: userText
            )
        }
    }

    private static func claude(
        block: JSONValue, in json: JSONValue, raw: Data, isOnlyBlock: Bool,
        isUser: Bool, userText: UserText
    ) -> Reading? {
        // The bytes of the line itself wherever the line holds one block, which is every line in
        // every capture measured. It is the payload every renderer downstream wants: the uuid,
        // the model, the usage and `tool_result_meta` are all outside `content` and all of them
        // are lost by rebuilding the line rather than keeping it.
        func payload() -> Data? {
            isOnlyBlock ? raw : oneBlockLine(json, holding: block)
        }

        switch block["type"]?.stringValue {
        case "text":
            let body = (block["text"]?.stringValue ?? "")
                .trimmingCharacters(in: .whitespacesAndNewlines)
            guard !body.isEmpty else { return nil }
            // A text block on a USER line is the brief, not an answer. Reading it as an answer is
            // what drew the prompt under both headings.
            if isUser {
                guard userText == .row else { return .brief(body) }
                guard let payload = payload() else { return nil }
                return .block(TranscriptBlock(kind: .user, payload: payload))
            }
            guard let payload = payload() else { return nil }
            return .block(TranscriptBlock(kind: .assistantText, payload: payload))

        case "thinking":
            let thought = (block["thinking"]?.stringValue ?? "")
                .trimmingCharacters(in: .whitespacesAndNewlines)
            guard !isUser, !thought.isEmpty, let payload = payload() else { return nil }
            return .block(TranscriptBlock(kind: .thinking, payload: payload))

        case "tool_use":
            guard !isUser, let payload = payload() else { return nil }
            return .block(TranscriptBlock(
                kind: .toolUse, payload: payload, refID: block["id"]?.stringValue
            ))

        case "tool_result":
            guard let payload = payload() else { return nil }
            return .block(TranscriptBlock(
                kind: .toolResult, payload: payload, refID: block["tool_use_id"]?.stringValue
            ))

        // A block type nobody has written a case for. It keeps its bytes and draws collapsed,
        // for the same reason the line above it does.
        default:
            guard let payload = payload() else { return nil }
            return .block(TranscriptBlock(kind: .system, payload: payload))
        }
    }

    /// The same line with one block where its content was, for the message that carried several.
    ///
    /// One line means one row throughout Swarm, and `AgentEvent` reads the first block of a
    /// message and no others, so a message with two blocks in it has to become two lines before
    /// either can be drawn. Every key outside `content` is kept, which is what makes this safe to
    /// do to a line Swarm does not own.
    public static func oneBlockLine(_ json: JSONValue, holding block: JSONValue) -> Data? {
        guard case .object(var top) = json, case .object(var message)? = json["message"] else {
            return nil
        }
        message["content"] = .array([block])
        top["message"] = .object(message)
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.withoutEscapingSlashes]
        return try? encoder.encode(JSONValue.object(top))
    }

    // MARK: - Codex

    /// The Codex records that are session state rather than conversation.
    ///
    /// `event_msg` repeats what the response items already say and counts tokens. `session_meta`,
    /// `turn_context`, `world_state` and `token_usage_record` describe the session. The owner saw
    /// the last two drawn as rows of bare type names around a two-line answer.
    ///
    /// The list is by name because the alternative was a blanket skip, and a blanket skip is how a
    /// new record type disappears without anybody noticing. Every reader of these logs keeps such
    /// a list; `claude-code-viewer` reached the same shape through five schema breaks in four
    /// months (issues #213, #218, #228, #231, PR #235).
    public static let codexStateRecords: Set<String> = [
        "event_msg", "session_meta", "turn_context", "world_state", "token_usage_record",
    ]

    /// The openings of a Codex user message that the CLI sent rather than the reader.
    public static let codexHiddenTextPrefixes = [
        "# AGENTS.md instructions for ",
        "<environment_context>",
        "<permissions instructions>",
    ]

    /// Codex rollout lines, mapped into the same envelopes the Claude reading produces.
    public static func codex(
        _ json: JSONValue, raw: Data, providerSessionID: String
    ) -> [TranscriptBlock] {
        guard json["type"]?.stringValue == "response_item",
              let payload = json["payload"]
        else {
            // Named rather than blanket, so a record Codex adds next month is one folded row
            // instead of nothing at all. See `codexStateRecords`.
            return codexStateRecords.contains(json["type"]?.stringValue ?? "")
                ? []
                : [TranscriptBlock(kind: .system, payload: raw)]
        }

        // A call and its output, which is 50% of a rollout and drew nothing at all before.
        // They are rebuilt in the Claude vocabulary the rows are stored in, exactly the way
        // the live stream is, so one presenter draws both. See `CodexTranslation`.
        switch payload["type"]?.stringValue {
        case "function_call":
            let callID = payload["call_id"]?.stringValue ?? ""
            return [TranscriptBlock(
                kind: .toolUse,
                payload: CodexTranslation.assistantLine(
                    blocks: [.object([
                        "type": .string("tool_use"),
                        "id": .string(callID),
                        "name": .string(payload["name"]?.stringValue ?? "call"),
                        "input": codexArguments(payload["arguments"]),
                    ])],
                    messageID: callID, model: "", usage: .zero, sessionID: providerSessionID
                ),
                refID: callID
            )]

        case "function_call_output":
            let callID = payload["call_id"]?.stringValue ?? ""
            return [TranscriptBlock(
                kind: .toolResult,
                payload: CodexTranslation.toolResultLine(
                    toolUseID: callID,
                    text: payload["output"]?.stringValue ?? "",
                    isError: false, refusalKind: nil, sessionID: providerSessionID
                ),
                refID: callID
            )]

        case "reasoning":
            // Every one of the 564 reasoning records measured carries `encrypted_content` and
            // an empty `summary`, so most of these make no row. When a summary does arrive it
            // is the model's own account of its thinking, and it draws like Claude's.
            let thought = (payload["summary"]?.arrayValue ?? [])
                .compactMap { $0["text"]?.stringValue }
                .joined(separator: "\n\n")
                .trimmingCharacters(in: .whitespacesAndNewlines)
            guard !thought.isEmpty else { return [] }
            return [TranscriptBlock(
                kind: .thinking,
                payload: CodexTranslation.assistantLine(
                    blocks: [.object([
                        "type": .string("thinking"), "thinking": .string(thought),
                    ])],
                    messageID: "", model: "", usage: .zero, sessionID: providerSessionID
                )
            )]

        case "message":
            break

        default:
            // Every other item is something the agent did, such as a web search, so it keeps
            // its bytes and draws collapsed until it has a case of its own.
            return [TranscriptBlock(kind: .system, payload: raw)]
        }

        // `developer` and `system` messages are the instructions Codex sends ahead of the
        // chat: the skills list, AGENTS.md and hook output. Nobody in the chat said them.
        guard let role = payload["role"]?.stringValue,
              role == "user" || role == "assistant"
        else { return [] }

        let expected = role == "user" ? "input_text" : "output_text"
        let parts = (payload["content"]?.arrayValue ?? []).compactMap { block -> String? in
            guard block["type"]?.stringValue == expected,
                  let value = block["text"]?.stringValue,
                  !hidesCodexText(value) else { return nil }
            let trimmed = value.trimmingCharacters(in: .whitespacesAndNewlines)
            return trimmed.isEmpty ? nil : trimmed
        }
        let body = parts.joined(separator: "\n\n")
        guard !body.isEmpty else { return [] }

        if role == "user" {
            return [TranscriptBlock(kind: .user, payload: userLine(body))]
        }
        return [TranscriptBlock(
            kind: .assistantText,
            payload: CodexTranslation.assistantLine(
                blocks: [.object(["type": .string("text"), "text": .string(body)])],
                messageID: payload["id"]?.stringValue ?? "",
                model: "", usage: .zero, sessionID: providerSessionID
            )
        )]
    }

    /// A call's arguments, which Codex sends as a JSON string rather than as an object.
    ///
    /// Parsed so the tool row can name the command it ran instead of showing an escaped blob. A
    /// string that will not parse is kept as itself, because the row still has to show something.
    private static func codexArguments(_ value: JSONValue?) -> JSONValue {
        guard let text = value?.stringValue else { return value ?? .object([:]) }
        return JSONValue.parse(Data(text.utf8)) ?? .string(text)
    }

    private static func hidesCodexText(_ text: String) -> Bool {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        return codexHiddenTextPrefixes.contains { trimmed.hasPrefix($0) }
    }

    /// One user line in the Claude envelope, for text that no provider log holds yet.
    public static func userLine(_ text: String) -> Data {
        let json = JSONValue.object([
            "type": .string("user"),
            "message": .object([
                "role": .string("user"),
                "content": .array([.object([
                    "type": .string("text"), "text": .string(text),
                ])]),
            ]),
        ])
        return Data(json.compactJSON.utf8)
    }

    // MARK: - Published schema

    /// Raise this when a kind is added or removed, or when the envelope of a payload changes.
    public static let schemaVersion = 1

    /// The vocabulary and the drop lists, as JSON, so a renderer outside this app can draw the
    /// same rows. `docs/transcript-blocks.json` holds a copy, and a test fails when it drifts.
    public static var schemaJSON: String {
        let json = JSONValue.object([
            "version": .integer(schemaVersion),
            // Whichever provider wrote the line, the payload a renderer receives is one Claude
            // Code JSONL line, so a renderer needs one reader and not one for each provider.
            "envelope": .string("claude-code-jsonl-line"),
            "kinds": .array(MessageKind.allCases.map { .string($0.rawValue) }),
            "claude": .object([
                "deniedTypes": .array(claudeDeniedTypes.sorted().map { .string($0) }),
                "scaffoldingPrefixes": .array(claudeScaffoldingPrefixes.map { .string($0) }),
            ]),
            "codex": .object([
                "stateRecords": .array(codexStateRecords.sorted().map { .string($0) }),
                "hiddenTextPrefixes": .array(codexHiddenTextPrefixes.map { .string($0) }),
            ]),
        ])
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys, .withoutEscapingSlashes]
        guard let data = try? encoder.encode(json),
              let text = String(data: data, encoding: .utf8)
        else { return "" }
        return text + "\n"
    }
}
