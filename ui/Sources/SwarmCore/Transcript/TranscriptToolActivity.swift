import Foundation
import TranscriptTool

/// One tool call and the results that can be linked to it in the visible transcript.
public struct TranscriptToolActivity: Sendable, Hashable {
    public enum State: Sendable, Hashable {
        case waiting, finished, failed, interrupted, unreported
    }

    public var name: String
    public var input: JSONElement
    /// Setting the output or the command also sets `exitCode`, so a card's body never scans the output.
    public var output: String? { didSet { exitCode = Self.exitCode(output: output, command: command) } }
    /// Setting the diffs also sets `diffCounts`, so a card's body never sums the lines.
    public var diffs: [TranscriptDiff] { didSet { diffCounts = Self.counts(of: diffs) } }
    /// Added and removed lines across the call's diffs; nil when it has none.
    public private(set) var diffCounts: DiffCounts?
    public var state: State
    public var command: String? { didSet { exitCode = Self.exitCode(output: output, command: command) } }
    /// A command's exit status from its result; nil when unknown. See `exitCode(output:command:)`.
    public private(set) var exitCode: Int?
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
        diffCounts = Self.counts(of: diffs)
        self.state = state
        self.command = command
        exitCode = Self.exitCode(output: output, command: command)
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
            let line = command.drop(while: \.isWhitespace).prefix(200).prefix(while: { !$0.isNewline })
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

    public struct DiffCounts: Sendable, Hashable {
        public var added: Int
        public var removed: Int
    }

    /// O(total diff lines); runs when the row builder sets the diffs, not in a view's body.
    private static func counts(of diffs: [TranscriptDiff]) -> DiffCounts? {
        guard !diffs.isEmpty else { return nil }
        var counts = DiffCounts(added: 0, removed: 0)
        for line in diffs.lazy.flatMap(\.hunks).flatMap(\.lines) {
            if line.hasPrefix("+") { counts.added += 1 } else if line.hasPrefix("-") { counts.removed += 1 }
        }
        return counts
    }

    /// The result says the step failed: a non-zero exit code, or a Codex code-mode command's
    /// `Script failed` header. Another tool's text can start with those words, so only a command
    /// in the `Script failed` … `Output:` shape counts.
    public var reportsFailure: Bool {
        if (exitCode ?? 0) != 0 { return true }
        guard command != nil, let output, output.hasPrefix("Script failed\n") else { return false }
        return Self.codeModeOutput(output) != nil
    }

    /// Claude Code writes `Exit code N` on the first line of a failed command's result. Codex
    /// exec_command without code mode writes a `Chunk ID:` header with `Process exited with code N`
    /// before `Output:`. Codex code mode writes `Script completed`, its wall time, `Output:`, and
    /// then each exec_command's JSON result; the first `"exit_code":N` other than 0 there is the
    /// code. A key inside an escaped string reads `\"exit_code\":`, so it does not match. Codex
    /// codes of 0 give nil, so a card shows no "exit 0". O(output length).
    static func exitCode(output: String?, command: String?) -> Int? {
        guard command != nil, let output else { return nil }
        if output.hasPrefix("Exit code ") {
            return Int(output.dropFirst("Exit code ".count).prefix(while: { !$0.isNewline }))
        }
        if output.hasPrefix("Chunk ID: ") {
            let header = output.range(of: "\nOutput:\n").map { output[..<$0.lowerBound] } ?? output[...]
            guard let key = header.range(of: "\nProcess exited with code ") else { return nil }
            let code = Int(header[key.upperBound...].prefix { !$0.isNewline })
            return code == 0 ? nil : code
        }
        guard var rest = codeModeOutput(output) else { return nil }
        while let key = rest.range(of: "\"exit_code\":") {
            rest = rest[key.upperBound...]
            if let code = Int(rest.prefix { $0 == "-" || $0.isASCII && $0.isNumber }), code != 0 { return code }
        }
        return nil
    }

    /// The text after `Output:` of a Codex code-mode result, whose header is `Script completed`,
    /// `Script failed`, or `Script running with cell ID N`; nil for any other text.
    static func codeModeOutput(_ output: String) -> Substring? {
        let headers = ["Script completed\n", "Script failed\n", "Script running with cell ID "]
        guard headers.contains(where: output.hasPrefix), let start = output.range(of: "\nOutput:\n") else { return nil }
        return output[start.upperBound...]
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
            return execCommands(inScript: value) ?? value
        }
        return nil
    }

    /// The `cmd` string of each `exec_command({cmd:"…"})` or `exec_command({"cmd":"…"})` in a Codex
    /// code-mode script, decoded as a JSON string and joined by newlines; nil when it has none.
    static func execCommands(inScript script: String) -> String? {
        var commands: [String] = []
        var rest = script[...]
        while let call = rest.range(of: "exec_command({") {
            rest = rest[call.upperBound...]
            let head = rest.drop(while: \.isWhitespace)
            let key = ["cmd:", "\"cmd\":"].first { head.hasPrefix($0) }
            guard let key else { continue }
            let value = head.dropFirst(key.count).drop(while: \.isWhitespace)
            guard value.first == "\"", let literal = stringLiteral(at: value) else { continue }
            commands.append(literal)
        }
        return commands.isEmpty ? nil : commands.joined(separator: "\n")
    }

    /// The double-quoted literal that `text` starts with, decoded; nil when it does not close.
    private static func stringLiteral(at text: Substring) -> String? {
        var escaped = false
        for index in text.indices.dropFirst() {
            if escaped {
                escaped = false
            } else if text[index] == "\\" {
                escaped = true
            } else if text[index] == "\"" {
                let literal = text[...index]
                // A JavaScript-only escape such as \' is not JSON; keep the source text then.
                return (try? JSONDecoder().decode(String.self, from: Data(literal.utf8)))
                    ?? String(literal.dropFirst().dropLast())
            }
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
