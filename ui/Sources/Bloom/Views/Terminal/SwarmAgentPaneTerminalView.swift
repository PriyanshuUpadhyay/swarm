import SwiftUI
import BloomCore

/// The SwiftUI host for a retained swarm attach terminal.
struct SwarmAgentPaneTerminalView: NSViewRepresentable {
    var agent: SwarmAgentID
    var session: SwarmSessionID
    var workspaceID: WorkspaceID?
    var bus: any SwarmBus

    func makeNSView(context: Context) -> TerminalHostView {
        let host = TerminalHostView()
        configure(host)
        return host
    }

    func updateNSView(_ nsView: TerminalHostView, context: Context) {
        configure(nsView)
    }

    private func configure(_ host: TerminalHostView) {
        let terminal = TerminalSessionStore.shared.swarmAgentTerminal(
            for: agent, in: session, workspaceID: workspaceID, bus: bus
        )
        host.isFocusedPane = false
        host.attach(terminal)
        terminal.updateTheme()
    }
}
