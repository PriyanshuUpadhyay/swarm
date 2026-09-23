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

public struct WorktreeNode: Sendable, Hashable, Identifiable {
    public var id: String { entry.path }
    public let entry: WorktreeEntry
    public let sessions: [SwarmProjectSession]

    public init(entry: WorktreeEntry, sessions: [SwarmProjectSession]) {
        self.entry = entry
        self.sessions = sessions
    }
}

public struct ProjectNode: Sendable, Hashable, Identifiable {
    public let id: SwarmPathIdentity
    public let path: String
    public let launchDirectory: String
    public let worktrees: [WorktreeNode]
    public let sessions: [SwarmProjectSession]

    public var name: String { URL(fileURLWithPath: path).lastPathComponent }

    public init(
        id: SwarmPathIdentity, path: String, launchDirectory: String,
        worktrees: [WorktreeNode], sessions: [SwarmProjectSession]
    ) {
        self.id = id
        self.path = path
        self.launchDirectory = launchDirectory
        self.worktrees = worktrees
        self.sessions = sessions
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
                    id: identity, path: path, launchDirectory: path, worktrees: [],
                    sessions: rows(sessions, agentsBySession: agentsBySession)
                )
            case .repository(let commonDirectory):
                let listed = worktreeLister(commonDirectory)
                let worktrees = listed.compactMap { entry -> WorktreeNode? in
                    guard !entry.isBare else { return nil }
                    let matches = sessions.filter { contains($0.cwd, in: entry.path) }
                    guard !matches.isEmpty else { return nil }
                    return WorktreeNode(entry: entry, sessions: rows(matches, agentsBySession: agentsBySession))
                }
                let projectSessions = sessions.filter { session in
                    !listed.contains { !$0.isBare && contains(session.cwd, in: $0.path) }
                }
                guard !worktrees.isEmpty || !projectSessions.isEmpty else { return nil }
                let path = [".git", ".bare"].contains(URL(fileURLWithPath: commonDirectory).lastPathComponent)
                    ? URL(fileURLWithPath: commonDirectory).deletingLastPathComponent().path
                    : commonDirectory
                let launchDirectory = listed.first { $0.branch == "main" && !$0.isBare }?.path
                    ?? listed.first { !$0.isBare }?.path ?? path
                return ProjectNode(
                    id: identity, path: path, launchDirectory: launchDirectory,
                    worktrees: worktrees, sessions: rows(projectSessions, agentsBySession: agentsBySession)
                )
            }
        }.sorted { $0.path.localizedStandardCompare($1.path) == .orderedAscending }
        return SessionsTree(projects: projects, agentsBySession: agentsBySession)
    }

    public func session(_ id: SwarmSessionID) -> SwarmProjectSession? {
        for project in projects {
            if let row = SwarmSessionListing.chat(id, in: project.sessions) { return row }
            for worktree in project.worktrees {
                if let row = SwarmSessionListing.chat(id, in: worktree.sessions) { return row }
            }
        }
        return nil
    }

    public func retainedSelection(_ id: SwarmSessionID?) -> SwarmSessionID? {
        guard let id else { return nil }
        return session(id) == nil ? nil : id
    }

    public func windowTitle(for id: SwarmSessionID) -> String? {
        for project in projects {
            guard let row = SwarmSessionListing.chat(id, in: project.sessions)
                ?? project.worktrees.lazy.compactMap({ SwarmSessionListing.chat(id, in: $0.sessions) }).first
            else { continue }
            return "\(project.name) · \(row.provider ?? "no chair") \(id.rawValue.prefix(8))"
        }
        return nil
    }

    public func launchDirectory(for id: SwarmSessionID) -> String? {
        for project in projects {
            if SwarmSessionListing.chat(id, in: project.sessions) != nil { return project.launchDirectory }
            for worktree in project.worktrees {
                if SwarmSessionListing.chat(id, in: worktree.sessions) != nil {
                    return worktree.entry.path
                }
            }
        }
        return nil
    }

    public func text(now: Int = Int(Date().timeIntervalSince1970)) -> String {
        var lines: [String] = []
        for project in projects {
            lines.append(project.name)
            for row in project.sessions { lines.append("  " + Self.rowText(row, now: now)) }
            for worktree in project.worktrees {
                lines.append("  " + URL(fileURLWithPath: worktree.entry.path).lastPathComponent)
                for row in worktree.sessions { lines.append("    " + Self.rowText(row, now: now)) }
            }
        }
        return lines.joined(separator: "\n")
    }

    public static func rowText(_ row: SwarmProjectSession, now: Int) -> String {
        let presentation = SessionRowPresentation.make(row, now: now)
        return "\(presentation.title) · \(presentation.caption)"
    }

    public static func ordered(_ rows: [SwarmProjectSession]) -> [SwarmProjectSession] {
        rows.sorted { lhs, rhs in
            let left = order(SessionRowPresentation.state(of: lhs))
            let right = order(SessionRowPresentation.state(of: rhs))
            return left == right ? lhs.lastActivity > rhs.lastActivity : left < right
        }
    }

    private static func rows(
        _ sessions: [SwarmSession], agentsBySession: [SwarmSessionID: [SwarmAgent]]
    ) -> [SwarmProjectSession] {
        ordered(SwarmSessionListing.chatGroups(sessions).map {
            let known = $0.allSatisfy { agentsBySession[$0.id] != nil }
            let agents = $0.flatMap { agentsBySession[$0.id] ?? [] }
            let running = known ? agents.contains(where: { $0.alive == true }) : nil
            let provider = $0[0].chairProvider
                ?? agents.first(where: { $0.id == SwarmPanePolicy.chair })?.provider
                ?? agents.first?.provider
            return SwarmProjectSession(
                sessions: $0, title: "Chat", isRunning: running,
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
        return SessionsTree.build(
            sessions: sessions, agentsBySession: agentsBySession,
            repositoryPathsResolver: Git.repositoryPaths,
            worktreeLister: { listings[$0] ?? [] }
        )
    }
}
