import Foundation

public struct SessionRowPresentation: Sendable, Hashable {
    public enum State: Sendable, Hashable { case live, ended, noChair }

    public var title: String
    public var caption: String
    public var age: String
    public var state: State
    public var provider: String?

    public static func make(_ row: ChatRow, now: Int, appName: String? = nil) -> SessionRowPresentation {
        let state = state(of: row.session)
        let age = max(0, now - row.session.lastActivity)
        let ageText: String
        if age < 60 { ageText = "\(age)s" }
        else if age < 3_600 { ageText = "\(age / 60)m" }
        else if age < 86_400 { ageText = "\(age / 3_600)h" }
        else { ageText = "\(age / 86_400)d" }

        let title = ChatTitle.title(row.session, appName: appName)
        let status = state == .ended ? "ended" : row.session.provider ?? "no chair"
        return SessionRowPresentation(
            title: title, caption: "\(row.workspace) · \(status)", age: ageText,
            state: state, provider: row.session.provider
        )
    }

    fileprivate static func state(of row: SwarmProjectSession) -> State {
        if row.isRunning == false { return .ended }
        if row.provider == nil { return .noChair }
        return .live
    }
}

public struct ChatRow: Sendable, Hashable, Identifiable {
    public var id: SwarmSessionID { session.id }
    public let session: SwarmProjectSession
    public let workspace: String
    public let workspacePath: String

    public init(session: SwarmProjectSession, workspace: String, workspacePath: String) {
        self.session = session
        self.workspace = workspace
        self.workspacePath = workspacePath
    }
}

public struct WorkspaceNode: Sendable, Hashable, Identifiable {
    public enum Mark: String, Sendable, Hashable { case locked, detached }

    public static func removedPath(for projectPath: String) -> String { projectPath + "#removed" }

    public var id: String { path }
    public let path: String
    public let name: String
    public let branch: String?
    public let missing: Bool
    public let mark: Mark?
    public let isRemoved: Bool
    public var canStartChat: Bool { !missing && !isRemoved }
    public var sessions: [SwarmProjectSession]

    public init(
        path: String, name: String, sessions: [SwarmProjectSession], branch: String? = nil,
        missing: Bool = false, mark: Mark? = nil, isRemoved: Bool = false
    ) {
        self.path = path
        self.name = name
        self.branch = branch
        self.missing = missing
        self.mark = mark
        self.isRemoved = isRemoved
        self.sessions = sessions
    }
}

public struct ProjectNode: Sendable, Hashable, Identifiable {
    public let id: SwarmPathIdentity
    public let path: String
    public let launchDirectory: String
    public let workspaces: [WorkspaceNode]

    public var name: String { URL(fileURLWithPath: path).lastPathComponent }

    public static func projectPath(for identity: SwarmPathIdentity) -> String {
        switch identity {
        case .folder(let path): return path
        case .repository(let commonDirectory):
            return [".git", ".bare"].contains(URL(fileURLWithPath: commonDirectory).lastPathComponent)
                ? URL(fileURLWithPath: commonDirectory).deletingLastPathComponent().path
                : commonDirectory
        }
    }
    public var mainWorkspacePath: String? { Self.mainWorkspacePath(id, workspaces: workspaces) }

    static func mainWorkspacePath(_ identity: SwarmPathIdentity, workspaces: [WorkspaceNode]) -> String? {
        let path = projectPath(for: identity)
        if case .repository(let common) = identity, common != path + "/.git" {
            return workspaces.first { $0.branch == "main" }?.path
                ?? workspaces.first { $0.branch == "master" }?.path
        }
        return path
    }

