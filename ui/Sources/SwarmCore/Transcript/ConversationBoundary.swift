import Foundation
import TranscriptTool

public enum ConversationBoundary {
    static let clearHook = "SessionStart:clear"
    static let clearCommand = "<command-name>/clear</command-name>"
    static let commandCaveat = "<local-command-caveat>"
    static let emptyCommandOutput = "<local-command-stdout></local-command-stdout>"

    /// True when the first 30 records hold a hook result named "SessionStart:clear" or a command
    /// record for `/clear`, which is how Claude starts the log that `/clear` opens.
    public static func isClear(_ records: [TranscriptRecord]) -> Bool {
        records.prefix(30).contains { isClearMarker($0.event) }
    }

    static func isClearMarker(_ event: TranscriptEvent) -> Bool {
        switch event {
        case .hookResult(_, _, let name, _, _, _): name == clearHook
        case .systemMessage(let kind, let text, _): kind == "command" && text.contains(clearCommand)
        default: false
        }
    }

    /// A cleared log's rows without the `/clear` command, its caveat, and its empty output,
    /// which Claude writes before the first user or assistant row.
    static func withoutClearPreamble(_ rows: [TranscriptRow]) -> [TranscriptRow] {
        let start = rows.firstIndex { $0.kind == .user || $0.kind == .assistant } ?? rows.endIndex
        return rows[..<start].filter { !isClearPreamble($0) } + rows[start...]
    }

    private static func isClearPreamble(_ row: TranscriptRow) -> Bool {
        guard row.kind == .system else { return false }
        let text = row.text.trimmingCharacters(in: .whitespacesAndNewlines)
        return text.contains(clearCommand) || text.hasPrefix(commandCaveat) || text == emptyCommandOutput
    }
}
