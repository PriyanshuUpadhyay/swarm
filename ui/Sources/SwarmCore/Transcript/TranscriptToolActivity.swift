import TranscriptTool

/// One tool call and the results that can be linked to it in the visible transcript.
public struct TranscriptToolActivity: Sendable, Hashable {
    public enum State: Sendable, Hashable {
        case waiting, finished, failed, interrupted, unreported
    }

    public var name: String
    public var input: JSONElement
    public var output: String?
    public var diffs: [TranscriptDiff]
    public var state: State
    public var command: String?
    public var path: String?

    public init(
        name: String, input: JSONElement, output: String? = nil,
        diffs: [TranscriptDiff] = [], state: State,
        command: String? = nil, path: String? = nil
    ) {
        self.name = name
        self.input = input
        self.output = output
        self.diffs = diffs
        self.state = state
        self.command = command
        self.path = path
    }

    static func command(in input: JSONElement, name: String) -> String? {
        if case .object(let fields) = input {
            for key in ["command", "cmd", "CommandLine"] {
                if case .string(let value) = fields[key] { return value }
            }
        }
        if case .string(let value) = input,
           name.lowercased().contains("exec") || name.lowercased().contains("shell") {
            return value
        }
        return nil
    }

    static func path(in input: JSONElement) -> String? {
        guard case .object(let fields) = input else { return nil }
        for key in ["file_path", "TargetFile", "AbsolutePath"] {
            if case .string(let value) = fields[key] { return value }
        }
        return nil
    }
}
