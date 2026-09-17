import Foundation

/// The durable decisions for a chat hosted by an interactive terminal CLI.
public enum InteractiveChatLifecycle {
    public enum State: String, Sendable, Hashable {
        case starting
        case running
        case stopped

        public var label: String {
            switch self {
            case .starting: "Starting"
            case .running: "Running"
            case .stopped: "Stopped"
            }
        }
    }

    public enum ExitDisposition: Sendable, Hashable {
        case closePane
        case keepChat
    }

    /// A terminal chat is a saved conversation, so its shell ending cannot mean Archive.
    public static func disposition(
        for exit: TerminalExit, isTerminalChat: Bool
    ) -> ExitDisposition {
        isTerminalChat || !exit.closesPane ? .keepChat : .closePane
    }

    public static func state(agentIsPresent: Bool, launchIsPending: Bool) -> State {
        if agentIsPresent { return .running }
        return launchIsPending ? .starting : .stopped
    }

    /// A workspace row says Stopped only when every terminal chat in it has stopped.
    public static func workspaceLabel(for states: some Sequence<State>) -> String? {
        let states = Array(states)
        return !states.isEmpty && states.allSatisfy { $0 == .stopped } ? State.stopped.label : nil
    }

    /// Claude Code accepts Swarm's UUID at creation. Codex chooses its own and reports it by hook.
    public static func initialProviderSessionID(
        for agent: AgentKind, sessionID: SessionID
    ) -> String? {
        agent == .claudeCode ? sessionID.rawValue : nil
    }

    public static func resumeSessionID(_ stored: String?) -> String? {
        guard let id = stored?.trimmingCharacters(in: .whitespacesAndNewlines), !id.isEmpty else {
            return nil
        }
        return id
    }
}

/// Finds and reads the provider-owned log for a stopped interactive chat.
public enum InteractiveChatTranscript {
    public enum Failure: Error, Sendable, Hashable {
        case noSessionID
        case unsupported
        case missing
        case unreadable

        public var sentence: String {
            switch self {
            case .noSessionID:
                "This chat has no saved provider session ID, so its conversation log cannot be found."
            case .unsupported:
                "This provider does not have a readable terminal conversation log."
            case .missing:
                "The provider conversation log could not be found."
            case .unreadable:
                "The provider conversation log could not be read."
            }
        }
    }

    public static func reader(
        agent: AgentKind,
        providerSessionID: String?,
        sessionID: SessionID,
        home: URL = FileManager.default.homeDirectoryForCurrentUser
    ) -> Result<TranscriptLogReader, Failure> {
        guard let providerSessionID = InteractiveChatLifecycle.resumeSessionID(providerSessionID) else {
            return .failure(.noSessionID)
        }
        guard let path = path(agent: agent, providerSessionID: providerSessionID, home: home) else {
            return agent == .claudeCode || agent == .codex ? .failure(.missing) : .failure(.unsupported)
        }
        switch agent {
        case .claudeCode:
            return .success(TranscriptLogReader(
                url: path, format: .claude(sessionID: sessionID)
            ))
        case .codex:
            return .success(TranscriptLogReader(
                url: path,
                format: .codex(sessionID: sessionID, providerSessionID: providerSessionID)
            ))
        case .cursor, .openCode, .grok:
            return .failure(.unsupported)
        }
    }

    public static func read(
        _ reader: TranscriptLogReader
    ) async -> Result<SubagentTranscript, Failure> {
        do {
            return .success(try await reader.read())
        } catch {
            return .failure(.unreadable)
        }
    }

    /// The newest exact match wins when more than one CLI profile contains the same id.
    public static func path(
        agent: AgentKind,
        providerSessionID: String,
        home: URL = FileManager.default.homeDirectoryForCurrentUser
    ) -> URL? {
        guard safe(providerSessionID) else { return nil }
        switch agent {
        case .claudeCode:
            return newest(claudePaths(providerSessionID, home: home))
        case .codex:
            return newest(codexPaths(providerSessionID, home: home))
        case .cursor, .openCode, .grok:
            return nil
        }
    }

