import Foundation
import Testing
@testable import SwarmCore

@Suite("Swarm agent conversations", .scratchDirectory)
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

    @Test("archive cleanup notice names the session and every agent left open")
    func archiveCloseFailureNotice() {
        let failure = SwarmArchiveCloseFailure(
            session: SwarmSessionID("session-7"),
            agents: [reviewerAgent, coderAgent, reviewerAgent],
            listingFailed: false
        )

        let notice = failure.notice(after: "Docs was archived. It stopped a background command: Serve.")
        #expect(notice.message == "Docs was archived. It stopped a background command: Serve. "
            + "Swarm could not close swarm agents `code-complex-1`, `review-1` in session `session-7`. "
            + "Run `swarm close <name>` for each agent by hand.")
        #expect(notice.dismissal == .untilDismissed)
    }

    @Test("archive cleanup notice names known agents after a list failure")
    func archiveListFailureNotice() {
        let failure = SwarmArchiveCloseFailure(
            session: SwarmSessionID("session-8"), agents: [coderAgent], listingFailed: true
        )

        #expect(failure.notice(after: "Docs was archived.").message == "Docs was archived. "
            + "Swarm could not list or close swarm agent `code-complex-1` in session `session-8`. "
            + "Run `swarm close <name>` for each agent by hand.")

        let unknown = SwarmArchiveCloseFailure(
            session: SwarmSessionID("session-9"), agents: [], listingFailed: true
        )
        #expect(unknown.notice(after: "Docs was archived.").message == "Docs was archived. "
            + "Swarm could not list or close the agents in swarm session `session-9`. "
            + "Run `swarm agents`, then `swarm close <name>` by hand.")
    }

    @Test("polling backs off to the sweep interval")
    func backoff() {
        #expect(SwarmPollSchedule.delay(afterFailures: 0) == 2)
        #expect(SwarmPollSchedule.delay(afterFailures: 1) == 4)
        #expect(SwarmPollSchedule.delay(afterFailures: 8) == 30)
    }

    @Test("a chat keeps its swarm session id")
    func storedChatSession() async throws {
        let store = try makeTestStore("chat-swarm-session")
        let chat = SessionID("chat-1")
        let swarm = SwarmSessionID("42")

        try await SwarmChatSession.save(swarm, sessionID: chat, in: store)

        #expect(await SwarmChatSession.load(sessionID: chat, from: store) == swarm)
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
