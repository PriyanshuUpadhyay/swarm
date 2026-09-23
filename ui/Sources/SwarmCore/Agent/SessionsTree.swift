import Foundation

public struct SessionRowPresentation: Sendable, Hashable {
    public enum State: Sendable, Hashable { case live, ended, noChair }

    public var title: String
    public var caption: String
    public var state: State

    public static func make(_ row: SwarmProjectSession, now: Int) -> SessionRowPresentation {
        let state = state(of: row)
        let age = max(0, now - row.session.createdAt)
        let ageText: String
        if age < 60 { ageText = "\(age)s" }
        else if age < 3_600 { ageText = "\(age / 60)m" }
        else if age < 86_400 { ageText = "\(age / 3_600)h" }
        else { ageText = "\(age / 86_400)d" }

        let fallback = "\(row.provider ?? "Chat") \(row.id.rawValue.prefix(8))"
        let title = row.title.isEmpty ? fallback : row.title
        var caption = "\(state == .ended ? "ended" : row.provider ?? "no chair") · \(ageText)"
        if let liveAgents = row.liveAgents {
            let children = state == .live && row.isRunning == true
                ? max(0, liveAgents - 1) : liveAgents
            if children > 0 { caption += " · \(children) \(children == 1 ? "agent" : "agents")" }
        }
        return SessionRowPresentation(title: title, caption: caption, state: state)
    }

    fileprivate static func state(of row: SwarmProjectSession) -> State {
        if row.isRunning == false { return .ended }
        if row.provider == nil { return .noChair }
        return .live
    }
}

public struct WorkspaceNode: Sendable, Hashable, Identifiable {
    public var id: String { path }
    public let path: String
    public let name: String
    public let sessions: [SwarmProjectSession]
    public var current: SwarmProjectSession? { sessions.first }
    public var state: SessionRowPresentation.State? {
        current.map(SessionRowPresentation.state)
    }

    public init(path: String, name: String, sessions: [SwarmProjectSession]) {
        self.path = path
        self.name = name
        self.sessions = sessions
    }
}

public struct ProjectNode: Sendable, Hashable, Identifiable {
    public let id: SwarmPathIdentity
    public let path: String
    public let launchDirectory: String
    public let workspaces: [WorkspaceNode]

    public var name: String { URL(fileURLWithPath: path).lastPathComponent }

    public init(
        id: SwarmPathIdentity, path: String, launchDirectory: String,
        workspaces: [WorkspaceNode]
    ) {
        self.id = id
        self.path = path
        self.launchDirectory = launchDirectory
        self.workspaces = workspaces
    }
}

/// The same ordered value feeds the sidebar and the command-line tree.
public struct SessionsTree: Sendable, Hashable {
    public let projects: [ProjectNode]
    public let agentsBySession: [SwarmSessionID: [SwarmAgent]]

    public init(
        projects: [ProjectNode], agentsBySession: [SwarmSessionID: [SwarmAgent]] = [:]
    ) {
        self.projects = projects
        self.agentsBySession = agentsBySession
    }

    public static func build(
        sessions: [SwarmSession],
        agentsBySession: [SwarmSessionID: [SwarmAgent]] = [:],
        titles: [SwarmSessionID: String] = [:],
        repositoryPathsResolver: (String) -> GitRepositoryPaths?,
        worktreeLister: (String) -> [WorktreeEntry]
    ) -> SessionsTree {
        var groups: [SwarmPathIdentity: [SwarmSession]] = [:]
        for session in sessions where session.archivedAt == nil {
            let identity = SwarmSessionDiscovery.identity(
                for: session.cwd, repositoryPathsResolver: repositoryPathsResolver
            )
            groups[identity, default: []].append(session)
        }

        let projects = groups.compactMap { identity, sessions -> ProjectNode? in
            switch identity {
            case .folder(let path):
                return ProjectNode(
                    id: identity, path: path, launchDirectory: path,
                    workspaces: [WorkspaceNode(
                        path: path, name: URL(fileURLWithPath: path).lastPathComponent,
                        sessions: rows(sessions, agentsBySession: agentsBySession, titles: titles)
                    )]
                )
            case .repository(let commonDirectory):
                let listed = worktreeLister(commonDirectory)
                var workspaces = listed.compactMap { entry -> WorkspaceNode? in
                    guard !entry.isBare else { return nil }
                    let matches = sessions.filter { contains($0.cwd, in: entry.path) }
                    guard !matches.isEmpty else { return nil }
                    return WorkspaceNode(
                        path: entry.path,
                        name: entry.branch ?? URL(fileURLWithPath: entry.path).lastPathComponent,
                        sessions: rows(matches, agentsBySession: agentsBySession, titles: titles)
                    )
                }
                let hubSessions = sessions.filter { session in
                    !listed.contains { !$0.isBare && contains(session.cwd, in: $0.path) }
                }
                let path = [".git", ".bare"].contains(URL(fileURLWithPath: commonDirectory).lastPathComponent)
                    ? URL(fileURLWithPath: commonDirectory).deletingLastPathComponent().path
                    : commonDirectory
                if !hubSessions.isEmpty {
                    let hubPath = URL(fileURLWithPath: commonDirectory).lastPathComponent == ".bare"
                        ? commonDirectory : path
                    workspaces.append(WorkspaceNode(
                        path: hubPath, name: URL(fileURLWithPath: hubPath).lastPathComponent,
                        sessions: rows(hubSessions, agentsBySession: agentsBySession, titles: titles)
                    ))
                }
                guard !workspaces.isEmpty else { return nil }
                let launchDirectory = listed.first { $0.branch == "main" && !$0.isBare }?.path
                    ?? listed.first { !$0.isBare }?.path ?? path
                return ProjectNode(
                    id: identity, path: path, launchDirectory: launchDirectory,
                    workspaces: ordered(workspaces)
                )
            }
        }.sorted { $0.path.localizedStandardCompare($1.path) == .orderedAscending }
        return SessionsTree(projects: projects, agentsBySession: agentsBySession)
    }

