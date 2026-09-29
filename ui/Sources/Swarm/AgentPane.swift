import AppKit
import Darwin
import Observation
import SwiftTerm
import SwiftUI
import SwarmCore

@MainActor
final class SwarmTerminalView: LocalProcessTerminalView {
    var onEnded: (() -> Void)?
    var onFocusChange: ((Bool) -> Void)?
    private(set) var ended = false
    private var stopping = false
    private var scrolledOnAttach = false

    override init(frame: CGRect) {
        super.init(frame: frame)
        applySystemColors()
    }

    required init?(coder: NSCoder) {
        super.init(coder: coder)
        applySystemColors()
    }

    func start(_ launch: SwarmAttachLaunch) {
        guard !process.running else { return }
        startProcess(
            executable: launch.executable, args: launch.arguments,
            environment: launch.environment.map { "\($0.key)=\($0.value)" }.sorted(),
            execName: URL(fileURLWithPath: launch.executable).lastPathComponent,
            currentDirectory: launch.directory
        )
    }

    func willStop() {
        stopping = true
        guard process.running else { return }
        let pid = process.shellPid
        if pid > 0 { _ = Darwin.kill(-pid, SIGHUP) }
        terminate()
    }

    func showLatestOnFirstAttach() {
        guard !scrolledOnAttach else { return }
        scrolledOnAttach = true
        scroll(toPosition: 1)
    }

    override func processTerminated(_ source: LocalProcess, exitCode: Int32?) {
        super.processTerminated(source, exitCode: exitCode)
        Task { @MainActor in
            self.ended = true
            if !self.stopping { self.onEnded?() }
        }
    }

    override func send(source: SwiftTerm.TerminalView, data: ArraySlice<UInt8>) {
        guard !ended else { return }
        super.send(source: source, data: data)
    }

    override func mouseDown(with event: NSEvent) {
        window?.makeFirstResponder(self)
        super.mouseDown(with: event)
    }

    // SwiftTerm sets this from its non-overridable first-responder methods.
    override var hasFocus: Bool {
        get { super.hasFocus }
        set {
            super.hasFocus = newValue
            onFocusChange?(newValue)
        }
    }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        if window == nil { onFocusChange?(false) }
    }

    override func viewDidChangeEffectiveAppearance() {
        super.viewDidChangeEffectiveAppearance()
        applySystemColors()
    }

    var firstScreenLine: String {
        let terminal = getTerminal()
        for index in 0..<terminal.rows {
            let text = terminal.getLine(row: index)?.translateToString(trimRight: true) ?? ""
            if !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty { return text }
        }
        return ""
    }

    private func applySystemColors() {
        nativeForegroundColor = .labelColor
        nativeBackgroundColor = .textBackgroundColor
        caretColor = .labelColor
    }
}

/// Owns the live terminals of the selected chat (ADR 0024). A terminal starts when its pane first
/// appears and stays while the chat is selected, also when its pane scrolls off screen.
@MainActor @Observable
final class AgentPaneStore {
    private let bus = SwarmCLIBus()
    private var terminals: [String: SwarmTerminalView] = [:]
    private(set) var ended: Set<String> = []
    private(set) var focusedKey: String?
    /// The pane that fills the main area. Keys set it; the strip only shows it.
    var zoomedKey: String?

    func clearFocus() { focusedKey = nil }

    static func key(session: SwarmSessionID, agent: String) -> String {
        session.rawValue + ":" + agent
    }

    func key(session: SwarmSession, agent: SwarmAgent) -> String {
        Self.key(session: session.id, agent: agent.id.rawValue)
    }

    func existingTerminal(key: String) -> SwarmTerminalView? { terminals[key] }

    func terminal(session: SwarmSession, agent: SwarmAgent) -> SwarmTerminalView {
        open(key: key(session: session, agent: agent), launch: attachLaunch(session: session, agent: agent))
    }

    func open(key: String, launch: @autoclosure () -> SwarmAttachLaunch) -> SwarmTerminalView {
        if let terminal = terminals[key] { return terminal }
        let timing = SwarmPerformance.begin("TerminalAttach")
        defer { timing.end() }
        let terminal = SwarmTerminalView(frame: CGRect(x: 0, y: 0, width: 900, height: 400))
        terminal.onEnded = { [weak self, weak terminal] in
            guard let self, self.terminals[key] === terminal else { return }
            self.ended.insert(key)
        }
        terminal.onFocusChange = { [weak self] focused in
            guard let self else { return }
            if focused {
                self.focusedKey = key
            } else if self.focusedKey == key {
                self.focusedKey = nil
            }
        }
        terminal.start(launch())
        terminals[key] = terminal
        return terminal
    }

