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

/// Unknown wire states preserve uncertainty instead of asserting a sign-in or quota result.
public protocol SwarmWireState: RawRepresentable, Codable where RawValue == String {
    static var unavailable: Self { get }
}

public extension SwarmWireState {
    init(from decoder: Decoder) throws {
        self = Self(rawValue: try decoder.singleValueContainer().decode(String.self)) ?? .unavailable
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.singleValueContainer()
        try container.encode(rawValue)
    }
}

public enum SwarmAccountSourceState: String, SwarmWireState, Sendable {
    case ready, unavailable
    case noSource = "no_source"
}

public enum SwarmAccountAuthState: String, SwarmWireState, Sendable {
    case signedIn = "signed_in"
    case signedOut = "signed_out"
    case unavailable
}

public enum SwarmUsageState: String, SwarmWireState, Sendable {
    case fresh, stale, missing, failed, unavailable
    case noSource = "no_source"
}

public struct SwarmAccount: Sendable, Hashable, Codable, Identifiable {
    public var id: String { name }
    public var name: String
    public var email: String?
    public var home: String
    public var env: [String: String]
    public var authState: SwarmAccountAuthState
    public var remainingPct: Double?
    public var usageState: SwarmUsageState
    public var usageSource: String?
    public var summary: String?

    public init(
        name: String, email: String?, home: String, env: [String: String],
        authState: SwarmAccountAuthState, remainingPct: Double?, summary: String?,
        usageState: SwarmUsageState = .missing, usageSource: String? = nil
    ) {
        self.name = name
        self.email = email
        self.home = home
        self.env = env
        self.authState = authState
        self.remainingPct = validPercentage(remainingPct)
        self.summary = summary
        self.usageState = usageState
        self.usageSource = usageSource
    }

    enum CodingKeys: String, CodingKey {
        case name, email, home, env, authState, remainingPct, usageState, usageSource, summary
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        name = try container.decode(String.self, forKey: .name)
        email = try container.decodeIfPresent(String.self, forKey: .email)
        home = try container.decode(String.self, forKey: .home)
        env = try container.decode([String: String].self, forKey: .env)
        authState = try container.decodeIfPresent(SwarmAccountAuthState.self, forKey: .authState) ?? .unavailable
        remainingPct = try container.percentage(forKey: .remainingPct)
        usageState = try container.decodeIfPresent(SwarmUsageState.self, forKey: .usageState) ?? .unavailable
        usageSource = try container.decodeIfPresent(String.self, forKey: .usageSource)
        summary = try container.decodeIfPresent(String.self, forKey: .summary)
    }
}

public struct SwarmAccountList: Sendable, Hashable, Codable {
    public var provider: String
    public var source: String?
    public var state: SwarmAccountSourceState
    public var revision: String
    public var modified: Bool
    public var accounts: [SwarmAccount]
    public var auto: String?

    public init(
        provider: String, source: String?, accounts: [SwarmAccount], auto: String?,
        state: SwarmAccountSourceState = .ready, revision: String = "", modified: Bool = false
    ) {
        self.provider = provider
        self.source = source
        self.accounts = accounts
        self.auto = auto
        self.state = state
        self.revision = revision
        self.modified = modified
    }

    public var autoEnvironment: [String: String] {
        SwarmLaunchAccount.resolve(.auto, from: self)?.environment ?? [:]
    }
}

public struct SwarmUsageMeter: Sendable, Hashable, Codable, Identifiable {
    public var id: String { [provider, account ?? "", window ?? ""].joined(separator: ":") }
    public var provider: String
    public var account: String?
    public var label: String
    public var window: String?
    public var windowMinutes: Int?
    public var usedPct: Double?
    public var resetTimeSeconds: Int?
    public var state: SwarmUsageState
    public var source: String?
    public var reason: String?
    public var asOfSeconds: Int?

