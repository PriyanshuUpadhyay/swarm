import Foundation

public struct NoticePrefs: Codable, Sendable, Hashable {
    public var post = true
    public var sound = true
    public var done = true
    public var badge = true
    public var mutedProjects: Set<String> = []

    public init() {}

    public init(from decoder: Decoder) throws {
        let values = try decoder.container(keyedBy: CodingKeys.self)
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

    public init(kind: Kind, chat: SwarmProjectSession, projectPath: String) {
        self.kind = kind
        self.chat = chat
        self.projectPath = projectPath
    }
}

public struct Notice: Sendable, Hashable {
    public let title: String
    public let body: String
    public let sessionID: SwarmSessionID
    public let sound: Bool
}

public enum NoticeRule {
    public static func shouldPost(event: NoticeEvent, prefs: NoticePrefs, project: ProjectNode, title: String) -> Notice? {
        guard prefs.post, !prefs.mutedProjects.contains(project.path),
              event.kind != .done || prefs.done else { return nil }
        let body = event.kind == .needsInput
            ? "\(project.name): waiting on a permission or a question"
            : "\(project.name): chat is done"
        return Notice(title: "Swarm — \(title)", body: body,
                      sessionID: event.chat.id, sound: prefs.sound)
    }

    public static func transitions(previous: SessionsTree, current: SessionsTree) -> [NoticeEvent] {
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
                events.append(NoticeEvent(kind: kind, chat: chat, projectPath: project.path))
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
