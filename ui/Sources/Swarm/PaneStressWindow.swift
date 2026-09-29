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
        .frame(minWidth: 1600, minHeight: 1000)
        .background { if SwarmPaneStress.scrolls { StripSweeper() } }
        .onDisappear { panes.stopAll() }
        .onReceive(NotificationCenter.default.publisher(for: NSApplication.willTerminateNotification)) { _ in
            panes.stopAll()
        }
    }
}

/// Moves the strip's scroll view at 1,500 pt/s, turning at each end, as a stand-in for a trackpad.
private struct StripSweeper: NSViewRepresentable {
    func makeNSView(context: Context) -> NSView { SweepView() }
    func updateNSView(_ view: NSView, context: Context) {}

    private final class SweepView: NSView {
        private var timer: Timer?
        private var direction: CGFloat = 1
        private weak var found: NSScrollView?

        override func viewDidMoveToWindow() {
            super.viewDidMoveToWindow()
            timer?.invalidate()
            guard window != nil else { return }
            let timer = Timer(timeInterval: 1.0 / 120, repeats: true) { [weak self] _ in
                MainActor.assumeIsolated { self?.step() }
            }
            RunLoop.main.add(timer, forMode: .common)
            self.timer = timer
        }

        private func step() {
            if found == nil { found = window?.contentView.flatMap(Self.strip(in:)) }
            guard let strip = found,
                  let document = strip.documentView else { return }
            let clip = strip.contentView
            let maxX = document.frame.width - clip.bounds.width
            var x = clip.bounds.origin.x + direction * 1500 / 120
            if x >= maxX || x <= 0 { direction = -direction; x = min(max(x, 0), maxX) }
            clip.scroll(to: CGPoint(x: x, y: clip.bounds.origin.y))
            strip.reflectScrolledClipView(clip)
        }

        private static func strip(in view: NSView) -> NSScrollView? {
            if let scroll = view as? NSScrollView, let document = scroll.documentView,
               document.frame.width > scroll.contentView.bounds.width + 1 { return scroll }
            for child in view.subviews { if let found = strip(in: child) { return found } }
            return nil
        }
    }
}
