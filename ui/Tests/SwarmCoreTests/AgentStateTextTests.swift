import Foundation
import Testing
@testable import SwarmCore

@Suite("Agent state tooltip")
struct AgentStateTextTests {
    @Test("A decoded wait kind follows the status in the tooltip", arguments: ["permission", "question"])
    func waitingKind(kind: String) throws {
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        let agent = try decoder.decode(SwarmAgent.self, from: Data("""
            {"id":"child","role":"build","pane":"%1","alive":true,"state":"waiting",
             "state_kind":"\(kind)","state_at_s":100,"state_source":"hook","state_detail":"Choose."}
            """.utf8))
        #expect(AgentStateText.tooltip(agent: agent, now: 110) == "waiting · \(kind) since 10s · hook\nChoose.")
    }

    @Test("Wait titles name the kind; other statuses keep their title")
    func titles() {
        #expect(AgentStateText.title(status: .waiting, kind: "permission") == "Waiting for permission")
        #expect(AgentStateText.title(status: .waiting, kind: "question") == "Asked a question")
        #expect(AgentStateText.title(status: .waiting, kind: "future-kind") == "Waiting for you")
        for (status, title) in [(AgentStatus.working, "Working"), (.waiting, "Waiting for you"),
                                (.done, "Done"), (.failed, "Failed"), (.ended, "Ended")] {
            #expect(AgentStateText.title(status: status, kind: nil) == title)
            if status != .waiting {
                #expect(AgentStateText.title(status: status, kind: "permission") == title)
                #expect(AgentStateText.title(status: status, kind: "question") == title)
            }
        }
    }

    @Test("The tooltip shows state age, source and detail")
    func reportedState() {
        var agent = SwarmAgent(id: .init("child"), role: "build", pane: "%1", alive: true, state: "waiting")
        agent.stateAtS = 100
        agent.stateSource = "screen"
        agent.stateDetail = "Choose a branch."
        #expect(AgentStateText.tooltip(agent: agent, now: 225) == "waiting since 2m · screen\nChoose a branch.")
        agent.stateSource = "hook"
        agent.stateDetail = nil
        #expect(AgentStateText.tooltip(agent: agent, now: 110) == "waiting since 10s · hook")
        agent.stateDetail = ""
        agent.stateAtS = nil
        #expect(AgentStateText.tooltip(agent: agent, now: 225) == "waiting · hook")
    }

    @Test("Missing source is omitted and ended agrees with the glyph")
    func missingAndEnded() {
        var agent = SwarmAgent(id: .init("child"), role: "build", pane: nil, alive: false, state: "working")
        #expect(AgentStateText.tooltip(agent: agent, now: 100) == "ended")
        agent.stateAtS = 101
        #expect(AgentStateText.tooltip(agent: agent, now: 100) == "ended since 0s")
        agent.stateAtS = 100
        #expect(AgentStateText.tooltip(agent: agent, now: 3_700) == "ended since 1h")
        #expect(AgentStateText.tooltip(agent: agent, now: 86_500) == "ended since 1d")
    }
}
