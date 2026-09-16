import SwiftUI
import BloomCore

/// One swarm agent's live pane: a terminal running `swarm attach` for that agent.
///
/// The agent tab embeds this beside the agent's chat (ADR 0003 in the swarm repository).
struct SwarmAgentPaneView: View {
    let agent: SwarmAgentID
    let session: SwarmSessionID
    /// `SwarmAgent.alive`: nil when the agent has no pane or swarm could not tell.
    let isAlive: Bool?

    @Environment(AppModel.self) private var app
    @State private var terminals = TerminalSessionStore.shared

    var body: some View {
        let state = SwarmAgentPaneState(
            isAlive: isAlive,
            attachment: terminals.swarmAgentAttachment(for: agent, in: session)
        )

        switch state {
        case .noLivePane:
            ContentUnavailableView(
                "No live pane",
                systemImage: "terminal",
                description: Text("\(agent.rawValue) has no live pane.")
            )
        case .agentEnded:
            ContentUnavailableView(
                "Agent ended",
                systemImage: "checkmark.circle",
                description: Text("\(agent.rawValue) has ended.")
            )
        case .startTerminal:
            terminal(startsProcess: true)
        case .terminal:
            terminal(startsProcess: false)
        case .reattach:
            VStack(spacing: 0) {
                SwarmAgentPaneReattachStrip {
                    terminals.reattach(
                        agent: agent,
                        session: session,
                        command: app.swarmBus.attachCommand(for: agent, in: session)
                    )
                }
                terminal(startsProcess: false)
            }
        }
    }

    private func terminal(startsProcess: Bool) -> some View {
        SwarmAgentPaneTerminalView(
            agent: agent,
            session: session,
            command: app.swarmBus.attachCommand(for: agent, in: session),
            startsProcess: startsProcess
        )
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(Palette.surfaceSunken)
    }
}
