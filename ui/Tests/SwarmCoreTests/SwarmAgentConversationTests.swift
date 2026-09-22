import Foundation
import Testing
@testable import SwarmCore

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

    @Test("overlapping acknowledgement batches reserve each sequence once")
    func acknowledgementReservations() {
        let first = SwarmAckReservation.reserve([42, 43], inFlight: [])
        let overlapping = SwarmAckReservation.reserve([42, 43], inFlight: first.inFlight)

        #expect(first.sequences == [42, 43])
        #expect(overlapping.sequences.isEmpty)
    }

    @Test("a failed acknowledgement is released for a later retry")
    func acknowledgementRetry() {
        let first = SwarmAckReservation.reserve([42], inFlight: [])
        let released = SwarmAckReservation.release(first.sequences, inFlight: first.inFlight)
        let retry = SwarmAckReservation.reserve([42], inFlight: released)

        #expect(retry.sequences == [42])
    }

    @Test("dismissed errors wait for changed text or a successful call")
    func errorDisplay() {
        var display = SwarmErrorDisplay()
        display.record("swarm is unavailable")
        display.dismiss()
        display.record("swarm is unavailable")
        #expect(display.visible == nil)

        display.record("account is unavailable")
        #expect(display.visible == "account is unavailable")
        display.dismiss()
        display.succeed()
        display.record("account is unavailable")
        #expect(display.visible == "account is unavailable")
    }

    @Test("polling backs off to the sweep interval")
    func backoff() {
        #expect(SwarmPollSchedule.delay(afterFailures: 0) == 2)
        #expect(SwarmPollSchedule.delay(afterFailures: 1) == 4)
        #expect(SwarmPollSchedule.delay(afterFailures: 8) == 30)
    }

    @Test("live attachable workers without tabs are added once")
    func agentsToAdd() {
        let existingAgent = SwarmAgentID("existing")
        let endedAgent = SwarmAgentID("ended")
        let startingAgent = SwarmAgentID("starting")
        let agents = [
            SwarmAgent(id: orchestrator, role: "orchestrator", pane: "%0", alive: true),
            SwarmAgent(id: coderAgent, role: "code", pane: "%1", alive: true),
            SwarmAgent(id: existingAgent, role: "test", pane: "%2", alive: true),
            SwarmAgent(id: endedAgent, role: "review", pane: "%3", alive: false),
            SwarmAgent(id: startingAgent, role: "research", pane: nil, alive: nil),
            SwarmAgent(id: coderAgent, role: "code", pane: "%1", alive: true),
        ]

        let missing = SwarmAgentTabs.agentsToAdd(
            from: agents, attachable: true,
            chairID: orchestrator, existing: [existingAgent]
        )
        let unsupported = SwarmAgentTabs.agentsToAdd(
            from: agents, attachable: false,
            chairID: orchestrator, existing: [existingAgent]
        )
        let unknown = SwarmAgentTabs.agentsToAdd(
            from: agents, attachable: nil,
            chairID: orchestrator, existing: [existingAgent]
        )

        #expect(missing == [coderAgent])
        #expect(unsupported.isEmpty)
        #expect(unknown.isEmpty)
    }

    @Test("workers fill the right half in rows")
    func workerPlacement() {
        let placements = (0..<5).map(SwarmAgentTabs.placement)

        #expect(placements[0].anchor == .chat)
        #expect(placements[0].axis == .horizontal)
        #expect(placements[1].anchor == .worker(0))
        #expect(placements[1].axis == .vertical)
        #expect(placements[2].anchor == .worker(0))
        #expect(placements[2].axis == .horizontal)
        #expect(placements[3].anchor == .worker(1))
        #expect(placements[3].axis == .horizontal)
        #expect(placements[4].anchor == .worker(2))
        #expect(placements[4].axis == .horizontal)
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
