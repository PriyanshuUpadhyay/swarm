import Foundation

// The profiles, providers, accounts and usage that swarm reports, as Swarm reads them.
//
// swarm owns these decisions (docs/decisions/0005 at the root of the swarm repository) and prints
// them as JSON. `src/config.rs` and `src/profiles.rs` own the shapes, so a change must update swarm's output and these
// types together. The JSON keys are snake_case; decode with `.convertFromSnakeCase`.

/// One runner of a profile: a provider, model and effort, plus the flags that provider takes.
public struct SwarmRunner: Sendable, Hashable, Codable, Identifiable {
    /// Made at decode and never saved, so a list row keeps its identity through a reorder or an
    /// edit. Equality and hashing leave it out: two runners with the same settings are the same.
    public var id = UUID()
    public var provider: String
    public var model: String
    public var effort: String
    public var sandbox: String?
    public var approval: String?
    public var permission: String?

    enum CodingKeys: String, CodingKey { case provider, model, effort, sandbox, approval, permission }

    public init(
        provider: String, model: String, effort: String,
        sandbox: String? = nil, approval: String? = nil, permission: String? = nil
    ) {
        self.provider = provider
        self.model = model
        self.effort = effort
        self.sandbox = sandbox
        self.approval = approval
        self.permission = permission
    }

    public static func == (lhs: SwarmRunner, rhs: SwarmRunner) -> Bool {
        (lhs.provider, lhs.model, lhs.effort, lhs.sandbox, lhs.approval, lhs.permission)
            == (rhs.provider, rhs.model, rhs.effort, rhs.sandbox, rhs.approval, rhs.permission)
    }

    public func hash(into hasher: inout Hasher) {
        hasher.combine(provider)
        hasher.combine(model)
        hasher.combine(effort)
        hasher.combine(sandbox)
        hasher.combine(approval)
        hasher.combine(permission)
    }

    /// The value of a provider field (`sandbox`, `approval`, or `permission`).
    public subscript(field name: String) -> String? {
        get {
            switch name {
            case "sandbox": sandbox
            case "approval": approval
            case "permission": permission
            default: nil
            }
        }
        set {
            switch name {
            case "sandbox": sandbox = newValue
            case "approval": approval = newValue
            case "permission": permission = newValue
            default: break
            }
        }
    }
}

/// One role and the runners that start it, first one that can run.
public struct SwarmProfile: Sendable, Hashable, Codable, Identifiable {
    public var id: String { name }
    public var name: String
    public var runners: [SwarmRunner]

    public init(name: String, runners: [SwarmRunner]) {
        self.name = name
        self.runners = runners
    }
}

/// Where the profiles came from when swarm imported the old routing file, and the routes it
/// could not convert.
public struct SwarmProfileImport: Sendable, Hashable, Codable {
    public var from: String
    public var unmapped: [String]
}

/// `swarm roles --json`. `revision` is what a save names, so a save never overwrites a newer file.
public struct SwarmProfileList: Sendable, Hashable, Codable {
    public var revision: String
    public var minUsageLeftPct: Int
    public var imported: SwarmProfileImport?
    public var profiles: [SwarmProfile]
}

/// Why a runner would not start. `code` may take new values; show `text`.
public struct SwarmSkip: Sendable, Hashable, Codable {
    public var index: Int
    public var code: String
    public var text: String
}

/// `swarm roles check --json`: which runner each profile would start now.
public struct SwarmProfileCheck: Sendable, Hashable, Codable {
    public var name: String
    public var pick: Int?
    public var skipped: [SwarmSkip]
}

struct SwarmProfileCheckList: Codable {
    var profiles: [SwarmProfileCheck]
}

/// One flag a provider takes beside model and effort. `values` is a hint from the CLI's help,
/// not a gate: a value outside it is kept.
public struct SwarmProviderField: Sendable, Hashable, Codable {
    public var name: String
    public var label: String
    public var values: [String]
    public var `default`: String
}

/// What a provider accepts, from `swarm providers --json`.
public struct SwarmProvider: Sendable, Hashable, Codable, Identifiable {
    public var id: String
    public var label: String
    public var efforts: [String]
    public var defaultEffort: String
    public var accounts: Bool
    public var fields: [SwarmProviderField]

    /// A new runner of this provider with `model`, its default effort, and each field's default.
    public func runner(model: String) -> SwarmRunner {
        var runner = SwarmRunner(provider: id, model: model, effort: defaultEffort)
        for field in fields { runner[field: field.name] = field.default }
        return runner
    }
}

struct SwarmProviderList: Codable {
    var providers: [SwarmProvider]
}

public struct SwarmModel: Sendable, Hashable, Codable, Identifiable {
    public var id: String
    public var label: String
    /// The efforts this model takes, when its CLI says; else the provider's list applies.
    public var efforts: [String]?

    public init(id: String, label: String, efforts: [String]? = nil) {
        self.id = id
        self.label = label
        self.efforts = efforts
    }
}

public struct SwarmModelList: Sendable, Hashable, Codable {
    public var provider: String
    public var models: [SwarmModel]
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

    /// The environment of the account this provider would pick by itself, or nothing.
    ///
    /// **A seat inherits the app's environment, and the app has none.** Swarm is started with a
    /// clean environment on purpose, so it carries no `CLAUDE_CONFIG_DIR`, and a CLI spawned by a
    /// chair opened the provider's default home rather than the signed-in profile. Both seats of
    /// one council run stopped at "Not logged in · Please run /login" for that reason alone.
    public var autoEnvironment: [String: String] {
        let signedIn = accounts.filter(\.signedIn)
        let chosen = signedIn.first { $0.name == auto } ?? signedIn.first
        return chosen?.env ?? [:]
    }
}

/// One usage window for one account, such as Claude's seven day window. A row with a nil `window`
/// describes the whole account instead, such as one that is logged out, and `reason` says why.
public struct SwarmUsageMeter: Sendable, Hashable, Codable {
    public var provider: String
    public var account: String?
    public var label: String
    public var window: String?
    public var usedPct: Int?
    public var resetsIn: String?
    public var state: String
    public var reason: String?
    public var asOf: Int?

    public init(
        provider: String, account: String?, label: String, window: String?,
        usedPct: Int?, resetsIn: String?, state: String, reason: String?, asOf: Int?
    ) {
        self.provider = provider
        self.account = account
        self.label = label
        self.window = window
        self.usedPct = usedPct
        self.resetsIn = resetsIn
        self.state = state
        self.reason = reason
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

/// Where Swarm reads accounts and usage. The live source runs the `swarm` CLI; tests and
/// previews hand in their own.
public protocol SwarmProfileSource: Sendable {
    func accounts(provider: String) async throws -> SwarmAccountList
    func usage() async throws -> [SwarmUsageMeter]
}

/// The source before swarm is connected. Every call fails as unavailable, so a view shows its
/// empty state rather than a wrong list.
public struct UnavailableSwarmProfileSource: SwarmProfileSource {
    public init() {}

    public func accounts(provider: String) async throws -> SwarmAccountList {
        throw SwarmProfileError.unavailable("swarm is not connected")
    }

    public func usage() async throws -> [SwarmUsageMeter] {
        throw SwarmProfileError.unavailable("swarm is not connected")
    }
}
