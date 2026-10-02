import Testing
import TranscriptTool
@testable import SwarmCore

@Suite("Composer queue")
struct ComposerQueueTests {
    private static let ADD_TEST = "add a test for the empty case"
    private static let KEEP_NAME = "keep the old name"
    private static let TASK_NOTIFICATION = "<task-notification>\n<task-id>t1</task-id>"
    private static let AGENT_MESSAGE = "<agent-message from=\"coder\">done</agent-message>"

    private func op(_ operation: String, _ content: String? = nil) -> TranscriptRecord {
        TranscriptRecord(
            event: .queueOperation(operation: operation, content: content, reason: nil, meta: Meta()),
            rawLine: ""
        )
    }

    @Test("A removed message leaves the queue and the next one stays")
    func removeDropsFirstEqual() {
        let records = [op("enqueue", Self.ADD_TEST), op("enqueue", Self.KEEP_NAME), op("remove", Self.ADD_TEST)]
        #expect(QueuedMessages.pending(in: records) == [Self.KEEP_NAME])
    }

    @Test("A dequeue takes the head")
    func dequeueTakesHead() {
        #expect(QueuedMessages.pending(in: [op("enqueue", Self.ADD_TEST), op("dequeue")]).isEmpty)
    }

    @Test("One popAll per message empties the queue")
    func popAllEmpties() {
        let records = [
            op("enqueue", Self.ADD_TEST), op("enqueue", Self.KEEP_NAME),
            op("popAll", Self.ADD_TEST), op("popAll", Self.KEEP_NAME),
        ]
        #expect(QueuedMessages.pending(in: records).isEmpty)
    }

    @Test("CLI-made entries are never pending, and a dequeue of one keeps the message behind it")
    func cliEntriesHidden() {
        #expect(QueuedMessages.pending(in: [op("enqueue", Self.TASK_NOTIFICATION), op("enqueue", Self.AGENT_MESSAGE)]).isEmpty)
        let records = [op("enqueue", Self.TASK_NOTIFICATION), op("enqueue", Self.ADD_TEST), op("dequeue")]
        #expect(QueuedMessages.pending(in: records) == [Self.ADD_TEST])
    }

    @Test("An unknown operation and other events are ignored")
    func unknownOperationIgnored() {
        let records = [
            op("enqueue", Self.ADD_TEST), op("reorder", Self.ADD_TEST),
            TranscriptRecord(event: .userMessageChunk(text: Self.ADD_TEST, meta: Meta()), rawLine: ""),
        ]
        #expect(QueuedMessages.pending(in: records) == [Self.ADD_TEST])
    }

