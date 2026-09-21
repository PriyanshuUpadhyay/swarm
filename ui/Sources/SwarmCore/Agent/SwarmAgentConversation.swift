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

public enum SwarmAckReservation {
    public static func reserve(
        _ candidates: [Int], inFlight: Set<Int>
    ) -> (sequences: [Int], inFlight: Set<Int>) {
        let sequences = candidates.filter { !inFlight.contains($0) }
        return (sequences, inFlight.union(sequences))
    }

    public static func release(_ sequences: [Int], inFlight: Set<Int>) -> Set<Int> {
        inFlight.subtracting(sequences)
    }
}

public struct SwarmErrorDisplay: Sendable, Equatable {
    public private(set) var visible: String?
    private var dismissed: String?

    public init() {}

    public mutating func record(_ message: String) {
        guard dismissed != message else { return }
        visible = message
    }

    public mutating func dismiss() {
        dismissed = visible
        visible = nil
    }

    public mutating func succeed() {
        dismissed = nil
    }
}

public struct SwarmArchiveCloseFailure: Sendable, Equatable {
    public var session: SwarmSessionID
    public var agents: [SwarmAgentID]
    public var listingFailed: Bool

    public init(
        session: SwarmSessionID, agents: some Sequence<SwarmAgentID>, listingFailed: Bool
    ) {
        self.session = session
        self.agents = Array(Set(agents)).sorted { $0.rawValue < $1.rawValue }
        self.listingFailed = listingFailed
    }

    public var logMessage: String {
        let names = agents.map(\.rawValue).joined(separator: ", ")
        let failure = listingFailed ? "could not list or close" : "could not close"
        let namesSuffix = names.isEmpty ? "" : ": \(names)"
        let noun = agents.count == 1 ? "swarm agent" : "swarm agents"
        return "\(failure) \(noun) in session \(session.rawValue)\(namesSuffix)"
    }

    public func notice(after archiveMessage: String) -> SwarmNotice {
        let action: String
        if agents.isEmpty {
            action = "Swarm could not list or close the agents in swarm session `\(session.rawValue)`. "
                + "Run `swarm agents`, then `swarm close <name>` by hand."
        } else {
            let names = agents.map { "`\($0.rawValue)`" }.joined(separator: ", ")
            let failure = listingFailed ? "list or close" : "close"
            let noun = agents.count == 1 ? "swarm agent" : "swarm agents"
            action = "Swarm could not \(failure) \(noun) \(names) in session `\(session.rawValue)`. "
                + "Run `swarm close <name>` for each agent by hand."
        }
        return SwarmNotice(
            message: archiveMessage + " " + action,
            dismissal: .untilDismissed
        )
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

    public static func clear(workspaceID: WorkspaceID, in store: Store) async throws {
        try await store.setSetting(settingKey(workspaceID: workspaceID), nil)
    }
}

public enum SwarmChatSession {
    private static func settingKey(sessionID: SessionID) -> String {
        "session.\(sessionID.rawValue).swarmSession"
    }

    public static func load(sessionID: SessionID, from store: Store) async -> SwarmSessionID? {
        guard let value = try? await store.setting(settingKey(sessionID: sessionID)),
              !value.isEmpty else { return nil }
        return SwarmSessionID(value)
    }

    /// Every chat that belongs to a swarm session, open or closed.
    ///
    /// **Closed chats have to be in here.** The links are read to decide which chat a swarm
    /// session types into, and `Store.sessions(workspaceID:)` lists open chats only. A chat closed
    /// while its swarm session stayed open therefore vanished from the map, the session fell back
    /// to the bus, and the owner saw "ring failed: can't find pane: %0".
    public static func loadAll(from store: Store) async -> [SwarmSessionID: SessionID] {
        let prefix = "session."
        let suffix = ".swarmSession"
        guard let rows = try? await store.settings(withPrefix: prefix) else { return [:] }
        var links: [SwarmSessionID: SessionID] = [:]
        for (key, value) in rows where key.hasSuffix(suffix) && !value.isEmpty {
            let id = String(key.dropFirst(prefix.count).dropLast(suffix.count))
            guard !id.isEmpty else { continue }
            links[SwarmSessionID(value)] = SessionID(id)
        }
        return links
    }

    public static func save(_ swarm: SwarmSessionID, sessionID: SessionID, in store: Store) async throws {
        try await store.setSetting(settingKey(sessionID: sessionID), swarm.rawValue)
    }
}
