import Foundation
import Observation
import SwiftUI
import SwarmCore

@MainActor @Observable
final class SessionsTreeModel {
    private let bus = SwarmCLIBus()
    private let discovery = SwarmSessionDiscovery()

    var tree = SessionsTree(projects: [])
    var selectedSessionID = UserDefaults.standard.string(forKey: "selectedSessionID").map(SwarmSessionID.init) {
        didSet {
            UserDefaults.standard.set(selectedSessionID?.rawValue, forKey: "selectedSessionID")
        }
    }
    private var pendingID: SwarmSessionID?
    var agents: [SwarmAgent] = []
    var error: String?

    var selectedSession: SwarmProjectSession? { selectedSessionID.flatMap(tree.session) }

    func select(_ id: SwarmSessionID) {
        pendingID = nil
        selectedSessionID = id
        agents = []
    }

    func startChat(_ plan: SwarmChatLaunchPlan) async throws -> SwarmSessionID {
        try await SwarmChatLauncher.start(plan, bus: bus) { id in
            await MainActor.run {
                self.pendingID = id
                self.selectedSessionID = id
                self.agents = []
            }
        }
    }

    func refresh() async throws {
        let sessions = try await bus.sessions()
        tree = try await discovery.tree(sessions: sessions, bus: bus)
        if let selectedSessionID, let row = tree.session(selectedSessionID) {
            pendingID = nil
            self.selectedSessionID = row.id
            agents = try await bus.agents(in: row.session)
        } else if pendingID == nil {
            selectedSessionID = tree.retainedSelection(selectedSessionID)
            agents = []
        }
        error = nil
    }

    func archive(_ id: SwarmSessionID) async throws {
        try await bus.archive([id])
        clearSelection(if: id)
        try await refresh()
    }

    func close(_ id: SwarmSessionID) async throws {
        try await bus.close(SwarmPanePolicy.chair, in: id)
        try await bus.archive([id])
        clearSelection(if: id)
        try await refresh()
    }

    func run() async {
        while !Task.isCancelled {
            do { try await refresh() }
            catch { self.error = String(describing: error) }
            try? await Task.sleep(for: .seconds(2))
        }
    }

    private func clearSelection(if id: SwarmSessionID) {
        guard selectedSessionID == id else { return }
        selectedSessionID = nil
        agents = []
    }
}

private struct SessionsWindow: View {
    @State private var model = SessionsTreeModel()
    @State private var panes = AgentPaneStore()
    @State private var newChatDirectory: String?

    var body: some View {
        NavigationSplitView {
            List(selection: $model.selectedSessionID) {
                ForEach(sidebarRows) { entry in
                    switch entry {
                    case .project(let project):
                        ProjectRowLabel(name: project.name) {
                            newChatDirectory = project.launchDirectory
                        }
                    case .chat(let row):
                        chatRow(row)
                        .padding(.leading, 16)
                        .tag(row.id)
                        .contextMenu { chatMenu(row) }
                    }
                }
            }
            .listStyle(.sidebar)
            .navigationTitle("Workspaces")
            .navigationSplitViewColumnWidth(min: 240, ideal: 300)
            .onChange(of: model.selectedSessionID) { oldID, id in
                guard oldID != id else { return }
                NSApp.keyWindow?.makeFirstResponder(nil)
                panes.clearFocus()
                if let id { model.select(id) }
            }
            .toolbar {
                Button {
                    if KeyRouting.route(focus: .sidebar, key: .commandN) == .openNewChat,
                       let id = model.selectedSessionID,
                       let path = model.tree.launchDirectory(for: id) {
                        newChatDirectory = path
                    }
                } label: {
                    Image(systemName: "plus.circle")
                }
                .help("New chat")
                .accessibilityLabel("New chat")
                .keyboardShortcut("n", modifiers: .command)
                .disabled(model.selectedSession == nil)
            }
        } detail: {
            if let row = model.selectedSession {
                SessionDetailView(
                    row: row,
                    title: model.selectedSessionID.flatMap(model.tree.windowTitle) ?? row.title,
                    agents: model.agents, panes: panes
                )
                    .id(row.id)
            } else if let error = model.error {
                ContentUnavailableView(error, systemImage: "exclamationmark.triangle")
            } else {
                ContentUnavailableView("Select a chat", systemImage: "bubble.left")
            }
        }
        .background(WindowFrameRestorer())
        .task {
            LoginShellPath.begin()
            await model.run()
        }
        .onDisappear { panes.stopAll() }
        .onReceive(NotificationCenter.default.publisher(for: NSApplication.willTerminateNotification)) { _ in
            panes.stopAll()
        }
        .sheet(item: Binding(
            get: { newChatDirectory.map(LaunchTarget.init) },
            set: { newChatDirectory = $0?.directory }
        )) { target in
            NewChatSheet(directory: target.directory, launch: model.startChat) { _ in
                Task { try? await model.refresh() }
            }
        }
    }

