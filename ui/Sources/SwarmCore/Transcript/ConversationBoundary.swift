import TranscriptTool

public enum ConversationBoundary {
    /// True when the first 30 records hold a hook result named "SessionStart:clear" or a command
    /// record for `/clear`, which is how Claude starts the log that `/clear` opens.
    public static func isClear(_ records: [TranscriptRecord]) -> Bool {
        records.prefix(30).contains { isClearMarker($0.event) }
    }

    static func isClearMarker(_ event: TranscriptEvent) -> Bool {
        switch event {
        case .hookResult(_, _, let name, _, _, _): name == "SessionStart:clear"
        case .systemMessage(let kind, let text, _):
            kind == "command" && text.contains("<command-name>/clear</command-name>")
        default: false
        }
    }
}
