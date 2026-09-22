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
    var agents: [SwarmAgent] = []
    var error: String?

    var selectedSession: SwarmProjectSession? {
        selectedID.flatMap { tree.session($0) }
    }

    func select(_ id: SwarmSessionID) {
        selectedID = id
        agents = []
    }

    func refresh() async throws {
        let sessions = try await bus.sessions()
        tree = try await discovery.tree(sessions: sessions)
        if let selectedID, let row = tree.session(selectedID) {
            self.selectedID = row.id
            agents = try await bus.agents(in: row.session)
        } else {
            selectedID = nil
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
                                Text(URL(fileURLWithPath: worktree.entry.path).lastPathComponent)
                            }
                        }
                    } label: {
                        Text(project.name)
                    }
                }
            }
            .navigationTitle("Sessions")
        } detail: {
            if let row = model.selectedSession {
                SessionDetailView(row: row, agents: model.agents, panes: panes)
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
    }

    private func sessionButton(_ row: SwarmProjectSession) -> some View {
        Button {
            model.select(row.id)
        } label: {
            Text(SessionsTree.rowText(row, now: Int(Date().timeIntervalSince1970)))
        }
        .buttonStyle(.plain)
    }
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
        } else {
            SwarmApp.main()
        }
    }

    private static func printTranscript(prefix: String) async {
        do {
            let session = try await matchingSession(prefix: prefix)
            let snapshot = await SwarmChairTranscript().poll(session: session)
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
