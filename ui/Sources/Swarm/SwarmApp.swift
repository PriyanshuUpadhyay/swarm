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
                List {
                    LabeledContent("ID", value: row.id.rawValue)
                    LabeledContent("Cwd", value: row.session.cwd)
                    LabeledContent("Chair", value: row.session.chairProvider ?? "No chair")
                    LabeledContent("Created") {
                        Text(Date(timeIntervalSince1970: TimeInterval(row.session.createdAt))
                            .formatted(date: .abbreviated, time: .standard))
                    }
                    Section("Agents") {
                        ForEach(model.agents) { agent in
                            VStack(alignment: .leading) {
                                Text(agent.id.rawValue)
                                Text("\(agent.role) · \(agent.provider ?? "unknown") · \(agent.alive == true ? "pane alive" : "pane not alive")")
                                    .foregroundStyle(.secondary)
                            }
                        }
                    }
                }
            } else if let error = model.error {
                ContentUnavailableView(error, systemImage: "exclamationmark.triangle")
            } else {
                ContentUnavailableView("Select a session", systemImage: "square.stack")
            }
        }
        .task { await model.run() }
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
enum SwarmExecutable {
    static func main() async {
        if CommandLine.arguments.contains("--print-tree") {
            let model = SessionsTreeModel()
            do {
                try await model.refresh()
                let output = model.tree.text()
                if !output.isEmpty { print(output) }
            } catch {
                fputs("\(error)\n", stderr)
                exit(1)
            }
        } else {
            SwarmApp.main()
        }
    }
}
