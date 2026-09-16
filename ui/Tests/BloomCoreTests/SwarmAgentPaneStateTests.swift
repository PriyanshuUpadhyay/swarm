import Testing
@testable import BloomCore

@Suite("SwarmAgentPaneState")
struct SwarmAgentPaneStateTests {
    @Test("an agent with no pane starts nothing")
    func noLivePane() {
        let new = SwarmAgentPaneState(isAlive: nil, attachment: .notStarted)
        let running = SwarmAgentPaneState(isAlive: nil, attachment: .running)
        let exited = SwarmAgentPaneState(isAlive: nil, attachment: .exited)

        #expect(new == .noLivePane)
        #expect(running == .noLivePane)
        #expect(exited == .noLivePane)
    }

    @Test("an ended agent starts nothing")
    func agentEnded() {
        let new = SwarmAgentPaneState(isAlive: false, attachment: .notStarted)
        let running = SwarmAgentPaneState(isAlive: false, attachment: .running)
        let exited = SwarmAgentPaneState(isAlive: false, attachment: .exited)

        #expect(new == .agentEnded)
        #expect(running == .agentEnded)
        #expect(exited == .agentEnded)
    }

    @Test("a live agent starts its first attachment")
    func startsTerminal() {
        let state = SwarmAgentPaneState(isAlive: true, attachment: .notStarted)

        #expect(state == .startTerminal)
    }

    @Test("a running attachment stays visible")
    func terminal() {
        let state = SwarmAgentPaneState(isAlive: true, attachment: .running)

        #expect(state == .terminal)
    }

    @Test("an exited attachment offers reattach only while the agent is live")
    func reattach() {
        let live = SwarmAgentPaneState(isAlive: true, attachment: .exited)
        let ended = SwarmAgentPaneState(isAlive: false, attachment: .exited)

        #expect(live == .reattach)
        #expect(ended == .agentEnded)
    }
}
