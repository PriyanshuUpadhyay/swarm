import Foundation

// The agents Swarm starts through swarm, and the messages it exchanges with them.
//
// Swarm is the chair of every agent it starts (docs/decisions/0007 at the root of the swarm
// repository). The commands and JSON are fixed in `docs/bus-contract.md` beside it, and a change
// to a shape changes that file, swarm's output and these types together. The JSON keys are
// snake_case; decode with `.convertFromSnakeCase`.

/// One agent in a workspace's swarm session. `pane` is nil for the chair and for an agent that was
/// closed or reported dead; `alive` is nil whenever `pane` is, or when swarm could not list panes.
public struct SwarmAgent: Sendable, Hashable, Codable, Identifiable {
    public var id: SwarmAgentID
    public var role: String
    public var pane: String?
    public var alive: Bool?

    public init(id: SwarmAgentID, role: String, pane: String?, alive: Bool?) {
        self.id = id
        self.role = role
        self.pane = pane
        self.alive = alive
    }
}

public struct SwarmAgentList: Sendable, Hashable, Codable {
    public var agents: [SwarmAgent]

    public init(agents: [SwarmAgent]) {
        self.agents = agents
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
    /// `swarm launch`, run in `directory`. `account` is `"auto"`, an account name, or nil for the
    /// CLI's default home.
    func launch(
        _ agent: SwarmAgentID, role: String, account: String?,
        in session: SwarmSessionID, directory: String
    ) async throws -> SwarmLaunch
    func agents(in session: SwarmSessionID) async throws -> [SwarmAgent]
    func messages(in session: SwarmSessionID, after seq: Int) async throws -> [SwarmMessage]
    /// `swarm send <agent> ask` with `body` on stdin. Returns the new message's seq.
    func send(_ body: String, to agent: SwarmAgentID, in session: SwarmSessionID) async throws -> Int
    func ack(_ seq: Int, in session: SwarmSessionID) async throws
    func sweep(in session: SwarmSessionID) async throws
    func close(_ agent: SwarmAgentID, in session: SwarmSessionID) async throws
    func attachCommand(for agent: SwarmAgentID, in session: SwarmSessionID) -> SwarmAttachCommand
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

    public func agents(in session: SwarmSessionID) async throws -> [SwarmAgent] { throw notConnected }

    public func messages(in session: SwarmSessionID, after seq: Int) async throws -> [SwarmMessage] {
        throw notConnected
    }

    public func send(_ body: String, to agent: SwarmAgentID, in session: SwarmSessionID) async throws -> Int {
        throw notConnected
    }

    public func ack(_ seq: Int, in session: SwarmSessionID) async throws { throw notConnected }

    public func sweep(in session: SwarmSessionID) async throws { throw notConnected }

    public func close(_ agent: SwarmAgentID, in session: SwarmSessionID) async throws { throw notConnected }

    public func attachCommand(for agent: SwarmAgentID, in session: SwarmSessionID) -> SwarmAttachCommand {
        SwarmAttachCommand(executable: "swarm", arguments: ["attach", agent.rawValue], environment: [:])
    }
}
