import Testing
@testable import Swarm
import SwarmCore

@Suite("Swarm session pane places")
struct SwarmSessionPaneStripTests {
    private let chair = SwarmAgentID("orchestrator")
    private let coder = SwarmAgent(id: SwarmAgentID("coder"), role: "coder", pane: "%1", alive: true)
    private let reviewer = SwarmAgent(id: SwarmAgentID("reviewer"), role: "reviewer", pane: "%2", alive: true)

    @Test func onePlacePerAgent() {
        let places = [
            SwarmPaneStripLayout.chairPlace(adapter: "tmux", attachable: true, chairID: chair),
            SwarmPaneStripLayout.place(for: coder, adapter: "tmux", attachable: true),
            SwarmPaneStripLayout.place(for: reviewer, adapter: "tmux", attachable: true),
        ]
        #expect(Set(places.map(\.id)) == Set([chair.rawValue, coder.id.rawValue, reviewer.id.rawValue]))
    }

    @Test func drawsOnePlacePerAgent() {
        let chairPlace = SwarmPaneStripLayout.chairPlace(adapter: "tmux", attachable: true, chairID: chair)
        let coderPlace = SwarmPaneStripLayout.place(for: coder, adapter: "tmux", attachable: true)
        let reviewerPlace = SwarmPaneStripLayout.place(for: reviewer, adapter: "tmux", attachable: true)
        #expect(chairPlace.isChair && chairPlace.content == .terminal(agent: chair))
        #expect(coderPlace.agentID == coder.id && coderPlace.content == .terminal(agent: coder.id))
        #expect(reviewerPlace.agentID == reviewer.id && reviewerPlace.content == .terminal(agent: reviewer.id))
    }

    @Test func herdrDrawsReasonNotGap() {
        let chairPlace = SwarmPaneStripLayout.chairPlace(adapter: "herdr", attachable: false, chairID: chair)
        let coderPlace = SwarmPaneStripLayout.place(for: coder, adapter: "herdr", attachable: false)
        #expect(chairPlace.reasonText == "This session runs in Herdr, which cannot attach a pane")
        #expect(coderPlace.reasonText == "This session runs in Herdr, which cannot attach a pane")
    }

    @Test func deadSeatStatesIt() {
        let ended = SwarmAgent(id: SwarmAgentID("ended"), role: "worker", pane: "%3", alive: false)
        let place = SwarmPaneStripLayout.place(for: ended, adapter: "tmux", attachable: true)
        #expect(place.reasonText == "This agent has ended")
    }

    @Test func startingSeat() {
        let starting = SwarmAgent(id: SwarmAgentID("starting"), role: "worker", pane: nil, alive: nil)
        let place = SwarmPaneStripLayout.place(for: starting, adapter: "tmux", attachable: true)
        #expect(place.reasonText == "Starting")
    }
}
