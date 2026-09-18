import Foundation

public extension AgentKind {
    func interactiveCommand(
        directory: String,
        prompt: String,
        sessionID: SessionID,
        model: String,
        effort: String,
        permissionMode: PermissionMode? = nil,
        resuming: String? = nil
    ) -> String? {
        guard let arguments = interactiveArguments(
            prompt: prompt, sessionID: sessionID, model: model,
            effort: effort, permissionMode: permissionMode, resuming: resuming
        ) else { return nil }
        let command = TerminalLaunchScript.shellCommand(
            directory: directory,
            executable: "/usr/bin/env",
            arguments: [
                "-u", "NO_COLOR", "TERM=xterm-256color", "COLORTERM=truecolor",
                "SWARM_UI_CLI_STATUS_FILE=\(Self.interactiveStatusURL(sessionID: sessionID).path)",
                executableName
            ] + arguments
        )
        guard command.unicodeScalars.contains(where: { CharacterSet.controlCharacters.contains($0) }) else {
            return command
        }
        let encoded = Data(command.utf8).base64EncodedString()
        return "/bin/sh -c \"$(printf '%s' '\(encoded)' | /usr/bin/base64 -d)\""
    }

    func interactiveArguments(
        prompt: String,
        sessionID: SessionID,
        model: String,
        effort: String,
        permissionMode: PermissionMode? = nil,
        resuming: String? = nil
    ) -> [String]? {
        let events = [
            "SessionStart", "UserPromptSubmit", "PreToolUse", "PostToolUse",
            "PermissionRequest", "Stop", "SessionEnd"
        ]
        var arguments: [String]
        switch self {
        case .claudeCode:
            var hooks: [String: Any] = Dictionary(uniqueKeysWithValues: (events + ["StopFailure"]).map {
                let permission = $0 == "PermissionRequest"
                let hook: [String: Any] = [
                    "type": "command",
                    "command": permission ? Self.interactivePermissionHookCommand : Self.interactiveHookCommand,
                    "timeout": permission ? 130 : 3
                ]
                return ($0, [["hooks": [hook]]] as Any)
            })
            let hook: [String: Any] = ["type": "command", "command": Self.interactiveHookCommand, "timeout": 3]
            hooks["Notification"] = [[
                "matcher": "permission_prompt|idle_prompt",
                "hooks": [hook]
            ]]
            guard let data = try? JSONSerialization.data(withJSONObject: ["hooks": hooks], options: [.sortedKeys]),
                  let settings = String(data: data, encoding: .utf8) else { return nil }
            arguments = resuming.map { ["--resume", $0] } ?? ["--session-id", sessionID.rawValue]
            arguments += ["--settings", settings]
            if let permissionMode {
                arguments += ["--permission-mode", permissionMode.nearest(on: self).cliValue]
            }
            if !effort.isEmpty { arguments += ["--effort", effort] }
        case .codex:
            arguments = resuming.map { ["resume", $0] } ?? []
            if let permissionMode {
                arguments += ["--sandbox", CodexRunner.sandboxMode(for: permissionMode).rawValue,
                              "--ask-for-approval", CodexRunner.approvalPolicy(for: permissionMode).rawValue]
                if permissionMode == .autoReview { arguments += ["--approve-for-me"] }
            }
            for event in events {
                let permission = event == "PermissionRequest"
                let command = Self.interactiveTOMLString(
                    permission ? Self.interactivePermissionHookCommand : Self.interactiveHookCommand
                )
                arguments += ["-c", "hooks.\(event)=[{hooks=[{type=\"command\",command=\(command),"
                    + "timeout=\(permission ? 130 : 3)}]}]"]
            }
            if !effort.isEmpty {
                arguments += ["-c", "model_reasoning_effort=\(Self.interactiveTOMLString(effort))"]
            }
        case .cursor, .openCode, .grok:
            return nil
        }
        if !model.isEmpty {
            arguments += ["--model", self == .claudeCode ? ModelAlias.cliValue(for: model) : model]
        }
        if !prompt.isEmpty { arguments += ["--", prompt] }
        return arguments
    }

    static func interactiveStatusURL(
        sessionID: SessionID,
        base: URL = FileManager.default.temporaryDirectory,
        namespace: String = Bundle.main.bundleIdentifier ?? "unbundled"
    ) -> URL {
        let name = sessionID.rawValue.utf8.map { String(format: "%02x", $0) }.joined()
        let container = namespace.utf8.map { String(format: "%02x", $0) }.joined()
        return base.appendingPathComponent("swarm-cli-status", isDirectory: true)
            .appendingPathComponent(container, isDirectory: true)
            .appendingPathComponent(name + ".json")
    }

