import AppKit
import Darwin
import Observation
import SwiftTerm
import SwiftUI
import SwarmCore

@MainActor
final class SwarmTerminalView: LocalProcessTerminalView {
    var onEnded: (() -> Void)?
    var onFocus: (() -> Void)?
    private(set) var ended = false
    private var stopping = false

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
        onFocus?()
        super.mouseDown(with: event)
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

@MainActor @Observable
final class AgentPaneStore {
    private let bus = SwarmCLIBus()
    private var terminals: [String: SwarmTerminalView] = [:]
    private(set) var ended: Set<String> = []
    private(set) var focusedKey: String?

    func clearFocus() { focusedKey = nil }

    func key(session: SwarmSession, agent: SwarmAgent) -> String {
        session.id.rawValue + ":" + agent.id.rawValue
    }

    func terminal(session: SwarmSession, agent: SwarmAgent) -> SwarmTerminalView {
        let key = key(session: session, agent: agent)
        if let terminal = terminals[key] { return terminal }
        let terminal = SwarmTerminalView(frame: CGRect(x: 0, y: 0, width: 900, height: 400))
        terminal.onEnded = { [weak self] in self?.ended.insert(key) }
        terminal.onFocus = { [weak self] in self?.focusedKey = key }
        let command = SwarmPanePolicy.attachCommand(bus: bus, session: session, agent: agent.id)
        terminal.start(SwarmAttachLaunch(command: command, directory: session.cwd))
        terminals[key] = terminal
        return terminal
    }

    func stopAll() {
        for terminal in terminals.values { terminal.willStop() }
        terminals.removeAll()
        ended.removeAll()
        focusedKey = nil
    }
}

struct AgentTerminalView: View {
    let session: SwarmSession
    let agent: SwarmAgent
    let store: AgentPaneStore

    @State private var terminal: SwarmTerminalView?

    var body: some View {
        ZStack {
            if let terminal {
                TerminalContainer(terminal: terminal)
            }
            if store.ended.contains(store.key(session: session, agent: agent)) {
                Text("This agent has ended")
                    .padding(8)
                    .background(.regularMaterial)
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .overlay {
            RoundedRectangle(cornerRadius: 2)
                .stroke(store.focusedKey == store.key(session: session, agent: agent)
                    ? Color.accentColor : Color.clear, lineWidth: 2)
                .allowsHitTesting(false)
        }
        .task(id: store.key(session: session, agent: agent)) {
            terminal = store.terminal(session: session, agent: agent)
        }
    }
}

private struct TerminalContainer: NSViewRepresentable {
    let terminal: SwarmTerminalView

    func makeNSView(context: Context) -> NSView {
        let host = TerminalHostView()
        attach(to: host)
        return host
    }

    func updateNSView(_ host: NSView, context: Context) {
        attach(to: host)
    }

    private func attach(to host: NSView) {
        guard terminal.superview !== host else { return }
        terminal.removeFromSuperview()
        terminal.frame = host.bounds
        terminal.autoresizingMask = [.width, .height]
        host.addSubview(terminal)
        host.nextKeyView = terminal
    }
}

private final class TerminalHostView: NSView {
    override var acceptsFirstResponder: Bool { true }

    override func mouseDown(with event: NSEvent) {
        if let terminal = subviews.first { window?.makeFirstResponder(terminal) }
        super.mouseDown(with: event)
    }
}
