import Foundation

/// Metadata identifying the origin and timing of a transcript event.
public struct Meta: Sendable, Hashable, Decodable {
    public var agentSessionID: String
    public var uuid: String
    public var timestamp: String

    public var sessionID: String { agentSessionID }

    public init(agentSessionID: String = "", uuid: String = "", timestamp: String = "") {
        self.agentSessionID = agentSessionID
        self.uuid = uuid
        self.timestamp = timestamp
    }

    enum CodingKeys: String, CodingKey {
        case agentSessionID = "session_id"
        case uuid
        case timestamp
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        self.agentSessionID = (try container.decodeIfPresent(String.self, forKey: .agentSessionID)) ?? ""
        self.uuid = (try container.decodeIfPresent(String.self, forKey: .uuid)) ?? ""
        self.timestamp = (try container.decodeIfPresent(String.self, forKey: .timestamp)) ?? ""
    }
}

public enum TurnEndedReason: String, Sendable, Hashable, Decodable {
    case completed
    case aborted
}

public enum SessionInfoKind: String, Sendable, Hashable, Decodable {
    case title
    case agentName = "agent_name"
    case model
    case cwd
}

public enum ToolStatus: String, Sendable, Hashable, Decodable {
    case pending
    case completed
    case failed
}

public struct ToolOption: Sendable, Hashable, Decodable {
    public var label: String
    public var description: String

    public init(label: String, description: String = "") {
        self.label = label
        self.description = description
    }

    enum CodingKeys: String, CodingKey {
        case label
        case description
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        self.label = (try container.decodeIfPresent(String.self, forKey: .label)) ?? ""
        self.description = (try container.decodeIfPresent(String.self, forKey: .description)) ?? ""
    }
}

public struct Question: Sendable, Hashable, Decodable {
    public var question: String
    public var header: String
    public var multiSelect: Bool
    public var options: [ToolOption]

    public init(
        question: String,
        header: String = "",
        multiSelect: Bool = false,
        options: [ToolOption] = []
    ) {
        self.question = question
        self.header = header
        self.multiSelect = multiSelect
        self.options = options
    }

    enum CodingKeys: String, CodingKey {
        case question
        case header
        case multiSelect = "multi_select"
        case options
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        self.question = (try container.decodeIfPresent(String.self, forKey: .question)) ?? ""
        self.header = (try container.decodeIfPresent(String.self, forKey: .header)) ?? ""
        self.multiSelect = (try container.decodeIfPresent(Bool.self, forKey: .multiSelect)) ?? false
        self.options = (try container.decodeIfPresent([ToolOption].self, forKey: .options)) ?? []
    }
}

public struct Answer: Sendable, Hashable, Decodable {
    public var question: String
    public var answer: String

    public init(question: String, answer: String) {
        self.question = question
        self.answer = answer
    }

    enum CodingKeys: String, CodingKey {
        case question
        case answer
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        self.question = (try container.decodeIfPresent(String.self, forKey: .question)) ?? ""
        self.answer = (try container.decodeIfPresent(String.self, forKey: .answer)) ?? ""
    }
}

/// Generic JSON tree representation for tool input payloads.
public enum JSONElement: Sendable, Hashable, Codable {
    case null
    case bool(Bool)
    case integer(Int64)
    case double(Double)
    case string(String)
    case array([JSONElement])
    case object([String: JSONElement])

    public init(from decoder: Decoder) throws {
        let container = try decoder.singleValueContainer()
        if container.decodeNil() {
            self = .null
        } else if let b = try? container.decode(Bool.self) {
            self = .bool(b)
        } else if let i = try? container.decode(Int64.self) {
            self = .integer(i)
        } else if let d = try? container.decode(Double.self) {
            self = .double(d)
        } else if let s = try? container.decode(String.self) {
            self = .string(s)
        } else if let arr = try? container.decode([JSONElement].self) {
            self = .array(arr)
        } else if let dict = try? container.decode([String: JSONElement].self) {
            self = .object(dict)
        } else {
            self = .null
        }
    }

    public func encode(to encoder: Encoder) throws {
        var container = encoder.singleValueContainer()
        switch self {
        case .null:
            try container.encodeNil()
        case .bool(let b):
            try container.encode(b)
        case .integer(let i):
            try container.encode(i)
        case .double(let d):
            try container.encode(d)
        case .string(let s):
            try container.encode(s)
        case .array(let arr):
            try container.encode(arr)
        case .object(let dict):
            try container.encode(dict)
        }
    }

