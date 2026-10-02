import TranscriptTool

/// Replays Claude's `queue-operation` records into the messages its queue still holds.
public enum QueuedMessages {
    /// The owner's messages in the queue. Content starting with "<task-notification" or
    /// "<agent-message" is never pending.
    public static func pending(in records: some Sequence<TranscriptRecord>) -> [String] {
        replay(records).filter(isOwners)
    }

    /// The whole queue, CLI-made entries too, so a dequeue that takes one does not drop a message
    /// behind it.
    public static func replay(_ records: some Sequence<TranscriptRecord>) -> [String] {
        var queue: [(text: String, waits: Bool)] = []
        for record in records { apply(record, to: &queue) }
        return queue.map(\.text)
    }

    static func isOwners(_ content: String) -> Bool {
        !content.hasPrefix("<task-notification") && !content.hasPrefix("<agent-message")
    }

    /// enqueue appends; dequeue drops the head; remove and popAll drop the first equal content.
    /// An unknown operation is ignored (new values may appear).
    static func apply(_ record: TranscriptRecord, to queue: inout [(text: String, waits: Bool)]) {
        guard case .queueOperation(let operation, let content, _, _) = record.event else { return }
        switch operation {
        case "enqueue":
            if let content { queue.append((content, false)) }
        case "dequeue":
            if !queue.isEmpty { queue.removeFirst() }
        case "remove", "popAll":
            if let index = queue.firstIndex(where: { $0.text == content }) { queue.remove(at: index) }
        default:
            break
        }
    }
}

/// What Claude's queue records say after the app pressed Up to pull its queued messages back.
public enum QueuePullBack: Sendable, Equatable {
    /// `owner` goes into the draft; `popped` is every popped text in log order, CLI-made ones
    /// too, because the CLI puts them all into its box.
    case pulled(owner: [String], popped: [String])
    /// The CLI took every message first; Up then recalled the last sent prompt into its box.
    case alreadySent
    case waiting
    /// The deadline passed while an owner message had no record, so the CLI box is unknown.
    case unconfirmed

    /// `queue` is the whole replay when Up was sent, and `records` are the log records after it.
    /// Each owner message is decided by its own record against that replay: a `popAll` pulls it,
    /// and a `remove` or a `dequeue` that takes it from the head means the CLI took it. The pulled
    /// text is every owner `popAll` after Up in log order, also one for a message the replay missed.
    public static func decide(
        queue: [String], after records: some Sequence<TranscriptRecord>, pastDeadline: Bool
    ) -> QueuePullBack {
        var open = queue.map { (text: $0, waits: QueuedMessages.isOwners($0)) }
        var popped: [String] = []
        for record in records {
            QueuedMessages.apply(record, to: &open)
            if case .queueOperation("popAll", let content?, _, _) = record.event { popped.append(content) }
        }
        if open.contains(where: \.waits) { return pastDeadline ? .unconfirmed : .waiting }
        let owner = popped.filter(QueuedMessages.isOwners)
        return owner.isEmpty ? .alreadySent : .pulled(owner: owner, popped: popped)
    }

    /// `C-u` presses that empty the CLI box holding `texts`, one per line. One press empties a
    /// line and the next joins it to the line above; extra presses on an empty box do nothing.
    public static func clearPresses(for texts: [String]) -> Int {
        2 * texts.joined(separator: "\n").split(separator: "\n", omittingEmptySubsequences: false).count + 1
    }
}