    func interactiveSetupOutput(prompt: String?, log: String) -> String {
        let preview = String(String.UnicodeScalarView((prompt ?? "").unicodeScalars.filter {
            !CharacterSet.controlCharacters.contains($0) || CharacterSet.whitespacesAndNewlines.contains($0)
        }))
            .split(whereSeparator: { $0.isWhitespace }).joined(separator: " ")
        var text = "\n  \u{1b}[1;36m" + label + "\u{1b}[0m\n"
            + "  Setting up your workspace\n\n"
        if !preview.isEmpty {
            text += "  \u{1b}[2mQueued prompt\u{1b}[0m\n  "
                + String(preview.prefix(180)) + (preview.count > 180 ? "…" : "") + "\n\n"
        }
        text += "  \u{1b}[2mSetup output · agent starts when ready\u{1b}[0m\n\n"
        return text + (log.isEmpty ? "  Waiting for setup output…\n" : log)
    }

    func interactiveScreenIsBusy(lines: [String]) -> Bool {
        var lines = lines
        while lines.last?.trimmingCharacters(in: .whitespaces).isEmpty == true { lines.removeLast() }
        switch self {
        case .codex:
            return lines.suffix(12).contains {
                $0.range(of: #"^\s*[•●◦⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏]?\s*[^>❯›]+\([0-9]+[smh][^)]* • [^)]* to interrupt\)\s*$"#,
                         options: .regularExpression) != nil
            }
        case .claudeCode:
            return lines.suffix(6).contains {
                $0.range(of: #"^\s*esc (?:to )?interrupt(?:\s*[·•].*)?\s*$"#,
                         options: [.regularExpression, .caseInsensitive]) != nil
            }
        case .cursor, .openCode, .grok:
            return false
        }
    }

    static func interactiveHookState(data: Data) -> SessionState? {
        guard let value = interactiveHookObject(data: data),
              let event = value["hook_event_name"] as? String else { return nil }
        switch event {
        case "UserPromptSubmit", "PreToolUse", "PostToolUse": return .running
        case "PermissionRequest": return .waiting
        case "Stop", "SessionEnd": return .idle
        case "StopFailure": return .failed
        case "SessionStart": return value["source"] as? String == "compact" ? .running : .idle
        case "Notification":
            switch value["notification_type"] as? String {
            case "permission_prompt": return .waiting
            case "idle_prompt": return .idle
            default: return nil
            }
        default: return nil
        }
    }

    static func interactiveHookSessionID(data: Data) -> String? {
        guard let id = interactiveHookObject(data: data)?["session_id"] as? String,
              !id.isEmpty else { return nil }
        return id
    }

    // Stable across panes so Codex can remember the user's hook trust decision.
    static var interactiveHookCommand: String {
        #"umask 077; if [ -n "$SWARM_UI_CLI_STATUS_FILE" ]; then mkdir -p "$(dirname "$SWARM_UI_CLI_STATUS_FILE")" 2>/dev/null && swarm_status_tmp=$(mktemp "$SWARM_UI_CLI_STATUS_FILE.XXXXXX") && { cat > "$swarm_status_tmp" && mv -f "$swarm_status_tmp" "$SWARM_UI_CLI_STATUS_FILE"; } 2>/dev/null; fi; exit 0"#
    }