    public var compactJSON: String {
        guard let data = try? JSONEncoder().encode(self) else { return "{}" }
        return String(decoding: data, as: UTF8.self)
    }
}

/// One event emitted by the Zig transcript binary.
public enum TranscriptEvent: Sendable, Hashable, Decodable {
    case ignored(kind: String, meta: Meta)
    case turnStarted(meta: Meta)
    case turnEnded(durationMs: Int64?, reason: TurnEndedReason, meta: Meta)
    case error(message: String, meta: Meta)
    case systemMessage(kind: String, text: String, meta: Meta)
    case sessionInfo(kind: String, value: String, meta: Meta)
    case image(role: String, mediaType: String, meta: Meta)
    case userMessageChunk(text: String, meta: Meta)
    case agentMessageChunk(text: String, meta: Meta)
    case agentThoughtChunk(text: String, meta: Meta)
    case toolCall(toolCallID: String, name: String, input: JSONElement, status: ToolStatus, meta: Meta)
    case toolCallUpdate(toolCallID: String, status: ToolStatus, content: String, meta: Meta)
    case elicitation(toolCallID: String, questions: [Question], meta: Meta)
    case elicitationResult(toolCallID: String, answers: [Answer], meta: Meta)
    case hookResult(kind: String, hookEvent: String, hookName: String, toolCallID: String, exitCode: Int64?, meta: Meta)
    case permissionDecision(hookEvent: String, toolCallID: String, decision: String, meta: Meta)
    case page(start: UInt64, end: UInt64)
    case unknown(raw: String, meta: Meta? = nil)

    enum CodingKeys: String, CodingKey {
        case type
        case kind
        case text
        case value
        case message
        case role
        case mediaType = "media_type"
        case durationMs = "duration_ms"
        case reason
        case toolCallID = "tool_call_id"
        case name
        case input
        case status
        case content
        case questions
        case answers
        case hookEvent = "hook_event"
        case hookName = "hook_name"
        case exitCode = "exit_code"
        case decision
        case startOffset = "start_offset"
        case endOffset = "end_offset"
        case raw
        case meta
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        guard let type = try? container.decodeIfPresent(String.self, forKey: .type) else {
            let raw = (try? container.decodeIfPresent(String.self, forKey: .raw)) ?? ""
            let meta = try? container.decodeIfPresent(Meta.self, forKey: .meta)
            self = .unknown(raw: raw, meta: meta)
            return
        }

        let meta = (try? container.decodeIfPresent(Meta.self, forKey: .meta)) ?? Meta()

