import Foundation

public enum SwarmAgentName {
    public static func isValid(_ name: String) -> Bool {
        name.range(
            of: "^[a-z0-9][a-z0-9-]{0,39}$",
            options: .regularExpression
        ) != nil
    }

    /// A role such as `code.complex` becomes `code-complex-1`, then the first unused number.
    public static func free(role: String, excluding taken: some Sequence<SwarmAgentID>) -> SwarmAgentID {
        let occupied = Set(taken.map(\.rawValue))
        var base = role.lowercased().map { character -> Character in
            character.isASCII && (character.isLetter || character.isNumber) ? character : "-"
        }
        while base.last == "-" { base.removeLast() }
        if base.isEmpty { base = Array("agent") }

        var number = 1
        while true {
            let suffix = "-\(number)"
            let stem = String(base.prefix(40 - suffix.count)).trimmingCharacters(in: CharacterSet(charactersIn: "-"))
            let candidate = stem + suffix
            if !occupied.contains(candidate) { return SwarmAgentID(candidate) }
            number += 1
        }
    }
}

public struct SwarmChatRow: Sendable, Hashable, Identifiable {
    public enum Author: Sendable, Hashable {
        case you
        case agent(SwarmAgentID)
    }

    public var id: Int { seq }
    public var seq: Int
    public var author: Author
    public var kindLabel: String?
    public var body: String?

    public init(seq: Int, author: Author, kindLabel: String?, body: String?) {
        self.seq = seq
        self.author = author
        self.kindLabel = kindLabel
        self.body = body
    }

    public static func rows(for agent: SwarmAgentID, in messages: some Sequence<SwarmMessage>) -> [Self] {
        let chair = SwarmAgentID("orchestrator")
        return messages.compactMap { message in
            let author: Author
            if message.sender == chair, message.recipient == agent {
                author = .you
            } else if message.sender == agent, message.recipient == chair {
                author = .agent(agent)
            } else {
                return nil
            }
            return Self(
                seq: message.seq,
                author: author,
                kindLabel: ["ask", "summary"].contains(message.kind) ? nil : message.kind,
                body: message.body
            )
        }
    }

    /// Only replies already visible in an agent tab are acknowledged.
    public static func acknowledgements(
        in messages: some Sequence<SwarmMessage>, shownAgents: Set<SwarmAgentID>
    ) -> [Int] {
        let chair = SwarmAgentID("orchestrator")
        return messages.compactMap { message in
            guard !message.read, message.recipient == chair,
                  shownAgents.contains(message.sender) else { return nil }
            return message.seq
        }
    }
}

public enum SwarmPollSchedule {
    public static let pollInterval: TimeInterval = 2
    public static let sweepInterval: TimeInterval = 30

    public static func delay(afterFailures failures: Int) -> TimeInterval {
        min(pollInterval * pow(2, Double(failures)), sweepInterval)
    }

    public static func shouldSweep(last: Date?, now: Date, hasPane: Bool) -> Bool {
        guard hasPane else { return false }
        guard let last else { return true }
        return now.timeIntervalSince(last) >= sweepInterval
    }
}

public enum SwarmWorkspaceSession {
    public static func settingKey(workspaceID: WorkspaceID) -> String {
        "workspace.\(workspaceID.rawValue).swarmSession"
    }

    public static func load(workspaceID: WorkspaceID, from store: Store) async -> SwarmSessionID? {
        guard let value = try? await store.setting(settingKey(workspaceID: workspaceID)),
              !value.isEmpty else { return nil }
        return SwarmSessionID(value)
    }

    public static func save(_ session: SwarmSessionID, workspaceID: WorkspaceID, in store: Store) async throws {
        try await store.setSetting(settingKey(workspaceID: workspaceID), session.rawValue)
    }
}
