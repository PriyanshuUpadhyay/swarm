import Foundation

/// A role from swarm as a choice Bloom can launch.
public struct SwarmLaunchRole: Identifiable, Sendable, Hashable {
    public var id: String { role.id }
    public var role: SwarmRole

    public init(_ role: SwarmRole) {
        self.role = role
    }

    public var agentKind: AgentKind? {
        switch role.provider {
        case "claude": .claudeCode
        case "codex": .codex
        default: nil
        }
    }

    public var disabledReason: String? {
        agentKind == nil ? "Bloom cannot run \(role.provider) roles" : nil
    }

    public func applying(to controls: ComposerControls) -> ComposerControls? {
        guard let agentKind else { return nil }
        var chosen = controls
        chosen.agentKind = agentKind
        chosen.model = role.model
        chosen.effort = role.effort ?? ""
        return chosen
    }

    public static func initial(in roles: [SwarmRole], controls: ComposerControls) -> SwarmLaunchRole? {
        let choices = roles.map(Self.init)
        return choices.first {
            $0.agentKind == controls.agentKind
                && $0.role.model == controls.model
                && ($0.role.effort ?? "") == controls.effort
        } ?? choices.first { $0.agentKind != nil }
    }
}

public enum SwarmAccountSelection: Sendable, Hashable, Identifiable {
    case auto
    case named(String)

    public var id: String {
        switch self {
        case .auto: "auto"
        case .named(let name): "account:\(name)"
        }
    }
}

public struct SwarmLaunchAccount: Sendable, Hashable, Codable {
    public var name: String
    public var environment: [String: String]

    public init(name: String, environment: [String: String]) {
        self.name = name
        self.environment = environment
    }

    public static func resolve(
        _ selection: SwarmAccountSelection,
        from list: SwarmAccountList
    ) -> SwarmLaunchAccount? {
        let name: String
        switch selection {
        case .auto:
            guard let automatic = list.auto else { return nil }
            name = automatic
        case .named(let chosen):
            name = chosen
        }
        guard let account = list.accounts.first(where: { $0.name == name }) else { return nil }
        return Self(name: account.name, environment: account.env)
    }

    public static func settingKey(sessionID: SessionID) -> String {
        "session.\(sessionID).launchAccount"
    }

    public func store(sessionID: SessionID, in store: Store) async {
        guard let data = try? JSONEncoder().encode(self) else { return }
        try? await store.setSetting(Self.settingKey(sessionID: sessionID), String(decoding: data, as: UTF8.self))
    }

    public static func load(sessionID: SessionID, from store: Store) async -> Self? {
        guard let stored = try? await store.setting(settingKey(sessionID: sessionID)) else { return nil }
        return try? JSONDecoder().decode(Self.self, from: Data(stored.utf8))
    }

    public func merging(into base: [String: String]) -> [String: String] {
        base.merging(environment) { _, account in account }
    }
}

public struct SwarmAccountOption: Identifiable, Sendable, Hashable {
    public var id: String { selection.id }
    public var selection: SwarmAccountSelection
    public var label: String
    public var account: SwarmLaunchAccount?

    public static func choices(from list: SwarmAccountList) -> [Self] {
        guard !list.accounts.isEmpty else { return [] }
        let automatic = SwarmLaunchAccount.resolve(.auto, from: list)
        let automaticSource = automatic.flatMap { chosen in
            list.accounts.first { $0.name == chosen.name }
        }
        var result = [Self(
            selection: .auto,
            label: automatic.map {
                let left = automaticSource?.remainingPct.map { ", \($0)% left" } ?? ""
                return "Auto (\($0.name))\(left)"
            } ?? "Auto",
            account: automatic
        )]
        result += list.accounts.map { account in
            Self(
                selection: .named(account.name),
                label: account.remainingPct.map { "\(account.name), \($0)% left" } ?? account.name,
                account: SwarmLaunchAccount(name: account.name, environment: account.env)
            )
        }
        return result
    }
}
