import Foundation

// The agents Swarm starts through swarm, and the messages it exchanges with them.
//
// Swarm is the chair of every agent it starts (docs/decisions/0007 at the root of the swarm
// repository). The commands and JSON are fixed in `docs/bus-contract.md` beside it, and a change
// to a shape changes that file, swarm's output and these types together. The JSON keys are
// snake_case; decode with `.convertFromSnakeCase`.

/// One agent in a swarm session. `pane` is nil for an agent that was registered and
/// never spawned, and for one that was closed or reported dead; `alive` is nil whenever `pane` is,
/// or when swarm could not list panes. The chair has a pane: `agent add` records the pane its own
/// adapter reports and refuses an empty answer, which is what a send is delivered to.
public struct SwarmAgent: Sendable, Hashable, Codable, Identifiable {
    public var id: SwarmAgentID
    public var role: String
    public var provider: String?
    public var pane: String?
    public var alive: Bool?
    public var createdAt: Int?

    public init(
        id: SwarmAgentID, role: String, pane: String?, alive: Bool?,
        provider: String? = nil, createdAt: Int? = nil
    ) {
        self.id = id
        self.role = role
        self.provider = provider
        self.pane = pane
        self.alive = alive
        self.createdAt = createdAt
    }
}

public struct SwarmAgentList: Sendable, Hashable, Codable {
    public var agents: [SwarmAgent]
    public var attachable: Bool?

    public init(agents: [SwarmAgent], attachable: Bool? = nil) {
        self.agents = agents
        self.attachable = attachable
    }
}

/// One message on the bus. `body` is nil when swarm could not read the message's body file.
public struct SwarmMessage: Sendable, Hashable, Codable, Identifiable {
    public var id: Int { seq }

    public var seq: Int
    public var sender: SwarmAgentID
    public var recipient: SwarmAgentID
    public var kind: String
    public var body: String?
    public var createdAt: Int
    public var read: Bool

    public init(
        seq: Int, sender: SwarmAgentID, recipient: SwarmAgentID, kind: String,
        body: String?, createdAt: Int, read: Bool
    ) {
        self.seq = seq
        self.sender = sender
        self.recipient = recipient
        self.kind = kind
        self.body = body
        self.createdAt = createdAt
        self.read = read
    }
}

public struct SwarmMessageList: Sendable, Hashable, Codable {
    public var messages: [SwarmMessage]

    public init(messages: [SwarmMessage]) {
        self.messages = messages
    }
}

/// One swarm session discovered through the bus.
///
/// The bus writes its UUID v7 session id as JSON text.
public struct SwarmSession: Sendable, Hashable, Codable, Identifiable {
    public var id: SwarmSessionID
    public var talkMode: String
    public var adapter: String?
    public var cwd: String
    public var createdAt: Int
    public var chairProvider: String?
    public var chairID: SwarmChairID?
    public var chairLog: String?
    public var agents: Int
    public var messages: Int
    public var lastMessageAt: Int?
    public var archivedAt: Int?

    public init(
        id: SwarmSessionID, talkMode: String, adapter: String?, cwd: String, createdAt: Int,
        chairProvider: String? = nil, chairID: SwarmChairID? = nil,
        chairLog: String?, agents: Int, messages: Int, lastMessageAt: Int?,
        archivedAt: Int? = nil
    ) {
        self.id = id
        self.talkMode = talkMode
        self.adapter = adapter
        self.cwd = cwd
        self.createdAt = createdAt
        self.chairProvider = chairProvider
        self.chairID = chairID
        self.chairLog = chairLog
        self.agents = agents
        self.messages = messages
        self.lastMessageAt = lastMessageAt
        self.archivedAt = archivedAt
    }

    private enum CodingKeys: String, CodingKey {
        case id, talkMode, adapter, cwd, createdAt, chairProvider, chairID = "chairId", chairLog
        case agents, messages, lastMessageAt, archivedAt
    }

    public init(from decoder: any Decoder) throws {
        let values = try decoder.container(keyedBy: CodingKeys.self)
        id = SwarmSessionID(try values.decode(String.self, forKey: .id))
        talkMode = try values.decode(String.self, forKey: .talkMode)
        adapter = try values.decodeIfPresent(String.self, forKey: .adapter)
        cwd = try values.decode(String.self, forKey: .cwd)
        createdAt = try values.decode(Int.self, forKey: .createdAt)
        chairProvider = try values.decodeIfPresent(String.self, forKey: .chairProvider)
        chairID = try values.decodeIfPresent(SwarmChairID.self, forKey: .chairID)
        chairLog = try values.decodeIfPresent(String.self, forKey: .chairLog)
        agents = try values.decode(Int.self, forKey: .agents)
        messages = try values.decode(Int.self, forKey: .messages)
        lastMessageAt = try values.decodeIfPresent(Int.self, forKey: .lastMessageAt)
        archivedAt = try values.decodeIfPresent(Int.self, forKey: .archivedAt)
    }

