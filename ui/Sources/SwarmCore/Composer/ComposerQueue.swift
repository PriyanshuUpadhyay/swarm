import Foundation

/// A message the agent has not taken yet, shown above the composer's text field.
public struct ComposerQueuedRow: Identifiable, Equatable, Sendable {
    /// `queued` is in the CLI's own queue; `sent` has no log signal yet.
    public enum State: Sendable, Equatable { case queued, sent }
    public var id: String
    public var text: String
    public var state: State

    public init(id: String, text: String, state: State) {
        self.id = id
        self.text = text
        self.state = state
    }

    /// The trailing tag. Codex adds a sent text to the running turn at its next tool call.
    public func caption(provider: String?) -> String {
        switch state {
        case .queued: "Queued"
        case .sent: provider == "codex" ? "Sent · joins after the next tool call" : "Sent"
        }
    }

    public var accessibilityLabel: String {
        "\(state == .queued ? "Queued" : "Sent") message: \(text)"
    }

    /// Claude's queue as rows. An id counts the equal texts ahead of it, so it stays the same
    /// while other messages leave the queue.
    public static func queued(_ pending: [String]) -> [ComposerQueuedRow] {
        var seen: [String: Int] = [:]
        return pending.map { text in
            let count = seen[text, default: 0]
            seen[text] = count + 1
            return ComposerQueuedRow(id: "queued-\(count)-\(text)", text: text, state: .queued)
        }
    }
}

/// Texts the app typed into a running Codex or AGY chat. Their logs say nothing until the agent
/// takes a message, so each text stays until a user row with equal text shows after the send.
public struct ComposerSentMessages: Sendable, Equatable {
    private struct Entry: Sendable, Equatable {
        var row: ComposerQueuedRow
        /// The last transcript row at the send. Only rows after it can confirm the text.
        var after: String?
    }

    private var entries: [Entry] = []

    public init() {}

    public var rows: [ComposerQueuedRow] { entries.map(\.row) }

    /// Holds the text only for a running Codex or AGY chat; Claude's log records its own queue.
    public mutating func record(
        _ text: String, provider: String?, isRunning: Bool, transcript: [TranscriptRow]
    ) {
        guard isRunning, provider == "codex" || provider == "agy" else { return }
        entries.append(Entry(
            row: ComposerQueuedRow(
                id: UUID().uuidString,
                text: text.trimmingCharacters(in: .whitespacesAndNewlines), state: .sent
            ),
            after: transcript.last?.eventID
        ))
    }

    /// Once the agent's turn ends, the CLI has taken or dropped every typed text, so all leave.
    /// This also ends a text the log records in another form than the one typed. A child that
    /// waits on a question mid-turn still holds its texts, so pass `AgentStatus.isMidTurn`.
    public mutating func update(isRunning: Bool) {
        if !isRunning { entries.removeAll() }
    }

    /// Drops each text that a user row after its send now shows. One row confirms one text.
    public mutating func confirm(by transcript: [TranscriptRow]) {
        var used = Set<Int>()
        entries.removeAll { entry in
            let start = entry.after
                .flatMap { id in transcript.lastIndex { $0.eventID == id } }
                .map { $0 + 1 } ?? 0
            let text = entry.row.text
            guard let match = transcript.indices[start...].first(where: {
                !used.contains($0) && transcript[$0].kind == .user
                    && transcript[$0].text.trimmingCharacters(in: .whitespacesAndNewlines) == text
            }) else { return false }
            used.insert(match)
            return true
        }
    }
}
