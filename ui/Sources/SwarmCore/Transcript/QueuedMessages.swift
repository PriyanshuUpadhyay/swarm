import TranscriptTool

/// Replays Claude's `queue-operation` records into the messages its queue still holds.
public enum QueuedMessages {
    /// enqueue appends; dequeue drops the head; remove and popAll drop the first equal content.
    /// Content starting with "<task-notification" or "<agent-message" is never pending.
    /// An unknown operation is ignored (new values may appear).
    public static func pending(in records: some Sequence<TranscriptRecord>) -> [String] {
        var queue: [String] = []
        for record in records {
            guard case .queueOperation(let operation, let content, _, _) = record.event else { continue }
            switch operation {
            case "enqueue":
                if let content { queue.append(content) }
            case "dequeue":
                if !queue.isEmpty { queue.removeFirst() }
            case "remove", "popAll":
                if let content, let index = queue.firstIndex(of: content) { queue.remove(at: index) }
            default:
                break
            }
        }
        // CLI-made entries stay in the replay, so a dequeue that takes one does not drop a
        // message behind it.
        return queue.filter { !$0.hasPrefix("<task-notification") && !$0.hasPrefix("<agent-message") }
    }
}

/// What Claude's queue records say after the app pressed Up to pull its queued messages back.
public enum QueuePullBack: Sendable, Equatable {
    case pulled([String])
    /// The CLI took every message first; Up then recalled the last sent prompt into its box.
    case alreadySent
    case waiting

    /// `queued` is the queue when Up was sent, and `records` are the log records after it. Each
    /// message is decided by its own record: an equal `popAll` pulls it, an equal `remove` or a
    /// `dequeue` (which has no content and takes the head) means the CLI took it. Past the
    /// deadline, a message with no record counts as taken.
    public static func decide(
        queued: [String], after records: some Sequence<TranscriptRecord>, pastDeadline: Bool
    ) -> QueuePullBack {
        var open = Array(queued.indices)
        var pulled: [Int] = []
        for record in records {
            guard case .queueOperation(let operation, let content, _, _) = record.event else { continue }
            let match: Int? = switch operation {
            case "popAll", "remove": open.firstIndex { queued[$0] == content }
            case "dequeue": open.indices.first
            default: nil
            }
            guard let match else { continue }
            let message = open.remove(at: match)
            if operation == "popAll" { pulled.append(message) }
        }
        guard open.isEmpty || pastDeadline else { return .waiting }
        return pulled.isEmpty ? .alreadySent : .pulled(pulled.sorted().map { queued[$0] })
    }

    /// `C-u` presses that empty the CLI box holding `text`. One press empties a line and the next
    /// joins it to the line above; extra presses on an empty box do nothing.
    public static func clearPresses(for text: String) -> Int {
        2 * text.split(separator: "\n", omittingEmptySubsequences: false).count + 1
    }
}