        switch type {
        case "ignored":
            let kind = (try? container.decodeIfPresent(String.self, forKey: .kind)) ?? ""
            self = .ignored(kind: kind, meta: meta)

        case "turn_started":
            self = .turnStarted(meta: meta)

        case "turn_ended":
            let duration = try? container.decodeIfPresent(Int64.self, forKey: .durationMs)
            let reason = (try? container.decodeIfPresent(TurnEndedReason.self, forKey: .reason)) ?? .completed
            self = .turnEnded(durationMs: duration, reason: reason, meta: meta)

        case "error":
            let message = (try? container.decodeIfPresent(String.self, forKey: .message)) ?? ""
            self = .error(message: message, meta: meta)

        case "system_message":
            let kind = (try? container.decodeIfPresent(String.self, forKey: .kind)) ?? ""
            let text = (try? container.decodeIfPresent(String.self, forKey: .text)) ?? ""
            self = .systemMessage(kind: kind, text: text, meta: meta)

        case "session_info":
            let kind = (try? container.decodeIfPresent(String.self, forKey: .kind)) ?? ""
            let value = (try? container.decodeIfPresent(String.self, forKey: .value)) ?? ""
            self = .sessionInfo(kind: kind, value: value, meta: meta)

        case "image":
            let role = (try? container.decodeIfPresent(String.self, forKey: .role)) ?? "agent"
            let mediaType = (try? container.decodeIfPresent(String.self, forKey: .mediaType)) ?? ""
            self = .image(role: role, mediaType: mediaType, meta: meta)

        case "user_message_chunk":
            let text = (try? container.decodeIfPresent(String.self, forKey: .text)) ?? ""
            self = .userMessageChunk(text: text, meta: meta)

        case "agent_message_chunk":
            let text = (try? container.decodeIfPresent(String.self, forKey: .text)) ?? ""
            self = .agentMessageChunk(text: text, meta: meta)

        case "agent_thought_chunk":
            let text = (try? container.decodeIfPresent(String.self, forKey: .text)) ?? ""
            self = .agentThoughtChunk(text: text, meta: meta)

        case "tool_call":
            let id = (try? container.decodeIfPresent(String.self, forKey: .toolCallID)) ?? ""
            let name = (try? container.decodeIfPresent(String.self, forKey: .name)) ?? ""
            let input = (try? container.decodeIfPresent(JSONElement.self, forKey: .input)) ?? .null
            let status = (try? container.decodeIfPresent(ToolStatus.self, forKey: .status)) ?? .pending
            self = .toolCall(toolCallID: id, name: name, input: input, status: status, meta: meta)

        case "tool_call_update":
            let id = (try? container.decodeIfPresent(String.self, forKey: .toolCallID)) ?? ""
            let status = (try? container.decodeIfPresent(ToolStatus.self, forKey: .status)) ?? .completed
            let content = (try? container.decodeIfPresent(String.self, forKey: .content)) ?? ""
            self = .toolCallUpdate(toolCallID: id, status: status, content: content, meta: meta)

        case "elicitation":
            let id = (try? container.decodeIfPresent(String.self, forKey: .toolCallID)) ?? ""
            let questions = (try? container.decodeIfPresent([Question].self, forKey: .questions)) ?? []
            self = .elicitation(toolCallID: id, questions: questions, meta: meta)

        case "elicitation_result":
            let id = (try? container.decodeIfPresent(String.self, forKey: .toolCallID)) ?? ""
            let answers = (try? container.decodeIfPresent([Answer].self, forKey: .answers)) ?? []
            self = .elicitationResult(toolCallID: id, answers: answers, meta: meta)

        case "hook_result":
            let kind = (try? container.decodeIfPresent(String.self, forKey: .kind)) ?? ""
            let hookEvent = (try? container.decodeIfPresent(String.self, forKey: .hookEvent)) ?? ""
            let hookName = (try? container.decodeIfPresent(String.self, forKey: .hookName)) ?? ""
            let id = (try? container.decodeIfPresent(String.self, forKey: .toolCallID)) ?? ""
            let exitCode = try? container.decodeIfPresent(Int64.self, forKey: .exitCode)
            self = .hookResult(
                kind: kind, hookEvent: hookEvent, hookName: hookName, toolCallID: id,
                exitCode: exitCode, meta: meta
            )

        case "permission_decision":
            let hookEvent = (try? container.decodeIfPresent(String.self, forKey: .hookEvent)) ?? ""
            let id = (try? container.decodeIfPresent(String.self, forKey: .toolCallID)) ?? ""
            let decision = (try? container.decodeIfPresent(String.self, forKey: .decision)) ?? ""
            self = .permissionDecision(hookEvent: hookEvent, toolCallID: id, decision: decision, meta: meta)

        case "page":
            let start = (try? container.decodeIfPresent(UInt64.self, forKey: .startOffset)) ?? 0
            let end = (try? container.decodeIfPresent(UInt64.self, forKey: .endOffset)) ?? 0
            self = .page(start: start, end: end)

        case "unknown":
            let raw = (try? container.decodeIfPresent(String.self, forKey: .raw)) ?? ""
            self = .unknown(raw: raw, meta: meta)

        default:
            // An unknown type decodes as .unknown(raw:), never as a decode error.
            let raw = try? container.decodeIfPresent(String.self, forKey: .raw)
            if let raw, !raw.isEmpty {
                self = .unknown(raw: raw, meta: meta)
            } else if let element = try? JSONElement(from: decoder) {
                self = .unknown(raw: element.compactJSON, meta: meta)
            } else {
                self = .unknown(raw: "{\"type\":\"\(type)\"}", meta: meta)
            }
        }
    }

    /// Decodes one JSON line emitted by the transcript process.
    ///
    /// Non-JSON data or unexpected framing falls back to `.unknown(raw:)` so an unrecognized
    /// record never halts transcript streaming.
    public static func decode(line: String) -> TranscriptEvent {
        let trimmed = line.trimmingCharacters(in: .whitespacesAndNewlines)
        guard let data = trimmed.data(using: .utf8) else {
            return .unknown(raw: line)
        }
        do {
            return try JSONDecoder().decode(TranscriptEvent.self, from: data)
        } catch {
            return .unknown(raw: line)
        }
    }
}
