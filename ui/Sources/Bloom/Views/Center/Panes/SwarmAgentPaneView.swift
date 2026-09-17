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

    var body: some View {
        let state = SwarmAgentPaneState(
            isAlive: isAlive,
            attachment: TerminalSessionStore.shared.swarmAgentAttachment(for: agent, in: session)
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
        case .terminal:
            terminal()
        case .reattach:
            VStack(spacing: 0) {
                SwarmAgentPaneReattachStrip {
                    TerminalSessionStore.shared.reattach(
                        agent: agent, session: session, bus: app.swarmBus
                    )
                }
                terminal()
            }
        }
    }

    private func terminal() -> some View {
        SwarmAgentPaneTerminalView(
            agent: agent,
            session: session,
            workspaceID: app.selectedWorkspace?.id,
            bus: app.swarmBus
        )
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(Palette.surfaceSunken)
    }
}