    private func chatRow(_ row: ChatRow) -> some View {
        let presentation = SessionRowPresentation.make(
            row, now: Int(Date().timeIntervalSince1970)
        )
        return HStack(spacing: 8) {
            RoundedRectangle(cornerRadius: 1.5)
                .fill(presentation.state == .live ? Color.green : Color.clear)
                .frame(width: 3)
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 2) {
                HStack(spacing: 5) {
                    Text(presentation.title)
                        .font(.body)
                        .foregroundStyle(
                            presentation.state == .ended ? Color.secondary : Color.primary
                        )
                        .lineLimit(1)
                        .truncationMode(.tail)
                    if let provider = presentation.provider {
                        Text(providerBadge(provider))
                            .font(.caption2)
                            .frame(width: 16, height: 16)
                            .background(Circle().fill(.quaternary))
                            .accessibilityLabel(provider)
                    }
                }
                Text(presentation.caption)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
            Spacer()
            Text(presentation.age)
                .font(.caption)
                .foregroundStyle(.secondary)
        }
    }

    @ViewBuilder
    private func chatMenu(_ row: ChatRow) -> some View {
        let presentation = SessionRowPresentation.make(
            row, now: Int(Date().timeIntervalSince1970)
        )
        Button("New chat here") { newChatDirectory = row.workspacePath }
        Button("Close chat") {
            Task { try? await model.close(row.id) }
        }
        .disabled(presentation.state != .live)
        Button("Archive") {
            Task { try? await model.archive(row.id) }
        }
    }

    private func providerBadge(_ provider: String) -> String {
        switch provider.lowercased() {
        case "codex": "X"
        case "agy": "A"
        default: provider.prefix(1).uppercased()
        }
    }

    private var sidebarRows: [SidebarRow] {
        var rows: [SidebarRow] = []
        for project in model.tree.projects {
            rows.append(.project(project))
            rows += project.chats.map(SidebarRow.chat)
        }
        return rows
    }
}

private struct ProjectRowLabel: View {
    let name: String
    let onNewChat: () -> Void
    @State private var hovered = false

    var body: some View {
        HStack {
            Text(name)
                .font(.caption)
                .foregroundStyle(.secondary)
                .lineLimit(1)
                .truncationMode(.middle)
            Spacer()
            Button(action: onNewChat) {
                Image(systemName: "plus.circle")
            }
            .buttonStyle(.borderless)
            .help("New chat")
            .accessibilityLabel("New chat")
            .opacity(hovered ? 1 : 0)
            .allowsHitTesting(hovered)
        }
        .onHover { hovered = $0 }
    }
}

private enum SidebarRow: Identifiable {
    case project(ProjectNode)
    case chat(ChatRow)

    var id: String {
        switch self {
        case .project(let project): "project:\(project.path)"
        case .chat(let row): "chat:\(row.id.rawValue)"
        }
    }
}

private struct WindowFrameRestorer: NSViewRepresentable {
    func makeNSView(context: Context) -> NSView { WindowFrameView() }
    func updateNSView(_ view: NSView, context: Context) {}
}

