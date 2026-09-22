import Foundation
import Observation
import SwiftUI
import SwarmCore

@MainActor @Observable
final class SessionsTreeModel {
    private let bus = SwarmCLIBus()
    private let discovery = SwarmSessionDiscovery()

    var tree = SessionsTree(projects: [])
    var selectedID: SwarmSessionID?
    private var pendingID: SwarmSessionID?
    var agents: [SwarmAgent] = []
    var error: String?

    var selectedSession: SwarmProjectSession? {
        selectedID.flatMap { tree.session($0) }
    }

    func select(_ id: SwarmSessionID) {
        pendingID = nil
        selectedID = id
        agents = []
    }

    func startChat(_ plan: SwarmChatLaunchPlan) async throws -> SwarmSessionID {
        try await SwarmChatLauncher.start(plan, bus: bus) { id in
            await MainActor.run {
                self.pendingID = id
                self.selectedID = id
                self.agents = []
            }
        }
    }

    func refresh() async throws {
        let sessions = try await bus.sessions()
        tree = try await discovery.tree(sessions: sessions, bus: bus)
        if let selectedID, let row = tree.session(selectedID) {
            pendingID = nil
            agents = try await bus.agents(in: row.session)
        } else if pendingID == nil {
            selectedID = tree.retainedSelection(selectedID)
            agents = []
        }
        error = nil
    }

    func run() async {
        while !Task.isCancelled {
            do { try await refresh() }
            catch { self.error = String(describing: error) }
            try? await Task.sleep(for: .seconds(2))
        }
    }
}

private struct SessionsWindow: View {
    @State private var model = SessionsTreeModel()
    @State private var panes = AgentPaneStore()
    @State private var newChatDirectory: String?

    var body: some View {
        NavigationSplitView {
            List {
                ForEach(model.tree.projects) { project in
                    DisclosureGroup {
                        ForEach(project.sessions) { row in sessionButton(row) }
                        ForEach(project.worktrees) { worktree in
                            DisclosureGroup {
                                ForEach(worktree.sessions) { row in sessionButton(row) }
                            } label: {
                                rowLabel(
                                    URL(fileURLWithPath: worktree.entry.path).lastPathComponent,
                                    directory: worktree.entry.path
                                )
                            }
                        }
                    } label: {
                        rowLabel(project.name, directory: project.launchDirectory)
                    }
                }
            }
            .navigationTitle("Sessions")
            .navigationSplitViewColumnWidth(min: 240, ideal: 300)
            .simultaneousGesture(TapGesture().onEnded {
                NSApp.keyWindow?.makeFirstResponder(nil)
                panes.clearFocus()
            })
            .toolbar {
                Button {
                    if KeyRouting.route(focus: .sidebar, key: .commandN) == .openNewChat,
                       let selectedID = model.selectedID {
                        newChatDirectory = model.tree.launchDirectory(for: selectedID)
                    }
                } label: {
                    Image(systemName: "plus.circle")
                }
                .help("New chat")
                .accessibilityLabel("New chat")
                .keyboardShortcut("n", modifiers: .command)
                .disabled(model.selectedID == nil)
            }
        } detail: {
            if let row = model.selectedSession {
                SessionDetailView(
                    row: row, title: model.tree.windowTitle(for: model.selectedID ?? row.id) ?? row.title,
                    agents: model.agents, panes: panes
                )
                    .id(row.id)
            } else if let error = model.error {
                ContentUnavailableView(error, systemImage: "exclamationmark.triangle")
            } else {
                ContentUnavailableView("Select a session", systemImage: "square.stack")
            }
        }
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

    private func rowLabel(_ name: String, directory: String) -> some View {
        HStack {
            Text(name)
                .lineLimit(1)
                .truncationMode(.middle)
            Spacer()
            Button { newChatDirectory = directory } label: {
                Image(systemName: "plus.circle")
            }
            .buttonStyle(.borderless)
            .help("New chat")
            .accessibilityLabel("New chat")
        }
    }

    private func sessionButton(_ row: SwarmProjectSession) -> some View {
        Button {
            NSApp.keyWindow?.makeFirstResponder(nil)
            panes.clearFocus()
            model.select(row.id)
        } label: {
            Text(SessionsTree.rowText(row, now: Int(Date().timeIntervalSince1970)))
                .lineLimit(1)
                .truncationMode(.middle)
        }
        .buttonStyle(.plain)
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
