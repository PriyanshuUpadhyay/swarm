import Foundation
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
    /// Seconds from the call to its last result; nil when either time is missing.
    public var duration: Double?
    /// The body of the skill a Skill call loaded, linked by the body record's `sourceToolUseID`.
    public var skillBody: String?

    public init(
        name: String, input: JSONElement, output: String? = nil,
        diffs: [TranscriptDiff] = [], state: State,
        command: String? = nil, path: String? = nil, duration: Double? = nil
    ) {
        self.name = name
        self.input = input
        self.output = output
        self.diffs = diffs
        self.state = state
        self.command = command
        self.path = path
        self.duration = duration
    }

    /// The one-line title after the tool name; see `title(input:command:path:)`.
    public var headerTitle: String {
        Self.title(input: input, command: command, path: path)
    }

    /// The one-line title after the tool name: the call's description (or a Skill call's skill name),
    /// else the command's first line, else a search's `"pattern" in folder`, else the file name with
    /// the line range a Read asked for.
    static func title(input: JSONElement, command: String?, path: String?) -> String {
        let fields: [String: JSONElement] = if case .object(let value) = input { value } else { [:] }
        for key in ["description", "Description", "toolSummary", "skill"] {
            if case .string(let value) = fields[key] {
                let trimmed = value.trimmingCharacters(in: .whitespacesAndNewlines)
                if !trimmed.isEmpty { return trimmed }
            }
        }
        if let command {
            // Only the first non-blank line is read, so a long heredoc costs nothing.
            let line = command.drop(while: \.isWhitespace).prefix(while: { !$0.isNewline }).prefix(200)
            let trimmed = line.trimmingCharacters(in: .whitespaces)
            if !trimmed.isEmpty { return trimmed }
        }
        if case .string(let pattern) = fields["pattern"] {
            guard case .string(let folder) = fields["path"] else { return "\"\(pattern)\"" }
            return "\"\(pattern)\" in \((folder as NSString).lastPathComponent)"
        }
        guard let path else { return "" }
        let name = (path as NSString).lastPathComponent
        func number(_ key: String) -> Int64? {
            if case .integer(let value) = fields[key] { value } else { nil }
        }
        let offset = number("offset")
        let limit = number("limit")
        guard offset != nil || limit != nil else { return name }
        let first = offset ?? 1
        // Tool input is model output, so the range math must not trap on overflow.
        if let limit, limit > 0, case (let last, false) = first.addingReportingOverflow(limit - 1) {
            return "\(name) · lines \(first)–\(last)"
        }
        return "\(name) · from line \(first)"
    }

    /// Added and removed lines across the call's diffs; nil when it has none. O(total diff lines).
    public var diffCounts: (added: Int, removed: Int)? {
        guard !diffs.isEmpty else { return nil }
        let lines = diffs.flatMap(\.hunks).flatMap(\.lines)
        return (lines.count { $0.hasPrefix("+") }, lines.count { $0.hasPrefix("-") })
    }

    /// The exit status Claude Code writes on the first line of a failed command's result.
    public var exitCode: Int? {
        guard command != nil, let output, output.hasPrefix("Exit code ") else { return nil }
        return Int(output.dropFirst("Exit code ".count).prefix(while: { !$0.isNewline }))
    }

    /// Seconds between two event timestamps (ISO 8601, with or without fractional seconds).
    static func duration(from start: String, to end: String) -> Double? {
        func date(_ text: String) -> Date? {
            (try? Date(text, strategy: .iso8601))
                ?? (try? Date(text, strategy: Date.ISO8601FormatStyle(includingFractionalSeconds: true)))
        }
        guard let start = date(start), let end = date(end), end >= start else { return nil }
        return end.timeIntervalSince(start)
    }

    /// "0.4s", "12s", or "3m 5s".
    public static func durationLabel(_ seconds: Double) -> String {
        if seconds < 10 { return String(format: "%.1fs", seconds) }
        if seconds < 60 { return "\(Int(seconds.rounded()))s" }
        let whole = Int(seconds.rounded())
        return "\(whole / 60)m \(whole % 60)s"
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
