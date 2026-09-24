import Foundation

/// A discovered session with the stable title the sidebar draws for it.
public struct SwarmProjectSession: Sendable, Hashable, Identifiable {
    public var sessions: [SwarmSession]
    public var title: String
    public var isRunning: Bool?
    public var liveAgents: Int?
    public var totalAgents: Int
    public var provider: String?

    public var session: SwarmSession { sessions[0] }
    public var id: SwarmSessionID { session.id }
    public var lastActivity: Int {
        sessions.map(SwarmSessionInteraction.lastActivity).max() ?? session.createdAt
    }

    public init(
        sessions: [SwarmSession], title: String,
        isRunning: Bool? = nil, liveAgents: Int? = nil, totalAgents: Int? = nil,
        provider: String? = nil
    ) {
        precondition(!sessions.isEmpty)
        self.sessions = sessions
        self.title = title
        self.isRunning = isRunning
        self.liveAgents = liveAgents
        self.totalAgents = totalAgents ?? sessions.reduce(0) { $0 + $1.agents }
        self.provider = provider ?? sessions[0].chairProvider
    }
}

/// The repository fact used to assign a swarm session to a project.
public enum SwarmPathIdentity: Sendable, Hashable {
    case repository(commonDirectory: String)
    case folder(String)
}

/// The pure decisions behind the sessions shown under each project.
public enum SwarmSessionListing {
    public static func archiveIDs(for chat: SwarmProjectSession) -> [SwarmSessionID] {
        chat.sessions.map(\.id)
    }

    public static func archiveIDs(
        forProjectAt path: String, sessions: [SwarmSession]
    ) -> [SwarmSessionID] {
        sessions.filter { contains($0.cwd, in: path) }.map(\.id)
    }

    /// The chat one swarm session belongs to, by the group's own id or by any session inside it.
    ///
    /// **A chat's id moves, and a selection holding the old one must still find it.** The id is
    /// the newest session of the group, so every `swarm session new` in the same chair renames the
    /// chat. A window that looked the chat up by id alone then found nothing and drew Home.
    public static func chat(
        _ id: SwarmSessionID, in chats: some Sequence<SwarmProjectSession>
    ) -> SwarmProjectSession? {
        var member: SwarmProjectSession?
        for chat in chats {
            if chat.id == id { return chat }
            if member == nil, chat.sessions.contains(where: { $0.id == id }) { member = chat }
        }
        return member
    }

    /// Sessions made by repeated `swarm session new` calls in one chair chat are one chat row.
    public static func chatGroups(_ sessions: [SwarmSession]) -> [[SwarmSession]] {
        var grouped: [String: [SwarmSession]] = [:]
        var ungrouped: [[SwarmSession]] = []
        for session in sessions {
            if let provider = session.chairProvider, let id = session.chairID {
                grouped["chair:\(provider):\(id)", default: []].append(session)
            } else if let log = session.chairLog {
                grouped["log:\(log)", default: []].append(session)
            } else {
                ungrouped.append([session])
            }
        }
        return (Array(grouped.values) + ungrouped)
            .map { $0.sorted { $0.createdAt == $1.createdAt
                ? $0.id.rawValue > $1.id.rawValue : $0.createdAt > $1.createdAt } }
            .sorted { newer($0[0], $1[0]) }
    }