    public init(
        provider: String, account: String?, label: String, window: String?,
        windowMinutes: Int? = nil, usedPct: Double?, resetTimeSeconds: Int? = nil,
        state: SwarmUsageState, source: String? = nil, reason: String? = nil, asOfSeconds: Int? = nil
    ) {
        self.provider = provider
        self.account = account
        self.label = label
        self.window = window
        self.windowMinutes = windowMinutes
        self.usedPct = validPercentage(usedPct)
        self.resetTimeSeconds = resetTimeSeconds
        self.state = state
        self.source = source
        self.reason = reason
        self.asOfSeconds = asOfSeconds
    }

    enum CodingKeys: String, CodingKey {
        case provider, account, label, window, windowMinutes, usedPct, resetTimeSeconds
        case state, source, reason, asOfSeconds
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        provider = try container.decode(String.self, forKey: .provider)
        account = try container.decodeIfPresent(String.self, forKey: .account)
        label = try container.decode(String.self, forKey: .label)
        window = try container.decodeIfPresent(String.self, forKey: .window)
        windowMinutes = try container.decodeIfPresent(Int.self, forKey: .windowMinutes)
        usedPct = try container.percentage(forKey: .usedPct)
        resetTimeSeconds = try container.decodeIfPresent(Int.self, forKey: .resetTimeSeconds)
        state = try container.decodeIfPresent(SwarmUsageState.self, forKey: .state) ?? .unavailable
        source = try container.decodeIfPresent(String.self, forKey: .source)
        reason = try container.decodeIfPresent(String.self, forKey: .reason)
        asOfSeconds = try container.decodeIfPresent(Int.self, forKey: .asOfSeconds)
    }
}

private func validPercentage(_ value: Double?) -> Double? {
    value.flatMap { $0.isFinite && (0...100).contains($0) ? $0 : nil }
}

private extension KeyedDecodingContainer {
    func percentage(forKey key: Key) throws -> Double? {
        guard let value = try decodeIfPresent(Double.self, forKey: key) else { return nil }
        guard validPercentage(value) != nil else {
            throw DecodingError.dataCorruptedError(forKey: key, in: self, debugDescription: "Percentage must be finite and from 0 to 100")
        }
        return value
    }
}

public struct SwarmUsage: Sendable, Hashable, Codable {
    public var meters: [SwarmUsageMeter]
    public init(meters: [SwarmUsageMeter]) { self.meters = meters }
}

public struct SwarmAccountLoginRequest: Sendable, Hashable {
    public var provider: String
    public var name: String
    public var revision: String

    public init(provider: String, name: String, revision: String) {
        self.provider = provider
        self.name = name
        self.revision = revision
    }

    public static func validName(_ name: String) -> Bool {
        !name.isEmpty && !["auto", "default", ".", ".."].contains(name)
            && !name.contains("..") && name.utf8.allSatisfy {
                (97...122).contains($0) || (48...57).contains($0) || [46, 95, 45].contains($0)
            }
    }
}

public enum SwarmAccountLoginState: String, SwarmWireState, Sendable {
    case opened, unavailable
}

public struct SwarmAccountLoginResult: Sendable, Hashable, Codable {
    public var provider: String
    public var account: String
    public var pane: String
    public var state: SwarmAccountLoginState
    public var revision: String
}

public struct SwarmAccountMetadataAction: Sendable, Hashable, Codable {
    public var revision: String
}

public enum SwarmProfileError: LocalizedError, Sendable, Equatable {
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
    func refreshUsage(provider: String) async throws -> SwarmUsage
    func openLogin(_ request: SwarmAccountLoginRequest) async throws -> SwarmAccountLoginResult
    func resetAccounts(revision: String) async throws -> SwarmAccountMetadataAction
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

public extension SwarmProfileSource {
    func refreshUsage(provider: String) async throws -> SwarmUsage {
        throw SwarmProfileError.unavailable("Usage refresh is unavailable")
    }

    func openLogin(_ request: SwarmAccountLoginRequest) async throws -> SwarmAccountLoginResult {
        throw SwarmProfileError.unavailable("Account login is unavailable")
    }

    func resetAccounts(revision: String) async throws -> SwarmAccountMetadataAction {
        throw SwarmProfileError.unavailable("Account settings are unavailable")
    }
}
