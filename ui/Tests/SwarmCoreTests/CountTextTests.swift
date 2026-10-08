import Testing
@testable import SwarmCore

@Suite("Count text")
struct CountTextTests {
    @Test("Zero, one and many tabs and agents have the right noun and verb")
    func plurals() {
        for (count, tabs, agents, running) in [
            (0, "0 tabs", "0 agents", "0 agents still run"),
            (1, "1 tab", "1 agent", "1 agent still runs"),
            (2, "2 tabs", "2 agents", "2 agents still run"),
        ] {
            #expect(CountText.count(count, singular: "tab", plural: "tabs") == tabs)
            #expect(CountText.count(count, singular: "agent", plural: "agents") == agents)
            #expect(CountText.agentsStillRunning(count) == running)
        }
    }
}