    public static func grouped(
        sessions: [SwarmSession],
        projects: [(id: String, identity: SwarmPathIdentity)],
        sessionIdentities: [SwarmSessionID: SwarmPathIdentity],
        excluding saved: Set<SwarmSessionID>
    ) -> [String: [SwarmSession]] {
        var answer: [String: [SwarmSession]] = [:]
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
        let left = SwarmSessionInteraction.lastActivity(of: lhs)
        let right = SwarmSessionInteraction.lastActivity(of: rhs)
        if left != right { return left > right }
        return lhs.id.rawValue > rhs.id.rawValue
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
        guard !firstLine.isEmpty else { return "Chat" }
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
    public static let defaultAdapter = "tmux-solo"
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
    public var id: String { sessionID.rawValue + ":" + agent.id.rawValue }
    public var sessionID: SwarmSessionID
    public var agent: SwarmAgent
    public var latestSummary: String?
    public var conversation: [SwarmMessage]

    public init(
        sessionID: SwarmSessionID, agent: SwarmAgent,
        latestSummary: String?, conversation: [SwarmMessage]
    ) {
        self.sessionID = sessionID
        self.agent = agent
        self.latestSummary = latestSummary
        self.conversation = conversation
    }
}

public enum SwarmSessionAgents {
    public static func digests(
        sessionID: SwarmSessionID,
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
                    sessionID: sessionID, agent: agent,
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
    private var titleLogs: [SwarmSessionID: String] = [:]
    private var titleMisses: [SwarmSessionID: Date] = [:]
    private let profiles: any SwarmProfileSource
    private let home: URL

    public init(
        profiles: any SwarmProfileSource = SwarmCLIProfileSource(),
        home: URL = FileManager.default.homeDirectoryForCurrentUser
    ) {
        self.profiles = profiles
        self.home = home
    }

    public func discover(
        sessions: [SwarmSession], projects: [String],
        excluding saved: Set<SwarmSessionID> = []
    ) -> [String: [SwarmProjectSession]] {
        let projectIdentities = projects.map { (id: $0, identity: identity(for: $0)) }
        let sessionIdentities = Dictionary(uniqueKeysWithValues: sessions.map {
            ($0.id, identity(for: $0.cwd))
        })
        let grouped = SwarmSessionListing.grouped(
            sessions: sessions,
            projects: projectIdentities,
            sessionIdentities: sessionIdentities,
            excluding: saved
        )
        return grouped.mapValues { matches in
            SwarmSessionListing.chatGroups(matches).map { group in
                SwarmProjectSession(sessions: group, title: title(for: group[0]))
            }
        }
    }

    public func composerCommandSource(
        for session: SwarmSession, provider: String?
    ) async -> ComposerCommandSource {
        let list: SwarmAccountList?
        if let provider, session.chairLog != nil {
            list = try? await profiles.accounts(provider: provider)
        } else {
            list = nil
        }
        return ComposerCommandSource.resolve(
            provider: provider, session: session,
            accounts: list?.accounts ?? [], homeDirectory: home.path
        )
    }

    func identity(for path: String) -> SwarmPathIdentity {
        let normal = URL(fileURLWithPath: path).resolvingSymlinksInPath().standardized.path
        if let cached = locations[normal] { return cached }
        let identity = Self.identity(for: normal, repositoryPathsResolver: Git.repositoryPaths)
        locations[normal] = identity
        return identity
    }

    func resolvedTitles(
        sessions: [SwarmSession], agentsBySession: [SwarmSessionID: [SwarmAgent]]
    ) async -> [SwarmSessionID: String] {
        let now = Date()
        var homesByProvider: [String: [URL]] = [:]

        for session in sessions where session.archivedAt == nil {
            let provider = session.chairProvider ?? agentsBySession[session.id]?
                .first(where: { $0.id == SwarmPanePolicy.chair })?.provider
            let discoversByTime = session.chairLog == nil && session.chairID == nil
                && (provider == "claude" || provider == "codex")
            let busLogChanged = session.chairLog != nil
                && titleLogs[session.id] != session.chairLog
            if !discoversByTime, !busLogChanged, titles[session.id] != nil {
                continue
            }
            if !discoversByTime, !busLogChanged,
               let missedAt = titleMisses[session.id], now.timeIntervalSince(missedAt) < 30 {
                continue
            }
            var log = session.chairLog
            if log == nil, let provider, provider == "claude" || provider == "codex" {
                if homesByProvider[provider] == nil {
                    let accounts = try? await profiles.accounts(provider: provider)
                    homesByProvider[provider] = ChairLogDiscovery.homes(
                        provider: provider,
                        accountHomes: accounts?.accounts.map(\.home) ?? [], userHome: home
                    )
                }
                log = ChairLogDiscovery.path(
                    provider: provider, chairID: session.chairID?.rawValue,
                    cwd: session.cwd, createdAt: session.createdAt,
                    homes: homesByProvider[provider] ?? []
                )?.path
            }
            if titleLogs[session.id] != log {
                titles[session.id] = nil
                titleMisses[session.id] = nil
                titleLogs[session.id] = log
            }
            guard titles[session.id] == nil else { continue }
            guard let prompt = log.flatMap(ChairLogTitle.firstUserPrompt) else {
                titleMisses[session.id] = now
                continue
            }
            titles[session.id] = SwarmSessionTitle.make(
                sessionID: session.id, firstUserPrompt: prompt
            )
            titleMisses[session.id] = nil
        }
        return titles
    }

    public static func identity(
        for path: String, repositoryPathsResolver: (String) -> GitRepositoryPaths?
    ) -> SwarmPathIdentity {
        let normal = URL(fileURLWithPath: path).standardized.path
        var directory = URL(fileURLWithPath: normal)
        while true {
            if let paths = repositoryPathsResolver(directory.path) {
                return .repository(commonDirectory: paths.commonDirectory)
            }
            let parent = directory.deletingLastPathComponent()
            if parent.path == directory.path { return .folder(normal) }
            directory = parent
        }
    }

    private func title(for session: SwarmSession) -> String {
        if let cached = titles[session.id] { return cached }
        guard let prompt = session.chairLog.flatMap(ChairLogTitle.firstUserPrompt) else {
            return "Chat"
        }
        let title = SwarmSessionTitle.make(sessionID: session.id, firstUserPrompt: prompt)
        titles[session.id] = title
        return title
    }
}
