import SwiftUI
import SwarmCore

/// `SWARM_PANE_STRESS=N` only: the pane strip with N local streaming panes for the performance gate.
private let stressStatuses: [AgentStatus] = [.working, .waiting, .done, .failed, .ended]

struct PaneStressWindow: View {
    @State private var panes = AgentPaneStore()
    @State private var lastWindowAction = "none"
    private let cells = (0..<SwarmPaneStress.count).map {
        PaneCell(id: "stress-\($0)", title: "stress-\($0)", role: "stress", model: "sh", status: stressStatuses[$0 % stressStatuses.count])
    }

    var body: some View {
        PaneStrip(
            cells: cells,
            focusedID: cells.first { $0.id == panes.focusedKey }?.id,
            zoomedID: panes.zoomedKey,
            revealID: panes.revealKey,
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
        .focusedSceneValue(\.chatKeyActions, ChatKeyActions(
            terminalFocused: panes.focusedKey != nil,
            focusComposer: { panes.revealChat() },
            moveFocus: { direction in
                if !panes.moveFocus(direction, among: cells.map(\.id)) {
                    NSApp.keyWindow?.makeFirstResponder(nil)
                }
            },
            zoom: { panes.toggleZoom() }
        ))
        // The stress window has no chats or sidebar; these record that the menu reached them.
        .focusedSceneValue(\.windowKeyActions, WindowKeyActions(
            newChat: { lastWindowAction = "newChat" },
            newWorkspace: { lastWindowAction = "newWorkspace" },
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
    /// `SWARM_KEY_SCRIPT="opt+cmd+right,cmd+1"` sends each chord through `NSApp.sendEvent`, so the
    /// real menu and responder path runs, and prints the focus, zoom, and window action after it.
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
                // Stands in for a click on the first pane, which focuses its terminal.
                panes.focus(key: cells[0].id)
                try? await Task.sleep(for: .milliseconds(500))
                log(word)
                continue
            }
            guard let chord = KeyChord(script: word), let event = Self.event(chord) else {
                log("unknown \(word)")
                continue
            }
            lastWindowAction = "none"
            NSApp.sendEvent(event)
            try? await Task.sleep(for: .milliseconds(500))
            log(word)
        }
    }

    private func log(_ step: String) {
        let responder = NSApp.keyWindow?.firstResponder.map { String(describing: type(of: $0)) } ?? "nil"
        let meta = cells.map { "\(panes.optionAsMeta(key: $0.id).map(String.init) ?? "-")" }.joined(separator: "/")
        print("key-script \(step): focused=\(panes.focusedKey ?? "chat") zoomed=\(panes.zoomedKey ?? "none")"
            + " reveal=\(panes.revealKey ?? "chat") action=\(lastWindowAction) responder=\(responder)"
            + " optionAsMeta=\(meta) active=\(NSApp.isActive)")
        fflush(stdout)
    }

    private static func event(_ chord: KeyChord) -> NSEvent? {
        guard let window = NSApp.keyWindow ?? NSApp.windows.first else { return nil }
        let codes: [Character: UInt16] = [
            "1": 18, "2": 19, "o": 31, "l": 37, "k": 40, "b": 11, "n": 45, "x": 7, "]": 30, "[": 33,
        ]
        let code: UInt16
        let text: String
        switch chord.key {
        case .returnKey: (code, text) = (36, "\r")
        case .escape: (code, text) = (53, "\u{1b}")
        case .left: (code, text) = (123, String(UnicodeScalar(NSLeftArrowFunctionKey)!))
        case .right: (code, text) = (124, String(UnicodeScalar(NSRightArrowFunctionKey)!))
        case .down: (code, text) = (125, String(UnicodeScalar(NSDownArrowFunctionKey)!))
        case .up: (code, text) = (126, String(UnicodeScalar(NSUpArrowFunctionKey)!))
        case .character(let character):
            guard let known = codes[character] else { return nil }
            (code, text) = (known, String(character))
        }
        var flags: NSEvent.ModifierFlags = []
        if chord.modifiers.contains(.command) { flags.insert(.command) }
        if chord.modifiers.contains(.shift) { flags.insert(.shift) }
        if chord.modifiers.contains(.option) { flags.insert(.option) }
        if chord.modifiers.contains(.control) { flags.insert(.control) }
        if (123...126).contains(code) { flags.formUnion([.function, .numericPad]) }
        return NSEvent.keyEvent(
            with: .keyDown, location: .zero, modifierFlags: flags,
            timestamp: ProcessInfo.processInfo.systemUptime, windowNumber: window.windowNumber,
            context: nil, characters: text, charactersIgnoringModifiers: text,
            isARepeat: false, keyCode: code
        )
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
