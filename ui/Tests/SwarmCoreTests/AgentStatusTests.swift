import Foundation
import Testing
@testable import SwarmCore

@Suite("Agent status")
struct AgentStatusTests {
    @Test("A dead agent is ended; a live one shows its state, or done without one")
    func mapping() {
        #expect(AgentStatus(alive: false, state: "working") == .ended)
        #expect(AgentStatus(alive: true, state: nil) == .done)
        #expect(AgentStatus(alive: nil, state: nil) == .done)
        #expect(AgentStatus(alive: true, state: "working") == .working)
        #expect(AgentStatus(alive: true, state: "waiting") == .waiting)
        #expect(AgentStatus(alive: true, state: "done") == .done)
        #expect(AgentStatus(alive: true, state: "failed") == .failed)
        #expect(AgentStatus(alive: true, state: "compacting") == .done)
    }

    @Test("The most urgent status wins: waiting, failed, working, done, ended")
    func aggregate() {
        #expect(AgentStatus.aggregate([]) == nil)
        #expect(AgentStatus.aggregate([.ended, .done]) == .done)
        #expect(AgentStatus.aggregate([.done, .working, .ended]) == .working)
        #expect(AgentStatus.aggregate([.working, .failed]) == .failed)
        #expect(AgentStatus.aggregate([.failed, .waiting, .working]) == .waiting)
    }

    @Test("State fields decode from the CLI, and an unknown state still decodes")
    func decoding() throws {
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        let reviewer = try decoder.decode(SwarmAgent.self, from: Data(#"""
            {"id":"reviewer","role":"review","pane":"%3","alive":true,"state":"waiting",
             "state_at_s":1790000000,"state_source":"hook","state_detail":"permission prompt"}
            """#.utf8))
        #expect(reviewer.status == .waiting)
        #expect(reviewer.stateAtS == 1_790_000_000)
        #expect(reviewer.stateSource == "hook")
        #expect(reviewer.stateDetail == "permission prompt")
        let future = try decoder.decode(SwarmAgent.self, from: Data(
            #"{"id":"coder","role":"code","pane":"%2","alive":true,"state":"thinking-hard"}"#.utf8
        ))
        #expect(future.status == .done)
        let older = try decoder.decode(SwarmAgent.self, from: Data(
            #"{"id":"coder","role":"code","pane":null,"alive":false}"#.utf8
        ))
        #expect(older.state == nil)
        #expect(older.status == .ended)
        // swarm close clears the pane and the state; the agent shows as ended, not done.
        let closed = SwarmAgent(id: .init("coder"), role: "code", pane: nil, alive: nil)
        #expect(closed.status == .ended)
    }
}
