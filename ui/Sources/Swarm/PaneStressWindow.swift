import SwiftUI
import SwarmCore

/// `SWARM_PANE_STRESS=N` only: the pane strip with N child chat columns for the performance gate.
private let stressStatuses: [AgentStatus] = [.working, .waiting, .done, .failed, .ended]

struct PaneStressWindow: View {
    @State private var panes = AgentPaneStore()
    @State private var lastWindowAction = "none"
    @State private var cells = (0..<SwarmPaneStress.count).map {
        PaneCell(id: "stress-\($0)", title: "stress-\($0)", role: "stress", model: "sh", status: stressStatuses[$0 % stressStatuses.count])
    }

    var body: some View {
        PaneStrip(
            cells: cells,
            focusedID: cells.first { $0.id == panes.focusedKey }?.id,
            zoomedID: panes.zoomedKey,
            revealID: panes.revealKey,
            revealCount: panes.revealCount,
            splitScope: "pane-stress",
            onFocus: { panes.focus(key: $0) },
            onZoom: { panes.toggleZoom(key: $0) },
            onDismiss: { ids in
                cells.removeAll { $0.ended && ids.contains($0.id) }
                panes.revealChat()
            },
            readOnlyReason: nil, onStop: { _ in }, onClose: { _ in }
        ) {
            Text("Pane stress: \(cells.count) panes")
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        } pane: { cell in
            StressColumn(model: panes.column(key: cell.id), withPrompt: cell.id == cells.first?.id)
        }
        .frame(minWidth: 1600, minHeight: 1000)
        .focusedSceneValue(\.chatKeyActions, ChatKeyActions(
            focusComposer: { panes.revealChat() },
            moveFocus: { direction in
                if !panes.moveFocus(direction, among: cells.map(\.id)) {
                    NSApp.keyWindow?.makeFirstResponder(nil)
                }
            },
            zoom: { panes.toggleZoom() },
            stop: {}
        ))
        // The stress window has no chats or sidebar; these record that the menu reached them.
        .focusedSceneValue(\.windowKeyActions, WindowKeyActions(
            newChat: { lastWindowAction = "newChat" },
            recentlyClosed: { lastWindowAction = "recentlyClosed" },
            newWorkspace: { lastWindowAction = "newWorkspace" },
            newProject: { lastWindowAction = "newProject" },
            stepWorkspace: { lastWindowAction = "stepWorkspace(\($0))" },
            selectTab: { lastWindowAction = "selectTab(\($0))" },
            stepTab: { lastWindowAction = "stepTab(\($0))" },
            toggleSidebar: { lastWindowAction = "toggleSidebar" },
            moveSidebar: { lastWindowAction = "moveSidebar" },
            sidebarView: { lastWindowAction = "sidebarView(\($0))" },
            showChanges: { lastWindowAction = "showChanges" },
            search: { lastWindowAction = "search" }
        ))
        .task { await runKeyScript() }
        .background { if SwarmPaneStress.scrolls { StripSweeper() } }
        .onDisappear { panes.stopAll() }
        .onReceive(NotificationCenter.default.publisher(for: NSApplication.willTerminateNotification)) { _ in
            panes.stopAll()
        }
    }
}