    public func workspace(_ id: String) -> WorkspaceNode? {
        for project in projects {
            if let workspace = project.workspaces.first(where: { $0.id == id }) { return workspace }
        }
        return nil
    }

    public func session(_ id: String) -> SwarmProjectSession? {
        workspace(id)?.current
    }

    public func retainedSelection(_ id: String?) -> String? {
        guard let id else { return nil }
        return workspace(id) == nil ? nil : id
    }

    public func windowTitle(for id: String) -> String? {
        for project in projects {
            guard let workspace = project.workspaces.first(where: { $0.id == id }),
                  let row = workspace.current else { continue }
            return "\(project.name) · \(row.provider ?? "no chair") \(row.id.rawValue.prefix(8))"
        }
        return nil
    }

    public func launchDirectory(for id: String) -> String? {
        workspace(id)?.path
    }

    public func text(now: Int = Int(Date().timeIntervalSince1970)) -> String {
        var lines: [String] = []
        for project in projects {
            lines.append(project.name)
            for workspace in project.workspaces {
                if let row = workspace.current {
                    lines.append("  \(workspace.name) · \(SessionRowPresentation.make(row, now: now).caption)")
                } else {
                    lines.append("  \(workspace.name)")
                }
            }
        }
        return lines.joined(separator: "\n")
    }

    public static func ordered(_ rows: [SwarmProjectSession]) -> [SwarmProjectSession] {
        rows.sorted { lhs, rhs in
            let left = order(SessionRowPresentation.state(of: lhs))
            let right = order(SessionRowPresentation.state(of: rhs))
            return left == right ? lhs.lastActivity > rhs.lastActivity : left < right
        }
    }

    public static func ordered(_ workspaces: [WorkspaceNode]) -> [WorkspaceNode] {
        workspaces.sorted { lhs, rhs in
            let left = lhs.state.map(order) ?? 3
            let right = rhs.state.map(order) ?? 3
            if left != right { return left < right }
            let leftActivity = lhs.current?.lastActivity ?? 0
            let rightActivity = rhs.current?.lastActivity ?? 0
            if leftActivity != rightActivity { return leftActivity > rightActivity }
            return lhs.name.localizedStandardCompare(rhs.name) == .orderedAscending
        }
    }

    private static func rows(
        _ sessions: [SwarmSession], agentsBySession: [SwarmSessionID: [SwarmAgent]],
        titles: [SwarmSessionID: String]
    ) -> [SwarmProjectSession] {
        ordered(SwarmSessionListing.chatGroups(sessions).map {
            let known = $0.allSatisfy { agentsBySession[$0.id] != nil }
            let agents = $0.flatMap { agentsBySession[$0.id] ?? [] }
            let running = known ? agents.contains(where: { $0.alive == true }) : nil
            let provider = $0[0].chairProvider
                ?? agents.first(where: { $0.id == SwarmPanePolicy.chair })?.provider
                ?? agents.first?.provider
            return SwarmProjectSession(
                sessions: $0, title: $0.lazy.compactMap { titles[$0.id] }.first ?? "Chat",
                isRunning: running,
                liveAgents: known ? agents.filter { $0.alive == true }.count : nil,
                totalAgents: known ? agents.count : nil, provider: provider
            )
        })
    }

    private static func order(_ state: SessionRowPresentation.State) -> Int {
        switch state {
        case .live: 0
        case .noChair: 1
        case .ended: 2
        }
    }

    private static func contains(_ path: String, in root: String) -> Bool {
        let path = URL(fileURLWithPath: path).standardized.path
        let root = URL(fileURLWithPath: root).standardized.path
        return path == root || path.hasPrefix(root + "/")
    }
}

extension SwarmSessionDiscovery {
    public func tree(sessions: [SwarmSession], bus: any SwarmBus) async throws -> SessionsTree {
        var listings: [String: [WorktreeEntry]] = [:]
        var agentsBySession: [SwarmSessionID: [SwarmAgent]] = [:]
        for session in sessions where session.archivedAt == nil {
            if session.agents == 0 {
                agentsBySession[session.id] = []
            } else if let agents = try? await bus.agents(in: session) {
                agentsBySession[session.id] = agents
            }
            guard case .repository(let common) = identity(for: session.cwd), listings[common] == nil else {
                continue
            }
            listings[common] = try await Git.worktrees(of: common)
        }
        let titles = await resolvedTitles(sessions: sessions, agentsBySession: agentsBySession)
        return SessionsTree.build(
            sessions: sessions, agentsBySession: agentsBySession, titles: titles,
            repositoryPathsResolver: Git.repositoryPaths,
            worktreeLister: { listings[$0] ?? [] }
        )
    }
}
