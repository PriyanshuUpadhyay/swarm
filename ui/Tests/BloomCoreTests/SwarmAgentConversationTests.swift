import Foundation
import Testing
@testable import BloomCore

@Suite("Swarm agent conversations")
struct SwarmAgentConversationTests {
    private let coderAgent = SwarmAgentID("code-complex-1")
    private let reviewerAgent = SwarmAgentID("review-1")
    private let orchestrator = SwarmAgentID("orchestrator")

    @Test("agent names use the role and first free number")
    func freeAgentName() {
        let name = SwarmAgentName.free(
            role: "code.complex",
            excluding: [coderAgent, SwarmAgentID("code-complex-2")]
        )
        #expect(name == SwarmAgentID("code-complex-3"))
        #expect(SwarmAgentName.isValid(name.rawValue))
        #expect(!SwarmAgentName.isValid("Code complex 3"))
        #expect(!SwarmAgentName.isValid(String(repeating: "a", count: 41)))
    }

    @Test("chat rows keep only the chair's exchange with one agent")
    func chatRows() {
        let messages = [
            message(1, from: orchestrator, to: coderAgent, kind: "ask", body: "Build it"),
            message(2, from: coderAgent, to: orchestrator, kind: "summary", body: "Done"),
            message(3, from: coderAgent, to: orchestrator, kind: "note", body: nil),
            message(4, from: reviewerAgent, to: orchestrator, kind: "summary", body: "Reviewed"),
        ]

        let rows = SwarmChatRow.rows(for: coderAgent, in: messages)
        #expect(rows.map(\.seq) == [1, 2, 3])
        #expect(rows.map(\.author) == [.you, .agent(coderAgent), .agent(coderAgent)])
        #expect(rows.map(\.kindLabel) == [nil, nil, "note"])
        #expect(rows.last?.body == nil)
    }

    @Test("only unread replies shown in their agent tab are acknowledged")
    func acknowledgements() {
        var readReply = message(4, from: coderAgent, to: orchestrator, kind: "summary", body: "Old")
        readReply.read = true
        let messages = [
            message(1, from: coderAgent, to: orchestrator, kind: "summary", body: "Done"),
            message(2, from: reviewerAgent, to: orchestrator, kind: "summary", body: "Reviewed"),
            message(3, from: orchestrator, to: coderAgent, kind: "ask", body: "Build it"),
            readReply,
        ]

        #expect(SwarmChatRow.acknowledgements(in: messages, shownAgents: [coderAgent]) == [1])
    }

    @Test("polling backs off to the sweep interval")
    func backoff() {
        #expect(SwarmPollSchedule.delay(afterFailures: 0) == 2)
        #expect(SwarmPollSchedule.delay(afterFailures: 1) == 4)
        #expect(SwarmPollSchedule.delay(afterFailures: 8) == 30)
    }

    @Test("sweeps run every thirty seconds only while a pane exists")
    func sweepSchedule() {
        let now = Date(timeIntervalSince1970: 100)
        #expect(SwarmPollSchedule.shouldSweep(last: nil, now: now, hasPane: true))
        #expect(!SwarmPollSchedule.shouldSweep(
            last: now.addingTimeInterval(-29), now: now, hasPane: true
        ))
        #expect(SwarmPollSchedule.shouldSweep(
            last: now.addingTimeInterval(-30), now: now, hasPane: true
        ))
        #expect(!SwarmPollSchedule.shouldSweep(last: nil, now: now, hasPane: false))
    }

    private func message(
        _ seq: Int, from sender: SwarmAgentID, to recipient: SwarmAgentID,
        kind: String, body: String?
    ) -> SwarmMessage {
        SwarmMessage(
            seq: seq, sender: sender, recipient: recipient, kind: kind,
            body: body, createdAt: 1_000 + seq, read: false
        )
    }
}