    /// Codex rollout lines are mapped into the same user and assistant envelopes the transcript draws.
    public static func parseCodex(
        _ text: String, sessionID: SessionID, providerSessionID: String,
        limit: Int? = SubagentTranscript.rowLimit
    ) -> SubagentTranscript {
        var messages: [Message] = []
        var used = Set<Int64>()

        for source in text.split(whereSeparator: \.isNewline) {
            guard let json = JSONValue.parse(Data(source.utf8)),
                  json["type"]?.stringValue == "response_item",
                  let payload = json["payload"], payload["type"]?.stringValue == "message",
                  let role = payload["role"]?.stringValue,
                  role == "user" || role == "assistant"
            else { continue }

            let expected = role == "user" ? "input_text" : "output_text"
            let parts = (payload["content"]?.arrayValue ?? []).compactMap { block -> String? in
                guard block["type"]?.stringValue == expected,
                      let value = block["text"]?.stringValue,
                      !hidesCodexSystemText(value) else { return nil }
                let trimmed = value.trimmingCharacters(in: .whitespacesAndNewlines)
                return trimmed.isEmpty ? nil : trimmed
            }
            let body = parts.joined(separator: "\n\n")
            guard !body.isEmpty else { continue }

            let rowPayload = role == "user"
                ? userLine(body)
                : CodexTranslation.assistantLine(
                    blocks: [.object(["type": .string("text"), "text": .string(body)])],
                    messageID: payload["id"]?.stringValue ?? "",
                    model: "", usage: .zero, sessionID: providerSessionID
                )
            var id = SubagentTranscript.rowID(for: rowPayload)
            while used.contains(id) { id -= 1 }
            used.insert(id)
            messages.append(Message(
                id: id,
                sessionID: sessionID,
                seq: messages.count,
                kind: role == "user" ? .user : .assistantText,
                payload: rowPayload
            ))
        }

        guard let limit else { return SubagentTranscript(messages: messages) }
        let dropped = max(0, messages.count - limit)
        return SubagentTranscript(
            messages: Array(messages.suffix(limit)), droppedRows: dropped
        )
    }

    private static func userLine(_ text: String) -> Data {
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

    private static func hidesCodexSystemText(_ text: String) -> Bool {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed.hasPrefix("# AGENTS.md instructions for ")
            || trimmed.hasPrefix("<environment_context>")
            || trimmed.hasPrefix("<permissions instructions>")
    }

    private static func safe(_ id: String) -> Bool {
        !id.isEmpty && id.unicodeScalars.allSatisfy {
            CharacterSet.alphanumerics.contains($0) || $0 == "-" || $0 == "_"
        }
    }

    private static func profileRoots(prefix: String, home: URL) -> [URL] {
        let manager = FileManager.default
        return (try? manager.contentsOfDirectory(
            at: home, includingPropertiesForKeys: nil
        ))?.filter { $0.lastPathComponent.hasPrefix(prefix) }
            .sorted { $0.path < $1.path } ?? []
    }

    private static func claudePaths(_ id: String, home: URL) -> [URL] {
        let manager = FileManager.default
        return profileRoots(prefix: ".claude", home: home).flatMap { root in
            let projects = root.appendingPathComponent("projects", isDirectory: true)
            let directories = (try? manager.contentsOfDirectory(
                at: projects, includingPropertiesForKeys: nil, options: [.skipsHiddenFiles]
            )) ?? []
            return directories.compactMap { directory in
                let candidate = directory.appendingPathComponent(id + ".jsonl")
                return manager.fileExists(atPath: candidate.path) ? candidate : nil
            }
        }
    }

    private static func codexPaths(_ id: String, home: URL) -> [URL] {
        let manager = FileManager.default
        var matches: [URL] = []
        for root in profileRoots(prefix: ".codex", home: home) {
            let sessions = root.appendingPathComponent("sessions", isDirectory: true)
            guard let files = manager.enumerator(
                at: sessions,
                includingPropertiesForKeys: [.contentModificationDateKey, .isRegularFileKey],
                options: [.skipsHiddenFiles, .skipsPackageDescendants]
            ) else { continue }
            for case let file as URL in files
            where file.lastPathComponent.hasPrefix("rollout-")
                && file.lastPathComponent.hasSuffix("-\(id).jsonl") {
                matches.append(file)
            }
        }
        return matches
    }

    private static func newest(_ paths: [URL]) -> URL? {
        paths.max { lhs, rhs in
            let left = (try? lhs.resourceValues(forKeys: [.contentModificationDateKey])
                .contentModificationDate) ?? .distantPast
            let right = (try? rhs.resourceValues(forKeys: [.contentModificationDateKey])
                .contentModificationDate) ?? .distantPast
            return left == right ? lhs.path < rhs.path : left < right
        }
    }
}
