import Foundation

public struct NoticePrefs: Codable, Sendable, Hashable {
    public var startAtLogin = true
    public var post = true
    public var sound = true
    public var done = true
    public var badge = true
    public var mutedProjects: Set<String> = []

    public init() {}

    public init(from decoder: Decoder) throws {
        let values = try decoder.container(keyedBy: CodingKeys.self)
        startAtLogin = try values.decodeIfPresent(Bool.self, forKey: .startAtLogin) ?? startAtLogin
        post = try values.decodeIfPresent(Bool.self, forKey: .post) ?? post
        sound = try values.decodeIfPresent(Bool.self, forKey: .sound) ?? sound
        done = try values.decodeIfPresent(Bool.self, forKey: .done) ?? done
        badge = try values.decodeIfPresent(Bool.self, forKey: .badge) ?? badge
        mutedProjects = try values.decodeIfPresent(Set<String>.self, forKey: .mutedProjects) ?? mutedProjects
    }
}

public struct NoticeEvent: Sendable, Hashable {
    public enum Kind: Sendable, Hashable { case needsInput, done }
    public let kind: Kind
    public let chat: SwarmProjectSession
    public let projectPath: String
    public let agentID: SwarmAgentID?
    public let stateKind: String?

    public init(kind: Kind, chat: SwarmProjectSession, projectPath: String,
                agentID: SwarmAgentID? = nil, stateKind: String? = nil) {
        self.kind = kind
        self.chat = chat
        self.projectPath = projectPath
        self.agentID = agentID
        self.stateKind = stateKind
    }
}

public struct Notice: Sendable, Hashable {
    public let title: String
    public let body: String
    public let sessionID: SwarmSessionID?
    public let agentID: SwarmAgentID?
    public let sound: Bool

    public init(title: String, body: String, sessionID: SwarmSessionID? = nil,
                agentID: SwarmAgentID? = nil, sound: Bool = false) {
        self.title = title
        self.body = body
        self.sessionID = sessionID
        self.agentID = agentID
        self.sound = sound
    }
}

/// One explicit owner notice read and marked posted by `swarm notices --json`.
public struct StoredNotice: Decodable, Sendable {
    public let id: Int64
    public let sessionId: SwarmSessionID?
    public let agentId: SwarmAgentID?
    public let title: String
    public let body: String
}

public enum NoticeRule {
    public static func shouldPost(row: StoredNotice, prefs: NoticePrefs, tree: SessionsTree) -> Notice? {
        let project = tree.projects.first { project in
            project.chats.contains { chat in chat.session.sessions.contains { $0.id == row.sessionId } }
        }
        let state = row.sessionId.flatMap { tree.agentsBySession[$0] }?
            .first { $0.id == row.agentId }?.state
        guard prefs.post, project.map({ !prefs.mutedProjects.contains($0.path) }) ?? true,
              state == "waiting" || prefs.done else { return nil }
        return Notice(title: row.title, body: row.body, sessionID: row.sessionId,
                      agentID: row.agentId, sound: prefs.sound)
    }

    public static func shouldPost(event: NoticeEvent, prefs: NoticePrefs, project: ProjectNode, title: String) -> Notice? {
        guard prefs.post, !prefs.mutedProjects.contains(project.path),
              event.kind != .done || prefs.done else { return nil }
        let body: String
        switch (event.kind, event.stateKind) {
        case (.needsInput, "permission"): body = "\(project.name): waiting on a permission"
        case (.needsInput, "question"): body = "\(project.name): asked a question"
        case (.needsInput, _): body = "\(project.name): waiting on a permission or a question"
        case (.done, _): body = "\(project.name): chat is done"
        }
        return Notice(title: "Swarm — \(title)", body: body,
                      sessionID: event.chat.id, agentID: event.agentID, sound: prefs.sound)
    }

    public static func transitions(previous: SessionsTree, current: SessionsTree,
                                   agentsBySession: [SwarmSessionID: [SwarmAgent]] = [:]) -> [NoticeEvent] {
        var previousBySession: [SwarmSessionID: SwarmProjectSession] = [:]
        for row in previous.projects.flatMap(\.chats) {
            for session in row.session.sessions { previousBySession[session.id] = row.session }
        }
        var events: [NoticeEvent] = []
        var seen: Set<SwarmSessionID> = []
        for project in current.projects {
            for row in project.chats where seen.insert(row.id).inserted {
                let chat = row.session
                let before = chat.sessions.lazy.compactMap { previousBySession[$0.id] }.first
                // A failed status read is no transition. A first-seen idle chat is not a completion.
                if before != nil && before?.status == nil { continue }
                let kind: NoticeEvent.Kind
                switch chat.status {
                case .waiting where before?.status != .waiting: kind = .needsInput
                case .done where before != nil && before?.status != .done: kind = .done
                default: continue
                }
                let agents = agentsBySession[chat.id] ?? []
                let agent = kind == .needsInput
                    ? agents.first { $0.state == "waiting" }
                    : agents.first { SwarmPanePolicy.isChair($0, in: chat.session) }
                events.append(NoticeEvent(kind: kind, chat: chat, projectPath: project.path,
                                          agentID: agent?.id, stateKind: agent?.stateKind))
            }
        }
        return events
    }
}

public enum DockBadge {
    public static func count(tree: SessionsTree, prefs: NoticePrefs) -> Int {
        guard prefs.badge else { return 0 }
        return Set(tree.projects.flatMap(\.chats).filter { $0.session.status == .waiting }.map(\.id)).count
    }
}
