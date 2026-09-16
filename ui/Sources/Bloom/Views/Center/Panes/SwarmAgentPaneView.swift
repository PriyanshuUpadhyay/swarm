import SwiftUI
import BloomCore

/// One swarm agent's live pane: a terminal running `swarm attach` for that agent.
///
/// The agent tab embeds this beside the agent's chat (ADR 0003 in the swarm repository). Its
/// initialiser is fixed, so the tab and the terminal can be built apart; until the terminal is
/// in, it says what it will show.
struct SwarmAgentPaneView: View {
    let agent: SwarmAgentID
    let session: SwarmSessionID
    /// `SwarmAgent.alive`: nil when the agent has no pane or swarm could not tell.
    let isAlive: Bool?

    var body: some View {
        ContentUnavailableView(
            "Live pane",
            systemImage: "terminal",
            description: Text("The terminal for \(agent.rawValue) appears here.")
        )
    }
}
