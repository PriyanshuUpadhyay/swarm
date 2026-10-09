import Testing
@testable import SwarmCore

@Suite("Agent state tooltip")
struct AgentStateTextTests {
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
