import Foundation

public extension SwarmRole {
    var launchAgentKind: AgentKind? {
        switch provider {
        case "claude": .claudeCode
        case "codex": .codex
        default: nil
        }
    }

    var launchDisabledReason: String? {
        launchAgentKind == nil ? "Bloom cannot run \(provider) roles" : nil
    }

    func applyingToLaunchControls(_ controls: ComposerControls) -> ComposerControls? {
        guard let launchAgentKind else { return nil }
        var chosen = controls
        chosen.agentKind = launchAgentKind
        chosen.model = model
        chosen.effort = effort ?? ""
        return chosen
    }

    static func initialLaunchRole(
        in roles: [SwarmRole], controls: ComposerControls
    ) -> SwarmRole? {
        roles.first {
            $0.launchAgentKind == controls.agentKind
                && $0.model == controls.model
                && ($0.effort ?? "") == controls.effort
        }
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
    public var provider: String
    public var environment: [String: String]

    public init?(name: String, provider: String, environment: [String: String]) {
        guard let key = Self.environmentKey(for: provider),
              let value = environment[key], !value.isEmpty else { return nil }
        self.name = name
        self.provider = provider
        self.environment = [key: value]
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
        guard let account = list.accounts.first(where: { $0.name == name }),
              account.signedIn else { return nil }
        return Self(name: account.name, provider: list.provider, environment: account.env)
    }

    public static func settingKey(sessionID: SessionID) -> String {
        "session.\(sessionID).launchAccount"
    }

    public func store(sessionID: SessionID, in store: Store) async {
        guard let data = try? JSONEncoder().encode(self) else { return }
        try? await store.setSetting(
            Self.settingKey(sessionID: sessionID), String(decoding: data, as: UTF8.self)
        )
    }

    public static func load(sessionID: SessionID, from store: Store) async -> Self? {
        guard let stored = try? await store.setting(settingKey(sessionID: sessionID)),
              let decoded = try? JSONDecoder().decode(Self.self, from: Data(stored.utf8)) else {
            return nil
        }
        return Self(name: decoded.name, provider: decoded.provider, environment: decoded.environment)
    }

    public func merging(into base: [String: String]) -> [String: String] {
        guard let filtered = Self(
            name: name, provider: provider, environment: environment
        )?.environment else { return base }
        return base.merging(filtered) { _, account in account }
    }

    private static func environmentKey(for provider: String) -> String? {
        switch provider {
        case "claude": "CLAUDE_CONFIG_DIR"
        case "codex": "CODEX_HOME"
        default: nil
        }
    }
}

public struct SwarmAccountOption: Identifiable, Sendable, Hashable {
    public var id: String { selection.id }
    public var selection: SwarmAccountSelection
    public var label: String
    public var account: SwarmLaunchAccount?
    public var disabledReason: String?

    public static func choices(from list: SwarmAccountList) -> [Self] {
        guard !list.accounts.isEmpty else { return [] }
        var result: [Self] = []
        if let automatic = SwarmLaunchAccount.resolve(.auto, from: list),
           let source = list.accounts.first(where: { $0.name == automatic.name }) {
            result.append(Self(
                selection: .auto,
                label: "Auto (\(automatic.name))\(remainingLabel(source.remainingPct))",
                account: automatic,
                disabledReason: nil
            ))
        }
        result += list.accounts.map { account in
            let resolved = SwarmLaunchAccount.resolve(.named(account.name), from: list)
            let disabledReason: String? = if !account.signedIn {
                "Not signed in"
            } else if resolved == nil {
                "Account cannot launch in Bloom"
            } else {
                nil
            }
            return Self(
                selection: .named(account.name),
                label: "\(account.name)\(remainingLabel(account.remainingPct))",
                account: resolved,
                disabledReason: disabledReason
            )
        }
        return result
    }

    public static func initialSelection(in choices: [Self]) -> SwarmAccountSelection? {
        choices.first { $0.disabledReason == nil && $0.account != nil }?.selection
    }

    public static func account(
        for selection: SwarmAccountSelection?, in choices: [Self]
    ) -> SwarmLaunchAccount? {
        guard let selection else { return nil }
        return choices.first {
            $0.selection == selection && $0.disabledReason == nil
        }?.account
    }

    private static func remainingLabel(_ remaining: Int?) -> String {
        remaining.map { ", \($0)% left" } ?? ""
    }
}
