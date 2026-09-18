import AppKit
import QuartzCore
import SwiftUI
import SwarmCore

/// Keeps one display-link clock beside the main window and records only gaps large enough to be
/// visible. The view owns the link, so closing the window also stops the callbacks.
@MainActor
struct PerformanceStallMonitor: NSViewRepresentable {
    var app: AppModel

    func makeNSView(context: Context) -> PerformanceStallView {
        PerformanceStallView(app: app)
    }

    func updateNSView(_ view: PerformanceStallView, context: Context) {
        view.app = app
    }
}

@MainActor
final class PerformanceStallView: NSView {
    weak var app: AppModel?
    private var link: CADisplayLink?
    private var last: CFTimeInterval = 0

    init(app: AppModel) {
        self.app = app
        super.init(frame: .zero)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { nil }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        if window == nil {
            stop()
        } else {
            start()
        }
    }

    private func start() {
        guard link == nil else { return }
        last = 0
        let link = displayLink(target: self, selector: #selector(tick))
        link.add(to: .main, forMode: .common)
        self.link = link
    }

    private func stop() {
        link?.invalidate()
        link = nil
        last = 0
    }

    @objc private func tick() {
        let current = CACurrentMediaTime()
        defer { last = current }
        guard last > 0 else { return }
        let milliseconds = (current - last) * 1_000
        guard milliseconds > 120 else { return }
        let screen = screenState()
        PerfLog.shared.record(.stall(
            milliseconds: milliseconds,
            paneKind: screen.paneKind,
            drawnRowCount: screen.drawnRowCount
        ))
    }

    private func screenState() -> (paneKind: String, drawnRowCount: Int) {
        guard let model = app?.selectedModel,
              let content = WorkspaceTabsStore.shared.selectedTab(in: model)
        else { return ("home", 0) }
        switch content {
        case .chat:
            return (PaneKind.chat.rawValue, TranscriptDrawn.rows)
        case .tool(let id):
            let kind = CenterTabStore.shared.tabs(for: model.workspace.id)
                .first { $0.id == id }?.kind.rawValue ?? "unknown"
            return (kind, 0)
        }
    }
}