    @Test("Up pulls back each queued message that gets its own popAll, in log order")
    func pullBackPullsEach() {
        let queue = [Self.ADD_TEST, Self.KEEP_NAME]
        #expect(QueuePullBack.decide(
            queue: queue, after: [op("popAll", Self.ADD_TEST), op("popAll", Self.KEEP_NAME)],
            pastDeadline: false
        ) == .pulled(queue))
        #expect(QueuePullBack.decide(
            queue: queue, after: [op("popAll", Self.ADD_TEST), op("remove", Self.KEEP_NAME)],
            pastDeadline: false
        ) == .pulled([Self.ADD_TEST]))
    }

    @Test("A popAll for a message the queue did not list yet is pulled too")
    func pullBackKeepsUnlistedText() {
        #expect(QueuePullBack.decide(
            queue: [Self.ADD_TEST], after: [op("popAll", Self.ADD_TEST), op("popAll", Self.KEEP_NAME)],
            pastDeadline: false
        ) == .pulled([Self.ADD_TEST, Self.KEEP_NAME]))
    }

    @Test("A popAll of a CLI-made entry never goes into the pulled text")
    func pullBackSkipsCLIEntries() {
        #expect(QueuePullBack.decide(
            queue: [Self.TASK_NOTIFICATION, Self.ADD_TEST],
            after: [op("popAll", Self.TASK_NOTIFICATION), op("popAll", Self.ADD_TEST)],
            pastDeadline: false
        ) == .pulled([Self.ADD_TEST]))
    }

    @Test("A dequeue of a CLI-made head entry is not credited to the owner's message")
    func pullBackDequeueOfCLIEntry() {
        let queue = [Self.TASK_NOTIFICATION, Self.ADD_TEST]
        #expect(QueuePullBack.decide(queue: queue, after: [op("dequeue")], pastDeadline: false) == .waiting)
        #expect(QueuePullBack.decide(
            queue: queue, after: [op("dequeue"), op("popAll", Self.ADD_TEST)], pastDeadline: false
        ) == .pulled([Self.ADD_TEST]))
    }

    @Test("A message the CLI took first is already sent, and no record by the deadline is unconfirmed")
    func pullBackAlreadySent() {
        #expect(QueuePullBack.decide(
            queue: [Self.ADD_TEST], after: [op("remove", Self.ADD_TEST)], pastDeadline: false
        ) == .alreadySent)
        #expect(QueuePullBack.decide(
            queue: [Self.ADD_TEST], after: [op("dequeue")], pastDeadline: false
        ) == .alreadySent)
        #expect(QueuePullBack.decide(queue: [Self.ADD_TEST], after: [], pastDeadline: false) == .waiting)
        #expect(QueuePullBack.decide(queue: [Self.ADD_TEST], after: [], pastDeadline: true) == .unconfirmed)
        // A CLI-made entry's record decides nothing for the owner's message.
        #expect(QueuePullBack.decide(
            queue: [Self.ADD_TEST], after: [op("remove", Self.TASK_NOTIFICATION)], pastDeadline: false
        ) == .waiting)
    }

    @Test("Pull-back is offered only for Claude in a session whose adapter can press keys")
    func pullBackNeedsKeyVerb() {
        #expect(SwarmSessionInteraction.canPullBack(provider: "claude", adapter: "tmux-solo"))
        #expect(SwarmSessionInteraction.canPullBack(provider: "claude", adapter: "tmux"))
        #expect(!SwarmSessionInteraction.canPullBack(provider: "claude", adapter: "herdr"))
        #expect(!SwarmSessionInteraction.canPullBack(provider: "claude", adapter: nil))
        #expect(!SwarmSessionInteraction.canPullBack(provider: "codex", adapter: "tmux-solo"))
    }

    @Test("C-u presses cover each line of the CLI box twice, plus one")
    func clearPresses() {
        #expect(QueuePullBack.clearPresses(for: Self.ADD_TEST) == 3)
        #expect(QueuePullBack.clearPresses(for: "a\nb") == 5)
    }

    @Test("Claude rows come from the queue and keep their ids while the head leaves")
    func claudeRows() {
        let before = ComposerQueuedRow.queued([Self.ADD_TEST, Self.KEEP_NAME])
        let after = ComposerQueuedRow.queued([Self.KEEP_NAME])
        #expect(before.map(\.state) == [.queued, .queued])
        #expect(before.map(\.text) == [Self.ADD_TEST, Self.KEEP_NAME])
        #expect(after.first?.id == before.last?.id)
        #expect(Set(ComposerQueuedRow.queued([Self.ADD_TEST, Self.ADD_TEST]).map(\.id)).count == 2)
    }

    private static let EARLIER = [
        TranscriptRow(kind: .user, text: KEEP_NAME, eventID: "u1"),
        TranscriptRow(kind: .assistant, text: "Working on it", eventID: "a1"),
    ]

    @Test("A Codex send while running stays sent until its user row shows, then leaves")
    func codexSentRow() {
        var sent = ComposerSentMessages()
        sent.record(Self.ADD_TEST, provider: "codex", isRunning: true, transcript: Self.EARLIER)
        sent.confirm(by: Self.EARLIER)
        let first = sent.rows
        sent.confirm(by: Self.EARLIER)
        #expect(first.map(\.state) == [.sent])
        #expect(first.map(\.text) == [Self.ADD_TEST])
        #expect(sent.rows.map(\.id) == first.map(\.id))

        sent.confirm(by: Self.EARLIER + [TranscriptRow(kind: .user, text: Self.ADD_TEST, eventID: "u2")])
        #expect(sent.rows.isEmpty)
    }

    @Test("A user row with equal text from before the send does not confirm it")
    func earlierRowDoesNotConfirm() {
        var sent = ComposerSentMessages()
        sent.record(Self.KEEP_NAME, provider: "agy", isRunning: true, transcript: Self.EARLIER)
        sent.confirm(by: Self.EARLIER)
        #expect(sent.rows.map(\.text) == [Self.KEEP_NAME])
    }

    @Test("Each row's caption and spoken label follow its provider and state")
    func captions() {
        let queued = ComposerQueuedRow(id: "q", text: Self.ADD_TEST, state: .queued)
        let sent = ComposerQueuedRow(id: "s", text: Self.ADD_TEST, state: .sent)
        #expect(queued.caption(provider: "claude") == "Queued")
        #expect(sent.caption(provider: "codex") == "Sent · joins after the next tool call")
        #expect(sent.caption(provider: "agy") == "Sent")
        #expect(queued.accessibilityLabel == "Queued message: \(Self.ADD_TEST)")
    }

    @Test("Sent rows stay while the agent runs and all leave when it stops, even if no user row matched")
    func sentRowsLeaveWhenStopped() {
        var sent = ComposerSentMessages()
        sent.record(Self.ADD_TEST, provider: "codex", isRunning: true, transcript: Self.EARLIER)
        sent.record(Self.KEEP_NAME, provider: "agy", isRunning: true, transcript: Self.EARLIER)
        sent.update(isRunning: true)
        #expect(sent.rows.count == 2)
        sent.update(isRunning: false)
        #expect(sent.rows.isEmpty)
    }

    @Test("Sent rows stay while a child waits on a question mid-turn, and leave when its turn ends")
    func sentRowsStayWhileWaiting() {
        var sent = ComposerSentMessages()
        sent.record(Self.ADD_TEST, provider: "codex", isRunning: true, transcript: Self.EARLIER)
        sent.update(isRunning: AgentStatus.waiting.isMidTurn)
        #expect(sent.rows.map(\.text) == [Self.ADD_TEST])
        #expect([AgentStatus.done, .failed, .ended].allSatisfy { !$0.isMidTurn })
        sent.update(isRunning: AgentStatus.done.isMidTurn)
        #expect(sent.rows.isEmpty)
    }

    @Test("A send made while a child waits on a question stays until its user row or the end of the turn")
    func sendWhileWaitingIsHeld() {
        var confirmed = ComposerSentMessages()
        confirmed.record(
            Self.ADD_TEST, provider: "codex", isRunning: AgentStatus.waiting.isMidTurn, transcript: Self.EARLIER
        )
        confirmed.update(isRunning: AgentStatus.waiting.isMidTurn)
        #expect(confirmed.rows.map(\.text) == [Self.ADD_TEST])
        confirmed.confirm(by: Self.EARLIER + [TranscriptRow(kind: .user, text: Self.ADD_TEST, eventID: "u2")])
        #expect(confirmed.rows.isEmpty)

        var turnEnded = ComposerSentMessages()
        turnEnded.record(
            Self.KEEP_NAME, provider: "agy", isRunning: AgentStatus.waiting.isMidTurn, transcript: Self.EARLIER
        )
        #expect(turnEnded.rows.map(\.text) == [Self.KEEP_NAME])
        turnEnded.update(isRunning: AgentStatus.done.isMidTurn)
        #expect(turnEnded.rows.isEmpty)
    }

    @Test("Only running Codex and AGY sends are held")
    func onlyRunningCodexAndAGY() {
        var sent = ComposerSentMessages()
        sent.record(Self.ADD_TEST, provider: "claude", isRunning: true, transcript: [])
        sent.record(Self.ADD_TEST, provider: "codex", isRunning: false, transcript: [])
        #expect(sent.rows.isEmpty)
    }
}