    public func encode(to encoder: any Encoder) throws {
        var values = encoder.container(keyedBy: CodingKeys.self)
        try values.encode(id.rawValue, forKey: .id)
        try values.encode(talkMode, forKey: .talkMode)
        try values.encodeIfPresent(adapter, forKey: .adapter)
        try values.encode(cwd, forKey: .cwd)
        try values.encode(createdAt, forKey: .createdAt)
        try values.encodeIfPresent(chairProvider, forKey: .chairProvider)
        try values.encodeIfPresent(chairID, forKey: .chairID)
        try values.encodeIfPresent(chairLog, forKey: .chairLog)
        try values.encode(agents, forKey: .agents)
        try values.encode(messages, forKey: .messages)
        try values.encodeIfPresent(lastMessageAt, forKey: .lastMessageAt)
        try values.encodeIfPresent(archivedAt, forKey: .archivedAt)
    }
}

public struct SwarmChair: Sendable, Hashable {
    public var provider: String
    public var id: SwarmChairID

    public init?(agent: AgentKind, id: String) {
        let provider: String? = switch agent {
        case .claudeCode: "claude"
        case .codex: "codex"
        case .cursor, .openCode, .grok: nil
        }
        guard let provider, !id.isEmpty else { return nil }
        self.provider = provider
        self.id = SwarmChairID(id)
    }

    public var argument: String { provider + ":" + id.rawValue }
}

public struct SwarmSessionList: Sendable, Hashable, Codable {
    public var sessions: [SwarmSession]

    public init(sessions: [SwarmSession]) {
        self.sessions = sessions
    }
}

/// What `swarm launch` reported: the new pane, and the account it runs on when one was asked for.
public struct SwarmLaunch: Sendable, Hashable {
    public var pane: String
    public var account: String?

    public init(pane: String, account: String?) {
        self.pane = pane
        self.account = account
    }
}

/// The process that shows one agent's live pane: `swarm attach <agent>` with the session's
/// environment. A value, so the terminal view starts it and nothing here runs it.
public struct SwarmAttachCommand: Sendable, Hashable {
    public var executable: String
    public var arguments: [String]
    public var environment: [String: String]

    public init(executable: String, arguments: [String], environment: [String: String]) {
        self.executable = executable
        self.arguments = arguments
        self.environment = environment
    }
}

/// Where Swarm starts, reads and talks to swarm agents. The live bus runs the `swarm` CLI; tests
/// and previews hand in their own. Failures are `SwarmProfileError`, the same two kinds the
/// profile source throws.
public protocol SwarmBus: Sendable {
    /// `swarm init`, `swarm session new lane` and `swarm agent add orchestrator orchestrator`.
    func startSession() async throws -> SwarmSessionID
    /// Creates the session whose chair is the app's interactive CLI. The chair registers from its
    /// tmux pane immediately before that CLI starts.
    func startChairSession(
        chair: SwarmChair?, directory: String
    ) async throws -> SwarmSessionID
    func setChair(_ chair: SwarmChair, in session: SwarmSessionID) async throws
    /// `swarm launch`, run in `directory`. `account` is `"auto"`, an account name, or nil for the
    /// CLI's default home.
    func launch(
        _ agent: SwarmAgentID, role: String, account: String?,
        in session: SwarmSessionID, directory: String
    ) async throws -> SwarmLaunch
    func agents(in session: SwarmSessionID, adapter: String) async throws -> [SwarmAgent]
    func agentListing(
        in session: SwarmSessionID, adapter: String
    ) async throws -> SwarmAgentList
    func messages(
        in session: SwarmSessionID, after seq: Int, adapter: String
    ) async throws -> [SwarmMessage]
    /// `swarm sessions --json`, with no session selected in the environment.
    func sessions() async throws -> [SwarmSession]
    /// `swarm session archive <id>...`, with no session selected in the environment.
    func archive(_ sessions: [SwarmSessionID]) async throws
    /// `swarm send <agent> ask` with `body` on stdin. Returns the new message's seq.
    func send(_ body: String, to agent: SwarmAgentID, in session: SwarmSessionID) async throws -> Int
    /// `swarm type <agent>` with `text` on stdin.
    func type(
        _ text: String, to agent: SwarmAgentID, in session: SwarmSessionID, adapter: String
    ) async throws
    /// `swarm interrupt <agent>`.
    func interrupt(
        _ agent: SwarmAgentID, in session: SwarmSessionID, adapter: String
    ) async throws
    func ack(_ seq: Int, in session: SwarmSessionID) async throws
    func sweep(in session: SwarmSessionID) async throws
    func close(_ agent: SwarmAgentID, in session: SwarmSessionID) async throws
    func attachCommand(for agent: SwarmAgentID, in session: SwarmSessionID) -> SwarmAttachCommand
}

