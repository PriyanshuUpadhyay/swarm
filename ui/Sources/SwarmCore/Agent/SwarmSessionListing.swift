import Foundation

/// A discovered session with the stable title the sidebar draws for it.
public struct SwarmProjectSession: Sendable, Hashable, Identifiable {
    public var session: SwarmSession
    public var title: String

    public var id: SwarmSessionID { session.id }

    public init(session: SwarmSession, title: String) {
        self.session = session
        self.title = title
    }
}

/// The repository fact used to assign a swarm session to a project.
public enum SwarmPathIdentity: Sendable, Hashable {
    case repository(commonDirectory: String)
    case folder(String)
}

/// The pure decisions behind the sessions shown under each project.
public enum SwarmSessionListing {
    public static func grouped(
        sessions: [SwarmSession],
        projects: [(id: RepoID, identity: SwarmPathIdentity)],
        sessionIdentities: [SwarmSessionID: SwarmPathIdentity],
        excluding saved: Set<SwarmSessionID>
    ) -> [RepoID: [SwarmSession]] {
        var answer: [RepoID: [SwarmSession]] = [:]
        for project in projects {
            answer[project.id] = sessions
                .filter { session in
                    guard !saved.contains(session.id),
                          let identity = sessionIdentities[session.id]
                    else { return false }
                    return matches(
                        project: project.identity,
                        session: identity,
                        sessionPath: session.cwd
                    )
                }
                .sorted(by: newer)
        }
        return answer
    }

    public static func matches(
        project: SwarmPathIdentity,
        session: SwarmPathIdentity,
        sessionPath: String
    ) -> Bool {
        switch project {
        case .repository(let projectCommon):
            guard case .repository(let sessionCommon) = session else { return false }
            return projectCommon == sessionCommon
        case .folder(let projectPath):
            return contains(sessionPath, in: projectPath)
        }
    }

    private static func newer(_ lhs: SwarmSession, _ rhs: SwarmSession) -> Bool {
        if lhs.createdAt != rhs.createdAt { return lhs.createdAt > rhs.createdAt }
        return (Int(lhs.id.rawValue) ?? 0) > (Int(rhs.id.rawValue) ?? 0)
    }

    private static func contains(_ path: String, in folder: String) -> Bool {
        let child = URL(fileURLWithPath: path).standardized.path
        let root = URL(fileURLWithPath: folder).standardized.path
        return child == root || child.hasPrefix(root.hasSuffix("/") ? root : root + "/")
    }
}

/// The first prompt's first line, capped for one sidebar row.
public enum SwarmSessionTitle {
    public static let limit = 80

    public static func make(sessionID: SwarmSessionID, firstUserPrompt: String?) -> String {
        let firstLine = firstUserPrompt?
            .trimmingCharacters(in: .whitespacesAndNewlines)
            .split(omittingEmptySubsequences: false, whereSeparator: \Character.isNewline)
            .first
            .map(String.init)?
            .trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        guard !firstLine.isEmpty else { return "Session \(sessionID.rawValue)" }
        guard firstLine.count > limit else { return firstLine }
        return String(firstLine.prefix(limit - 1)).trimmingCharacters(in: .whitespaces) + "…"
    }
}

public enum SwarmSessionInputTarget: Sendable, Equatable {
    case chair
    case agent
}

/// The shared rules for reaching panes in a discovered session.
public enum SwarmSessionInteraction {
    public static let workspaceAdapter = "tmux-solo"
    public static let missingAdapterSentence =
        "This session does not record an adapter, so Swarm cannot reach its panes."

    public static func adapter(for session: SwarmSession) throws -> String {
        guard let adapter = session.adapter?.trimmingCharacters(in: .whitespacesAndNewlines),
              !adapter.isEmpty else {
            throw SwarmProfileError.failed(missingAdapterSentence)
        }
        return adapter
    }