    public var chats: [ChatRow] {
        SessionsTree.ordered(workspaces.flatMap { workspace in
            workspace.sessions.compactMap { session in
                guard session.totalAgents != 0 else { return nil }
                return ChatRow(
                    session: session, workspace: workspace.name, workspacePath: workspace.path
                )
            }
        })
    }

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
        projectPaths: [String] = [],
        removed: Set<String> = [],
        agentsBySession: [SwarmSessionID: [SwarmAgent]] = [:],
        titles: [SwarmSessionID: String] = [:],
        cliNames: [SwarmSessionID: String] = [:],
        chairLogs: [SwarmSessionID: String] = [:],
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
        var openedProjects: [SwarmPathIdentity: String] = [:]
        for path in projectPaths {
            let identity = SwarmSessionDiscovery.identity(
                for: path, repositoryPathsResolver: repositoryPathsResolver
            )
            openedProjects[identity] = path
        }
        for identity in openedProjects.keys where groups[identity] == nil {
            groups[identity] = []
        }

        let projects = groups.compactMap { identity, sessions -> ProjectNode? in
            switch identity {
            case .folder(let path):
                return ProjectNode(
                    id: identity, path: path, launchDirectory: path,
                    workspaces: [WorkspaceNode(
                        path: path, name: URL(fileURLWithPath: path).lastPathComponent,
                        sessions: rows(sessions, agentsBySession: agentsBySession, titles: titles, cliNames: cliNames, chairLogs: chairLogs)
                    )]
                )
            case .repository(let commonDirectory):
                let listed = worktreeLister(commonDirectory)
                var workspaces = listed.compactMap { entry -> WorkspaceNode? in
                    guard !entry.isBare else { return nil }
                    let matches = sessions.filter { contains($0.cwd, in: entry.path) }
                    guard !matches.isEmpty || entry.isPrunable || openedProjects[identity] != nil else { return nil }
                    return WorkspaceNode(
                        path: entry.path,
                        name: entry.branch ?? URL(fileURLWithPath: entry.path).lastPathComponent,
                        sessions: rows(matches, agentsBySession: agentsBySession, titles: titles, cliNames: cliNames, chairLogs: chairLogs),
                        branch: entry.branch, missing: entry.isPrunable,
                        mark: entry.isLocked ? .locked : entry.isDetached ? .detached : nil
                    )
                }
                let unmatched = sessions.filter { session in
                    !listed.contains { !$0.isBare && contains(session.cwd, in: $0.path) }
                }
                let path = ProjectNode.projectPath(for: identity)
                let removedSessions = unmatched.filter { session in
                    let cwd = URL(fileURLWithPath: session.cwd).standardized.path
                    return cwd != path && cwd != commonDirectory && !FileManager.default.fileExists(atPath: cwd)
                }
                let removedIDs = Set(removedSessions.map(\.id))
                let hubSessions = unmatched.filter { !removedIDs.contains($0.id) }
                if !removedSessions.isEmpty {
                    workspaces.append(WorkspaceNode(
                        path: WorkspaceNode.removedPath(for: path), name: "Removed worktrees",
                        sessions: rows(removedSessions, agentsBySession: agentsBySession, titles: titles, cliNames: cliNames, chairLogs: chairLogs),
                        isRemoved: true
                    ))
                }
                if !hubSessions.isEmpty {
                    workspaces.append(WorkspaceNode(
                        path: path, name: URL(fileURLWithPath: path).lastPathComponent,
                        sessions: rows(hubSessions, agentsBySession: agentsBySession, titles: titles, cliNames: cliNames, chairLogs: chairLogs)
                    ))
                }
                guard !workspaces.isEmpty || openedProjects[identity] != nil else { return nil }
                let launchDirectory = openedProjects[identity]
                    ?? listed.first { $0.branch == "main" && !$0.isBare }?.path
                    ?? listed.first { !$0.isBare }?.path ?? path
                return ProjectNode(
                    id: identity, path: path, launchDirectory: launchDirectory,
                    workspaces: ordered(workspaces, mainPath: ProjectNode.mainWorkspacePath(identity, workspaces: workspaces), hubPath: path)
                )
            }
        }.filter { !removed.contains($0.path) && (!$0.chats.isEmpty || openedProjects[$0.id] != nil) }
            .sorted { $0.path.localizedStandardCompare($1.path) == .orderedAscending }
        return SessionsTree(projects: projects, agentsBySession: agentsBySession)
    }

    public func session(_ id: SwarmSessionID) -> SwarmProjectSession? {
        chat(id)?.session
    }

    public func archiveIDs(for id: SwarmSessionID) -> [SwarmSessionID] {
        chat(id).map { SwarmSessionListing.archiveIDs(for: $0.session) } ?? []
    }

    public func retainedSelection(_ id: SwarmSessionID?) -> SwarmSessionID? {
        guard let id else { return nil }
        return chat(id)?.id
    }

    public func windowTitle(for id: SwarmSessionID, navigation: WorkspaceNavigation) -> String? {
        for project in projects {
            guard let row = project.chats.first(where: {
                $0.session.sessions.contains { $0.id == id }
            }) else { continue }
            let title = navigation.title(for: row.session)
            return "\(navigation.projectTitle(for: project)) · \(title)"
        }
        return nil
    }

    /// The project with the deepest workspace at or above `path`, so a repository inside a
    /// plain-folder project wins over that folder; else the project rooted at `path`.
    public func project(containing path: String) -> ProjectNode? {
        projects.flatMap { project in project.workspaces.map { (project, $0.path) } }
            .filter { Self.contains(path, in: $0.1) }
            .max { $0.1.count < $1.1.count }?.0
            ?? projects.first { $0.path == path }
    }

    public func launchDirectory(for id: SwarmSessionID) -> String? {
        chat(id)?.workspacePath
    }

    public func workspaceChats(for id: SwarmSessionID) -> [ChatRow] {
        for project in projects {
            let chats = project.chats
            guard let selected = chats.first(where: { $0.id == id }) else { continue }
            return chats.filter { $0.workspacePath == selected.workspacePath }
        }
        return []
    }

    public func text(now: Int = Int(Date().timeIntervalSince1970)) -> String {
        var lines: [String] = []
        for project in projects {
            lines.append(project.name)
            for row in project.chats {
                let presentation = SessionRowPresentation.make(row, now: now)
                lines.append("  \(presentation.title) · \(presentation.caption) · \(presentation.age)")
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

    public static func ordered(_ rows: [ChatRow]) -> [ChatRow] {
        rows.sorted { lhs, rhs in
            let left = order(SessionRowPresentation.state(of: lhs.session))
            let right = order(SessionRowPresentation.state(of: rhs.session))
            return left == right
                ? lhs.session.lastActivity > rhs.session.lastActivity : left < right
        }
    }

    /// Stored paths lead; other rows keep a fixed name order. Hub and removed rows follow checkouts.
    public static func ordered(
        _ workspaces: [WorkspaceNode], order: [String] = [], mainPath: String? = nil, hubPath: String? = nil
    ) -> [WorkspaceNode] {
        let positions = Dictionary(order.enumerated().map { ($0.element, $0.offset) }, uniquingKeysWith: min)
        func group(_ workspace: WorkspaceNode) -> Int {
            if workspace.isRemoved { return 2 }
            return workspace.path == hubPath && workspace.path != mainPath ? 1 : 0
        }
        return workspaces.sorted { lhs, rhs in
            let left = positions[lhs.path] ?? Int.max
            let right = positions[rhs.path] ?? Int.max
            if left != right { return left < right }
            if group(lhs) != group(rhs) { return group(lhs) < group(rhs) }
            if (lhs.path == mainPath) != (rhs.path == mainPath) { return lhs.path == mainPath }
            let nameOrder = lhs.name.localizedStandardCompare(rhs.name)
            return nameOrder == .orderedSame ? lhs.path < rhs.path : nameOrder == .orderedAscending
        }
    }

    private static func rows(
        _ sessions: [SwarmSession], agentsBySession: [SwarmSessionID: [SwarmAgent]],
        titles: [SwarmSessionID: String], cliNames: [SwarmSessionID: String], chairLogs: [SwarmSessionID: String]
    ) -> [SwarmProjectSession] {
        ordered(SwarmSessionListing.chatGroups(sessions).map {
            let known = $0.allSatisfy { agentsBySession[$0.id] != nil }
            let agents = $0.flatMap { agentsBySession[$0.id] ?? [] }
            let running = known
                ? agentsBySession[$0[0].id]?.contains(where: { $0.alive == true })
                : nil
            let provider = $0[0].chairProvider
                ?? agents.first(where: { $0.id == SwarmPanePolicy.chair })?.provider
                ?? agents.first?.provider
            return SwarmProjectSession(
                sessions: $0, title: ChatTitle.resolve(
                    appName: nil, cliName: nil,
                    firstLine: $0.reversed().lazy.compactMap { titles[$0.id] }.first,
                    provider: provider, id: $0[0].id
                ),
                isRunning: running,
                liveAgents: known ? agents.filter { $0.alive == true }.count : nil,
                totalAgents: known ? agents.count : nil, provider: provider,
                status: agentsBySession[$0[0].id].flatMap { AgentStatus.aggregate($0.map(\.status)) },
                statusCounts: agentsBySession[$0[0].id].map { Dictionary($0.map { ($0.status, 1) }, uniquingKeysWith: +) } ?? [:],
                cliName: $0.lazy.compactMap { cliNames[$0.id] }.first, resolvedChairLog: chairLogs[$0[0].id]
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

    private func chat(_ id: SwarmSessionID) -> ChatRow? {
        projects.lazy.flatMap(\.chats).first {
            $0.session.sessions.contains { $0.id == id }
        }
    }

    private static func contains(_ path: String, in root: String) -> Bool {
        let path = URL(fileURLWithPath: path).standardized.path
        let root = URL(fileURLWithPath: root).standardized.path
        return path == root || path.hasPrefix(root + "/")
    }
}

extension SwarmSessionDiscovery {
    public func tree(
        sessions: [SwarmSession], projectPaths: [String] = [], removed: Set<String> = [], bus: any SwarmBus
    ) async throws -> SessionsTree {
        var listings: [String: [WorktreeEntry]] = [:]
        var agentsBySession: [SwarmSessionID: [SwarmAgent]] = [:]
        do {
            let timing = SwarmPerformance.begin("SessionDiscovery")
            defer { timing.end(count: agentsBySession.count) }
            // A `swarm` older than this app has no `--all`, so a failed batch reads each session.
            let listingsBySession = try? await bus.agentsBySession()
            for session in sessions where session.archivedAt == nil {
                if session.agents == 0 {
                    agentsBySession[session.id] = []
                } else if let listingsBySession {
                    if (try? SwarmSessionInteraction.adapter(for: session)) != nil,
                       let agents = listingsBySession[session.id] {
                        agentsBySession[session.id] = agents
                    }
                } else if let agents = try? await bus.agents(in: session) {
                    agentsBySession[session.id] = agents
                }
                guard case .repository(let common) = identity(for: session.cwd), listings[common] == nil else {
                    continue
                }
                listings[common] = try await worktrees(for: common)
            }
        }
        do {
            let timing = SwarmPerformance.begin("ProjectDiscovery")
            defer { timing.end(count: projectPaths.count) }
            for path in projectPaths {
                guard case .repository(let common) = identity(for: path), listings[common] == nil else {
                    continue
                }
                listings[common] = try await worktrees(for: common)
            }
        }
        let titleTiming = SwarmPerformance.begin("TitleResolution")
        let titles = await resolvedTitles(sessions: sessions, agentsBySession: agentsBySession)
        let cliNames = await resolvedCLINames(sessions: sessions, agentsBySession: agentsBySession)
        titleTiming.end(count: titles.count)
        let buildTiming = SwarmPerformance.begin("TreeBuild")
        let tree = SessionsTree.build(
            sessions: sessions, projectPaths: projectPaths, removed: removed,
            agentsBySession: agentsBySession, titles: titles, cliNames: cliNames, chairLogs: resolvedChairLogs,
            repositoryPathsResolver: Git.repositoryPaths,
            worktreeLister: { listings[$0] ?? [] }
        )
        buildTiming.end(count: tree.projects.count)
        return tree
    }
}
