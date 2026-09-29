import SwiftUI
import SwarmCore

/// `SWARM_PANE_STRESS=N` only: the pane strip with N local streaming panes for the performance gate.
struct PaneStressWindow: View {
    @State private var panes = AgentPaneStore()
    private let cells = (0..<SwarmPaneStress.count).map {
        PaneCell(id: "stress-\($0)", title: "stress-\($0)", role: "stress", model: "sh", alive: true)
    }

    var body: some View {
        PaneStrip(
            cells: cells,
            focusedID: cells.first { $0.id == panes.focusedKey }?.id,
            zoomedID: panes.zoomedKey,
            onFocus: { panes.focus(key: $0) },
            onZoom: { panes.zoomedKey = $0 },
            onReconnect: { panes.reconnect(key: $0, launch: SwarmPaneStress.launch) }
        ) {
            Text("Pane stress: \(cells.count) panes")
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        } pane: { cell in
            AgentTerminalView(key: cell.id, store: panes) {
                _ = panes.open(key: cell.id, launch: SwarmPaneStress.launch)
            }
        }
        .frame(minWidth: 900, minHeight: 600)
        .onDisappear { panes.stopAll() }
        .onReceive(NotificationCenter.default.publisher(for: NSApplication.willTerminateNotification)) { _ in
            panes.stopAll()
        }
    }
}