    /// The permission hook keeps the request open while the app answers through one file. Claude Code
    /// and Codex share the event, the payload and the answer shape, so both run this command.
    /// It is a constant command because hook trust is attached to the command text.
    static var interactivePermissionHookCommand: String {
        #"umask 077; if [ -n "$SWARM_UI_CLI_STATUS_FILE" ]; then swarm_status_dir=$(dirname "$SWARM_UI_CLI_STATUS_FILE"); swarm_permission_dir="$swarm_status_dir/permission"; mkdir -p "$swarm_permission_dir" 2>/dev/null; swarm_token=$(uuidgen 2>/dev/null); swarm_status_tmp=$(mktemp "$SWARM_UI_CLI_STATUS_FILE.XXXXXX" 2>/dev/null); swarm_payload_tmp=$(mktemp "$SWARM_UI_CLI_STATUS_FILE.XXXXXX" 2>/dev/null); if [ -n "$swarm_token" ] && [ -n "$swarm_status_tmp" ] && [ -n "$swarm_payload_tmp" ]; then cat > "$swarm_payload_tmp"; swarm_pending="$swarm_permission_dir/$swarm_token.pending"; swarm_answer="$swarm_permission_dir/$swarm_token.answer"; : > "$swarm_pending" 2>/dev/null; { printf '{"token":"%s","payload":' "$swarm_token"; cat "$swarm_payload_tmp"; printf '}\n'; } > "$swarm_status_tmp" 2>/dev/null; rm -f "$swarm_payload_tmp"; if mv -f "$swarm_status_tmp" "$SWARM_UI_CLI_STATUS_FILE" 2>/dev/null; then swarm_waited=0; while [ "$swarm_waited" -lt 120 ] && [ ! -f "$swarm_answer" ]; do sleep 1; swarm_waited=$((swarm_waited + 1)); done; if [ -f "$swarm_answer" ]; then cat "$swarm_answer"; rm -f "$swarm_answer" "$swarm_pending"; else rm -f "$swarm_pending"; fi; else rm -f "$swarm_pending" "$swarm_status_tmp"; fi; else cat >/dev/null; rm -f "$swarm_status_tmp" "$swarm_payload_tmp"; fi; fi; exit 0"#
    }

    private static func interactiveHookObject(data: Data) -> [String: Any]? {
        guard data.count <= 1_048_576 else { return nil }
        guard let object = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any] else { return nil }
        return object["payload"] as? [String: Any] ?? object
    }

    private static func interactiveTOMLString(_ value: String) -> String {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.withoutEscapingSlashes]
        guard let data = try? encoder.encode(value), let string = String(data: data, encoding: .utf8) else {
            return "\"\""
        }
        return string
    }
}

public enum InteractivePermissionAnswer: Sendable, Hashable {
    case allow
    case deny
    case terminal

    var data: Data {
        switch self {
        case .allow:
            Data(#"{"hookSpecificOutput":{"hookEventName":"PermissionRequest","decision":{"behavior":"allow"}}}"#.utf8)
        case .deny:
            Data(#"{"hookSpecificOutput":{"hookEventName":"PermissionRequest","decision":{"behavior":"deny","message":"Denied in Swarm"}}}"#.utf8)
        case .terminal:
            Data()
        }
    }
}

/// One pending Claude permission request, tied to the marker created by its hook invocation.
public struct InteractivePermissionCard: Sendable, Hashable, Identifiable {
    private let token: String
    private let toolName: String
    private let input: JSONValue
    private let statusURL: URL

    public var id: String { token }
    public var ask: PermissionAsk {
        PermissionAsk(requestID: token, toolName: toolName, input: input)
    }

    /// Creates a card only while the matching marker exists and no answer has been written.
    public init?(data: Data, statusURL: URL) {
        guard data.count <= 1_048_576,
              let envelope = JSONValue.parse(data),
              let token = envelope["token"]?.stringValue,
              UUID(uuidString: token) != nil,
              let payload = envelope["payload"],
              payload["hook_event_name"]?.stringValue == "PermissionRequest",
              let toolName = payload["tool_name"]?.stringValue,
              !toolName.isEmpty else { return nil }
        self.token = token
        self.toolName = toolName
        self.input = payload["tool_input"] ?? .object([:])
        self.statusURL = statusURL
        guard isPending else { return nil }
    }

    public var isPending: Bool {
        FileManager.default.fileExists(atPath: markerURL.path)
            && !FileManager.default.fileExists(atPath: answerURL.path)
    }

    /// Writes the answer atomically, or returns false when the hook stopped waiting before the click.
    @discardableResult
    public func answer(_ answer: InteractivePermissionAnswer) throws -> Bool {
        guard isPending else { return false }
        try answer.data.write(to: answerURL, options: .atomic)
        guard FileManager.default.fileExists(atPath: markerURL.path) else {
            try? FileManager.default.removeItem(at: answerURL)
            return false
        }
        return true
    }

    private var permissionDirectory: URL {
        statusURL.deletingLastPathComponent().appendingPathComponent("permission", isDirectory: true)
    }

    private var markerURL: URL {
        permissionDirectory.appendingPathComponent(token + ".pending")
    }

    private var answerURL: URL {
        permissionDirectory.appendingPathComponent(token + ".answer")
    }
}
