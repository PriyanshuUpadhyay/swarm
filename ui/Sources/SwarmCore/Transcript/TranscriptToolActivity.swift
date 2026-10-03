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