private final class WindowFrameView: NSView {
    private let frameName = "SwarmSessionsWindow"

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        guard let window, window.frameAutosaveName != frameName else { return }
        window.setFrameAutosaveName(frameName)
        if !window.setFrameUsingName(frameName) {
            window.setFrame(window.screen?.visibleFrame ?? NSScreen.main?.visibleFrame ?? window.frame, display: true)
        }
        let notifications = NotificationCenter.default
        notifications.addObserver(self, selector: #selector(saveFrame), name: NSWindow.didEndLiveResizeNotification, object: window)
        notifications.addObserver(self, selector: #selector(saveFrame), name: NSWindow.didMoveNotification, object: window)
        notifications.addObserver(self, selector: #selector(saveFrame), name: NSApplication.willTerminateNotification, object: nil)
    }

    @objc private func saveFrame() {
        window?.saveFrame(usingName: frameName)
    }

    deinit {
        NotificationCenter.default.removeObserver(self)
    }
}

private struct LaunchTarget: Identifiable {
    let directory: String
    var id: String { directory }
}

struct SwarmApp: App {
    var body: some Scene {
        WindowGroup { SessionsWindow() }
    }
}

@main
@MainActor
enum SwarmExecutable {
    static func main() async {
        let arguments = Array(CommandLine.arguments.dropFirst())
        if arguments == ["--print-tree"] {
            let model = SessionsTreeModel()
            do {
                try await model.refresh()
                let output = model.tree.text()
                if !output.isEmpty { print(output) }
            } catch {
                fputs("\(error)\n", stderr)
                exit(1)
            }
        } else if arguments.count == 2, arguments[0] == "--print-transcript" {
            await printTranscript(prefix: arguments[1])
        } else if arguments.count == 3, arguments[0] == "--attach-check" {
            await attachCheck(prefix: arguments[1], agentID: SwarmAgentID(arguments[2]))
        } else if arguments.count == 4, arguments[0] == "--launch-check" {
            await launchCheck(directory: arguments[1], provider: arguments[2], roleID: arguments[3])
        } else {
            SwarmApp.main()
        }
    }

    private static func launchCheck(directory: String, provider: String, roleID: String) async {
        do {
            await LoginShellPath.ready()
            let roles = try await SwarmCLIProfileSource().roles()
            guard let role = SwarmLaunchChoice.roles(roles, for: provider).first(where: { $0.id == roleID }),
                  let plan = SwarmChatLaunchPlan(directory: directory, role: role, account: .auto) else {
                throw SwarmProfileError.failed("Provider, role, or directory is invalid")
            }
            let id = try await SessionsTreeModel().startChat(plan)
            let agent = try await SwarmChatLauncher.waitForChairPane(in: id, bus: SwarmCLIBus())
            print("session: \(id.rawValue)")
            print("agent: \(agent.id.rawValue)")
            print("pane: \(agent.pane ?? "")")
        } catch {
            fputs("\((error as? SwarmProfileError)?.message ?? String(describing: error))\n", stderr)
            exit(1)
        }
    }

    private static func printTranscript(prefix: String) async {
        do {
            let session = try await matchingSession(prefix: prefix)
            let agents = try await SwarmCLIBus().agents(in: session)
            let provider = agents.first { $0.id == SwarmPanePolicy.chair }?.provider
            let snapshot = await SwarmChairTranscript().poll(
                session: session, chairProvider: provider
            )
            print(snapshot.printText)
            if case .unavailable = snapshot { exit(1) }
        } catch {
            fputs("\(error)\n", stderr)
            exit(1)
        }
    }

    private static func attachCheck(prefix: String, agentID: SwarmAgentID) async {
        do {
            let session = try await matchingSession(prefix: prefix)
            let bus = SwarmCLIBus()
            guard let agent = try await bus.agents(in: session).first(where: { $0.id == agentID }) else {
                throw SwarmProfileError.failed("agent not found")
            }
            if let reason = SwarmPanePolicy.unavailableReason(session: session, agent: agent) {
                throw SwarmProfileError.failed(reason)
            }
            await LoginShellPath.ready()
            let store = AgentPaneStore()
            let terminal = store.terminal(session: session, agent: agent)
            try await Task.sleep(for: .seconds(2))
            let alive = terminal.process.running
            print("child alive: \(alive)")
            print("first screen line: \(terminal.firstScreenLine)")
            store.stopAll()
            if !alive { exit(1) }
        } catch {
            fputs("\(error)\n", stderr)
            exit(1)
        }
    }

    private static func matchingSession(prefix: String) async throws -> SwarmSession {
        let sessions = try await SwarmCLIBus().sessions().filter { $0.id.rawValue.hasPrefix(prefix) }
        guard sessions.count == 1, let session = sessions.first else {
            throw SwarmProfileError.failed("session prefix does not name one session")
        }
        return session
    }
}
