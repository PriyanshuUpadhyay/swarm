import Foundation

// The roles, accounts and usage that swarm reports, as Bloom reads them.
//
// swarm owns these decisions (docs/decisions/0005 at the root of the swarm repository) and prints
// them as JSON. The shapes are fixed in `docs/profiles-contract.md` beside it, and a change to a
// shape changes that file, swarm's output and these types together. The JSON keys are snake_case;
// decode with `.convertFromSnakeCase`.

/// One route from swarm's routing config, resolved to the runner it starts with.
public struct SwarmRole: Sendable, Hashable, Codable, Identifiable {
    public var id: String { role }

    public var role: String
    public var runner: String
    public var provider: String
    public var model: String
    public var effort: String?
    public var sandbox: String?
    public var fallbacks: [String]

    public init(
        role: String, runner: String, provider: String, model: String,
        effort: String?, sandbox: String?, fallbacks: [String]
    ) {
        self.role = role
        self.runner = runner
        self.provider = provider
        self.model = model
        self.effort = effort
        self.sandbox = sandbox
        self.fallbacks = fallbacks
    }
}

public struct SwarmRoleList: Sendable, Hashable, Codable {
    public var roles: [SwarmRole]

    public init(roles: [SwarmRole]) {
        self.roles = roles
    }
}

/// One signed-in account a provider's CLI can run on, and the environment that selects it.
public struct SwarmAccount: Sendable, Hashable, Codable, Identifiable {
    public var id: String { name }

    public var name: String
    public var email: String?
    public var home: String
    public var env: [String: String]
    public var signedIn: Bool
    public var remainingPct: Int?
    public var summary: String?

    public init(
        name: String, email: String?, home: String, env: [String: String],
        signedIn: Bool, remainingPct: Int?, summary: String?
    ) {
        self.name = name
        self.email = email
        self.home = home
        self.env = env
        self.signedIn = signedIn
        self.remainingPct = remainingPct
        self.summary = summary
    }
}

/// Every account for one provider, and the one "Auto" picks. `source` is nil, `accounts` empty and
/// `auto` nil for a provider with no account source yet, which launches on the CLI's default home.
public struct SwarmAccountList: Sendable, Hashable, Codable {
    public var provider: String
    public var source: String?
    public var accounts: [SwarmAccount]
    public var auto: String?

    public init(provider: String, source: String?, accounts: [SwarmAccount], auto: String?) {
        self.provider = provider
        self.source = source
        self.accounts = accounts
        self.auto = auto
    }
}

/// One usage window for one account, such as Claude's seven day window.
public struct SwarmUsageMeter: Sendable, Hashable, Codable {
    public var provider: String
    public var account: String?
    public var label: String
    public var window: String
    public var usedPct: Int
    public var resetsIn: String?
    public var state: String
    public var asOf: Int?

    public init(
        provider: String, account: String?, label: String, window: String,
        usedPct: Int, resetsIn: String?, state: String, asOf: Int?
    ) {
        self.provider = provider
        self.account = account
        self.label = label
        self.window = window
        self.usedPct = usedPct
        self.resetsIn = resetsIn
        self.state = state
        self.asOf = asOf
    }
}

public struct SwarmUsage: Sendable, Hashable, Codable {
    public var meters: [SwarmUsageMeter]

    public init(meters: [SwarmUsageMeter]) {
        self.meters = meters
    }
}

public enum SwarmProfileError: Error, Sendable, Equatable {
    /// swarm, or a tool swarm wraps, cannot be reached. The message is what to tell the reader.
    case unavailable(String)
    /// swarm ran and failed, or printed something that is not the contract's JSON.
    case failed(String)
}

/// Where Bloom reads roles, accounts and usage. The live source runs the `swarm` CLI; tests and
/// previews hand in their own.
public protocol SwarmProfileSource: Sendable {
    func roles() async throws -> [SwarmRole]
    func accounts(provider: String) async throws -> SwarmAccountList
    func usage() async throws -> [SwarmUsageMeter]
}

/// The source before swarm is connected. Every call fails as unavailable, so a view shows its
/// empty state rather than a wrong list.
public struct UnavailableSwarmProfileSource: SwarmProfileSource {
    public init() {}

    public func roles() async throws -> [SwarmRole] {
        throw SwarmProfileError.unavailable("swarm is not connected")
    }

    public func accounts(provider: String) async throws -> SwarmAccountList {
        throw SwarmProfileError.unavailable("swarm is not connected")
    }

    public func usage() async throws -> [SwarmUsageMeter] {
        throw SwarmProfileError.unavailable("swarm is not connected")
    }
}
