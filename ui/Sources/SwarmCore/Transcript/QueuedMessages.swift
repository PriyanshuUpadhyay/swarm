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