public extension SwarmBus {
    func agentListing(
        in session: SwarmSessionID, adapter: String
    ) async throws -> SwarmAgentList {
        SwarmAgentList(agents: try await agents(in: session, adapter: adapter))
    }

    func startChairSession(
        chair: SwarmChair?, directory: String
    ) async throws -> SwarmSessionID {
        throw SwarmProfileError.unavailable("swarm chair sessions are not available")
    }

    func setChair(_ chair: SwarmChair, in session: SwarmSessionID) async throws {
        throw SwarmProfileError.unavailable("swarm chair sessions are not available")
    }

    func archive(_ sessions: [SwarmSessionID]) async throws {
        throw SwarmProfileError.unavailable("swarm session archives are not available")
    }

    func agents(in session: SwarmSessionID) async throws -> [SwarmAgent] {
        try await agents(in: session, adapter: SwarmSessionInteraction.defaultAdapter)
    }

    func agentListing(in session: SwarmSessionID) async throws -> SwarmAgentList {
        try await agentListing(in: session, adapter: SwarmSessionInteraction.defaultAdapter)
    }

    func messages(in session: SwarmSessionID, after seq: Int) async throws -> [SwarmMessage] {
        try await messages(
            in: session, after: seq, adapter: SwarmSessionInteraction.defaultAdapter
        )
    }

    func agents(in session: SwarmSession) async throws -> [SwarmAgent] {
        try await agents(in: session.id, adapter: try SwarmSessionInteraction.adapter(for: session))
    }

    func messages(in session: SwarmSession, after seq: Int) async throws -> [SwarmMessage] {
        try await messages(
            in: session.id, after: seq, adapter: try SwarmSessionInteraction.adapter(for: session)
        )
    }

    func type(_ text: String, to agent: SwarmAgentID, in session: SwarmSession) async throws {
        try await type(
            text, to: agent, in: session.id,
            adapter: try SwarmSessionInteraction.adapter(for: session)
        )
    }

    func interrupt(_ agent: SwarmAgentID, in session: SwarmSession) async throws {
        try await interrupt(
            agent, in: session.id, adapter: try SwarmSessionInteraction.adapter(for: session)
        )
    }
}

/// The bus before swarm is connected. Every call fails as unavailable, so a view shows its empty
/// state rather than a wrong list.
public struct UnavailableSwarmBus: SwarmBus {
    public init() {}

    private var notConnected: SwarmProfileError { .unavailable("swarm is not connected") }

    public func startSession() async throws -> SwarmSessionID { throw notConnected }

    public func launch(
        _ agent: SwarmAgentID, role: String, account: String?,
        in session: SwarmSessionID, directory: String
    ) async throws -> SwarmLaunch {
        throw notConnected
    }

    public func agents(
        in session: SwarmSessionID, adapter: String
    ) async throws -> [SwarmAgent] { throw notConnected }

    public func messages(
        in session: SwarmSessionID, after seq: Int, adapter: String
    ) async throws -> [SwarmMessage] {
        throw notConnected
    }

    public func sessions() async throws -> [SwarmSession] { throw notConnected }

    public func send(_ body: String, to agent: SwarmAgentID, in session: SwarmSessionID) async throws -> Int {
        throw notConnected
    }

    public func type(
        _ text: String, to agent: SwarmAgentID, in session: SwarmSessionID, adapter: String
    ) async throws {
        throw notConnected
    }

    public func interrupt(
        _ agent: SwarmAgentID, in session: SwarmSessionID, adapter: String
    ) async throws {
        throw notConnected
    }

    public func ack(_ seq: Int, in session: SwarmSessionID) async throws { throw notConnected }

    public func sweep(in session: SwarmSessionID) async throws { throw notConnected }

    public func close(_ agent: SwarmAgentID, in session: SwarmSessionID) async throws { throw notConnected }

    public func attachCommand(for agent: SwarmAgentID, in session: SwarmSessionID) -> SwarmAttachCommand {
        SwarmAttachCommand(executable: "swarm", arguments: ["attach", agent.rawValue], environment: [:])
    }
}