extension PaneStressWindow {
    /// `SWARM_KEY_SCRIPT="opt+cmd+right,cmd+1"` posts each chord to this process as a system key
    /// event, so the real menu and responder path runs, and prints the focus, zoom, and window
    /// action after it. `NSApp.sendEvent` is not enough: SwiftUI fills a menu's items only on the
    /// system event path, so a hand-made event found every menu item disabled.
    private func runKeyScript() async {
        guard let script = ProcessInfo.processInfo.environment["SWARM_KEY_SCRIPT"] else { return }
        try? await Task.sleep(for: .seconds(3))
        // Menu commands need a key window; a shell launch may leave the app in the background.
        for _ in 0..<40 where NSApp.keyWindow == nil || !NSApp.isActive {
            NSApp.activate(ignoringOtherApps: true)
            NSApp.windows.first?.makeKeyAndOrderFront(nil)
            try? await Task.sleep(for: .milliseconds(250))
        }
        log("activated")
        try? await Task.sleep(for: .seconds(3))
        log("start")
        for word in script.split(separator: ",").map(String.init) {
            if word == "click" {
                // Stands in for a click on the first pane, which focuses its composer.
                panes.focus(key: cells[0].id)
                try? await Task.sleep(for: .milliseconds(500))
                log(word)
                continue
            }
            guard let chord = KeyChord(script: word), let (down, up) = Self.events(chord) else {
                log("unknown \(word)")
                continue
            }
            lastWindowAction = "none"
            down.postToPid(getpid())
            up.postToPid(getpid())
            try? await Task.sleep(for: .milliseconds(500))
            log(word)
        }
    }

    private func log(_ step: String) {
        let responder = NSApp.keyWindow?.firstResponder.map { String(describing: type(of: $0)) } ?? "nil"
        print("key-script \(step): focused=\(panes.focusedKey ?? "chat") zoomed=\(panes.zoomedKey ?? "none")"
            + " reveal=\(panes.revealKey ?? "chat") action=\(lastWindowAction) responder=\(responder)"
            + " active=\(NSApp.isActive)")
        fflush(stdout)
    }

    private static func events(_ chord: KeyChord) -> (CGEvent, CGEvent)? {
        let codes: [Character: UInt16] = [
            "1": 18, "2": 19, "o": 31, "l": 37, "k": 40, "b": 11, "n": 45, "x": 7, "]": 30, "[": 33,
        ]
        let code: UInt16
        switch chord.key {
        case .returnKey: code = 36
        case .tab: code = 48
        case .escape: code = 53
        case .left: code = 123
        case .right: code = 124
        case .down: code = 125
        case .up: code = 126
        case .character(let character):
            guard let known = codes[character] else { return nil }
            code = known
        }
        var flags: CGEventFlags = []
        if chord.modifiers.contains(.command) { flags.insert(.maskCommand) }
        if chord.modifiers.contains(.shift) { flags.insert(.maskShift) }
        if chord.modifiers.contains(.option) { flags.insert(.maskAlternate) }
        if chord.modifiers.contains(.control) { flags.insert(.maskControl) }
        if (123...126).contains(code) { flags.formUnion([.maskSecondaryFn, .maskNumericPad]) }
        guard let down = CGEvent(keyboardEventSource: nil, virtualKey: code, keyDown: true),
              let up = CGEvent(keyboardEventSource: nil, virtualKey: code, keyDown: false) else { return nil }
        down.flags = flags
        up.flags = flags
        return (down, up)
    }
}

/// A child column's transcript, from `SWARM_PANE_STRESS_LOG` (a Claude log), with a question
/// card in the first column, as a real council shows them.
private struct StressColumn: View {
    let model: ChildColumnModel
    let withPrompt: Bool
    @FocusState private var focused: Bool

    var body: some View {
        TranscriptView(
            snapshot: model.snapshot, revision: model.revision, hasOlder: false,
            isLoadingOlder: false, historyError: nil, waitingMessage: "Set SWARM_PANE_STRESS_LOG",
            chair: "claude", rawSessionJSON: "", isActive: true, isVisible: false,
            loadOlder: {}, onTap: {}, focus: $focused
        ) {
            if withPrompt {
                PromptCard(
                    agent: "stress-0",
                    prompt: SwarmPrompt(
                        id: "stress", question: "Bash command\ntouch probe.txt\nDo you want to proceed?",
                        choices: ["Yes", "Yes, and always allow access to /tmp/work", "No"]
                    ),
                    answer: { _ in }
                )
            }
        }
        .task { await model.poll(log: SwarmPaneStress.log, provider: "claude") }
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
            if SwarmPaneStress.parksAtEnd {
                clip.scroll(to: CGPoint(x: maxX, y: clip.bounds.origin.y))
                strip.reflectScrolledClipView(clip)
                return
            }
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
