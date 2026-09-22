import Foundation

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
    public let worktrees: [WorktreeNode]
    public let sessions: [SwarmProjectSession]

    public var name: String { URL(fileURLWithPath: path).lastPathComponent }

    public init(
        id: SwarmPathIdentity, path: String,
        worktrees: [WorktreeNode], sessions: [SwarmProjectSession]
    ) {
        self.id = id
        self.path = path
        self.worktrees = worktrees
        self.sessions = sessions
    }
}

/// The same ordered value feeds the sidebar and the command-line tree.
public struct SessionsTree: Sendable, Hashable {
    public let projects: [ProjectNode]

    public init(projects: [ProjectNode]) { self.projects = projects }

    public static func build(
        sessions: [SwarmSession],
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
                    id: identity, path: path, worktrees: [],
                    sessions: rows(sessions)
                )
            case .repository(let commonDirectory):
                let worktrees = worktreeLister(commonDirectory).compactMap { entry -> WorktreeNode? in
                    guard !entry.isBare else { return nil }
                    let matches = sessions.filter { contains($0.cwd, in: entry.path) }
                    guard !matches.isEmpty else { return nil }
                    return WorktreeNode(entry: entry, sessions: rows(matches))
                }
                guard !worktrees.isEmpty else { return nil }
                let path = [".git", ".bare"].contains(URL(fileURLWithPath: commonDirectory).lastPathComponent)
                    ? URL(fileURLWithPath: commonDirectory).deletingLastPathComponent().path
                    : commonDirectory
                return ProjectNode(id: identity, path: path, worktrees: worktrees, sessions: [])
            }
        }.sorted { $0.path.localizedStandardCompare($1.path) == .orderedAscending }
        return SessionsTree(projects: projects)
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

    public func launchDirectory(for id: SwarmSessionID) -> String? {
        for project in projects {
            if SwarmSessionListing.chat(id, in: project.sessions) != nil { return project.path }
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
        let session = row.session
        let age = max(0, now - session.createdAt)
        let ageText: String
        if age < 60 { ageText = "\(age)s" }
        else if age < 3_600 { ageText = "\(age / 60)m" }
        else if age < 86_400 { ageText = "\(age / 3_600)h" }
        else { ageText = "\(age / 86_400)d" }
        let count = row.sessions.reduce(0) { $0 + $1.agents }
        return "\(session.chairProvider ?? "no chair") \(session.id.rawValue.prefix(8)) · \(ageText) · \(count) agents"
    }

    private static func rows(_ sessions: [SwarmSession]) -> [SwarmProjectSession] {
        SwarmSessionListing.chatGroups(sessions).map {
            SwarmProjectSession(sessions: $0, title: "Chat")
        }
    }

    private static func contains(_ path: String, in root: String) -> Bool {
        let path = URL(fileURLWithPath: path).standardized.path
        let root = URL(fileURLWithPath: root).standardized.path
        return path == root || path.hasPrefix(root + "/")
    }
}

extension SwarmSessionDiscovery {
    public func tree(sessions: [SwarmSession]) async throws -> SessionsTree {
        var listings: [String: [WorktreeEntry]] = [:]
        for session in sessions where session.archivedAt == nil {
            guard case .repository(let common) = identity(for: session.cwd), listings[common] == nil else {
                continue
            }
            listings[common] = try await Git.worktrees(of: common)
        }
        return SessionsTree.build(
            sessions: sessions, repositoryPathsResolver: Git.repositoryPaths,
            worktreeLister: { listings[$0] ?? [] }
        )
    }
}