    func reconnect(key: String, launch: @autoclosure () -> SwarmAttachLaunch) {
        if let terminal = terminals.removeValue(forKey: key) { stop(terminal) }
        ended.remove(key)
        _ = open(key: key, launch: launch())
    }

    func reconnect(session: SwarmSession, agent: SwarmAgent) {
        reconnect(key: key(session: session, agent: agent), launch: attachLaunch(session: session, agent: agent))
    }

    func focus(key: String) {
        guard let terminal = terminals[key] else { return }
        terminal.window?.makeFirstResponder(terminal)
    }

    /// Stops every terminal outside `session`, so a chat or workspace switch frees the old panes.
    func stop(keepingSession session: SwarmSessionID?) {
        let kept = session.map { $0.rawValue + ":" }
        for (key, terminal) in terminals where kept.map({ !key.hasPrefix($0) }) ?? true {
            terminals.removeValue(forKey: key)
            ended.remove(key)
            if focusedKey == key { focusedKey = nil }
            if zoomedKey == key { zoomedKey = nil }
            stop(terminal)
        }
    }

    func stopAll() { stop(keepingSession: nil) }

    private func stop(_ terminal: SwarmTerminalView) {
        terminal.willStop()
        terminal.removeFromSuperview()
    }

    private func attachLaunch(session: SwarmSession, agent: SwarmAgent) -> SwarmAttachLaunch {
        let command = SwarmPanePolicy.attachCommand(bus: bus, session: session, agent: agent.id)
        return SwarmAttachLaunch(command: command, directory: session.cwd)
    }
}

/// Shows the store's terminal for `key`, starting it through `open` when the pane first appears.
/// A connecting pane stays blank, with no spinner.
struct AgentTerminalView: View {
    let key: String
    let store: AgentPaneStore
    let open: () -> Void

    var body: some View {
        ZStack {
            if let terminal = store.existingTerminal(key: key) {
                // A reconnect makes a new terminal; a new host lets the old one dismantle.
                TerminalContainer(terminal: terminal).id(ObjectIdentifier(terminal))
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .task(id: key) { open() }
    }
}

private struct TerminalContainer: NSViewRepresentable {
    let terminal: SwarmTerminalView

    func makeNSView(context: Context) -> TerminalHostView {
        let host = TerminalHostView()
        attach(to: host)
        return host
    }

    func updateNSView(_ host: TerminalHostView, context: Context) {
        attach(to: host)
    }

    static func dismantleNSView(_ host: TerminalHostView, coordinator: ()) {
        // Only what this host still owns: another host may already show the terminal.
        for view in host.subviews { view.removeFromSuperview() }
    }

    private func attach(to host: TerminalHostView) {
        guard terminal.superview !== host else { return }
        terminal.removeFromSuperview()
        host.addSubview(terminal)
        host.fitTerminal()
        host.nextKeyView = terminal
        terminal.showLatestOnFirstAttach()
    }
}

private final class TerminalHostView: NSView {
    override var acceptsFirstResponder: Bool { true }

    override func mouseDown(with event: NSEvent) {
        if let terminal = subviews.first { window?.makeFirstResponder(terminal) }
        super.mouseDown(with: event)
    }

    override func resizeSubviews(withOldSize oldSize: NSSize) {
        fitTerminal()
    }

    /// A zero frame would resize the terminal to one cell and make tmux reflow its screen.
    func fitTerminal() {
        guard bounds.width > 0, bounds.height > 0 else { return }
        for view in subviews { view.frame = bounds }
    }

    override func hitTest(_ point: NSPoint) -> NSView? {
        let hit = super.hitTest(point)
        // SwiftTerm drops sideways wheel events, so they go up to the strip's scroll view.
        if hit != nil, let event = NSApp.currentEvent, event.type == .scrollWheel,
           abs(event.scrollingDeltaX) > abs(event.scrollingDeltaY) {
            return self
        }
        return hit
    }
}