    public static func disabledReason(
        adapter: String?, pane: String?, target: SwarmSessionInputTarget
    ) -> String? {
        guard let adapter,
              !adapter.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
            return missingAdapterSentence
        }
        guard pane != nil else {
            return target == .chair
                ? "The chair has no pane to receive input."
                : "This agent has no pane to receive input."
        }
        return nil
    }

    public static func canSubmit(
        _ text: String, adapter: String?, pane: String?, target: SwarmSessionInputTarget
    ) -> Bool {
        disabledReason(adapter: adapter, pane: pane, target: target) == nil
            && text.contains { !$0.isWhitespace }
    }

    public static func lastActivity(of session: SwarmSession) -> Int {
        session.lastMessageAt ?? session.createdAt
    }
}

/// One agent's read-only summary and its asks and summaries in bus order.
public struct SwarmSessionAgentDigest: Sendable, Hashable, Identifiable {
    public var id: SwarmAgentID { agent.id }
    public var agent: SwarmAgent
    public var latestSummary: String?
    public var conversation: [SwarmMessage]

    public init(agent: SwarmAgent, latestSummary: String?, conversation: [SwarmMessage]) {
        self.agent = agent
        self.latestSummary = latestSummary
        self.conversation = conversation
    }
}

public enum SwarmSessionAgents {
    public static func digests(
        agents: [SwarmAgent], messages: [SwarmMessage]
    ) -> [SwarmSessionAgentDigest] {
        let chair = SwarmAgentID("orchestrator")
        let relevant = messages.filter { ["ask", "summary"].contains($0.kind) }
        return agents
            .filter { $0.id != chair }
            .sorted { $0.id < $1.id }
            .map { agent in
                let conversation = relevant.filter {
                    ($0.sender == chair && $0.recipient == agent.id)
                        || ($0.sender == agent.id && $0.recipient == chair)
                }.sorted { $0.seq < $1.seq }
                let latest = conversation.last {
                    $0.sender == agent.id && $0.recipient == chair && $0.kind == "summary"
                }?.body
                return SwarmSessionAgentDigest(
                    agent: agent,
                    latestSummary: latest,
                    conversation: conversation
                )
            }
    }
}

/// Resolves project and session paths once, off the main actor, then applies the pure listing rule.
public actor SwarmSessionDiscovery {
    private var locations: [String: SwarmPathIdentity] = [:]
    private var titles: [SwarmSessionID: String] = [:]

    public init() {}

    public func discover(
        sessions: [SwarmSession], repos: [Repo], excluding saved: Set<SwarmSessionID>
    ) -> [RepoID: [SwarmProjectSession]] {
        let projects = repos.map { (id: $0.id, identity: identity(for: $0.path)) }
        let sessionIdentities = Dictionary(uniqueKeysWithValues: sessions.map {
            ($0.id, identity(for: $0.cwd))
        })
        let grouped = SwarmSessionListing.grouped(
            sessions: sessions,
            projects: projects,
            sessionIdentities: sessionIdentities,
            excluding: saved
        )
        return grouped.mapValues { matches in
            matches.map { session in
                SwarmProjectSession(session: session, title: title(for: session))
            }
        }
    }

    private func identity(for path: String) -> SwarmPathIdentity {
        let normal = URL(fileURLWithPath: path).resolvingSymlinksInPath().standardized.path
        if let cached = locations[normal] { return cached }

        var directory = URL(fileURLWithPath: normal)
        while true {
            if let paths = Git.repositoryPaths(in: directory.path) {
                let identity = SwarmPathIdentity.repository(commonDirectory: paths.commonDirectory)
                locations[normal] = identity
                return identity
            }
            let parent = directory.deletingLastPathComponent()
            if parent.path == directory.path { break }
            directory = parent
        }

        let identity = SwarmPathIdentity.folder(normal)
        locations[normal] = identity
        return identity
    }

    private func title(for session: SwarmSession) -> String {
        if let cached = titles[session.id] { return cached }
        let prompt = session.chairLog.flatMap(ChairTranscriptOutput.firstUserPrompt)
        let title = SwarmSessionTitle.make(sessionID: session.id, firstUserPrompt: prompt)
        titles[session.id] = title
        return title
    }
}
