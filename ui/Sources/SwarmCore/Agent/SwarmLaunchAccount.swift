import Foundation

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
        guard let key = Self.environmentKey(for: provider) else { return nil }
        if !(provider == "claude" && name == "default" && environment.isEmpty) {
            guard let value = environment[key], !value.isEmpty else { return nil }
        }
        self.name = name
        self.provider = provider
        let allowed: Set<String> = provider == "claude"
            ? ["CLAUDE_CONFIG_DIR", "CLAUDE_SECURESTORAGE_CONFIG_DIR", "AGENT_PROFILE_LABEL"] : ["CODEX_HOME"]
        guard environment.keys.allSatisfy(allowed.contains),
              environment.values.allSatisfy({ !$0.contains("\0") && !$0.contains("\n") && !$0.contains("\r") }) else { return nil }
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
        guard list.state == .ready, let account = list.accounts.first(where: { $0.name == name }),
              account.authState == .signedIn else { return nil }
        return Self(name: account.name, provider: list.provider, environment: account.env)
    }

    public func merging(into base: [String: String]) -> [String: String] {
        base.merging(environment) { _, account in account }
    }

    private static func environmentKey(for provider: String) -> String? {
        switch provider {
        case "claude": "CLAUDE_CONFIG_DIR"
        case "codex": "CODEX_HOME"
        default: nil
        }
    }
}

public struct SwarmAccountLoadDecision: Sendable, Hashable {
    public var options: [SwarmAccountOption]
    public var selection: SwarmAccountSelection?
    public var fallbackCaption: String?

    public static func loaded(_ list: SwarmAccountList) -> Self {
        guard list.state != .unavailable else { return fallback("Account status unavailable") }
        guard !list.accounts.isEmpty else {
            return Self(options: [], selection: nil, fallbackCaption: nil)
        }
        let options = SwarmAccountOption.choices(from: list)
        guard let selection = SwarmAccountOption.initialSelection(in: options) else {
            return fallback("No signed-in accounts")
        }
        return Self(options: options, selection: selection, fallbackCaption: nil)
    }

    public static func failed(_ error: SwarmProfileError) -> Self {
        switch error {
        case .unavailable(let message), .failed(let message):
            fallback("Accounts unavailable: \(message)")
        }
    }

    public static func failed(message: String) -> Self {
        fallback("Accounts unavailable: \(message)")
    }

    private static func fallback(_ reason: String) -> Self {
        let separator = reason.hasSuffix(".") ? "" : "."
        return Self(
            options: [],
            selection: nil,
            fallbackCaption: "\(reason)\(separator) Using this repository's settings."
        )
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
            let disabledReason: String? = if account.authState == .unavailable {
                "Status unavailable"
            } else if account.authState == .signedOut {
                "Not signed in"
            } else if resolved == nil {
                "Account cannot launch in Swarm"
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

    /// Pin the displayed Auto result so Switch Model starts the home that the picker showed.
    public static func launchSelection(
        for selection: SwarmAccountSelection?, in choices: [Self]
    ) -> SwarmAccountSelection? {
        account(for: selection, in: choices).map { .named($0.name) } ?? selection
    }

    private static func remainingLabel(_ remaining: Double?) -> String {
        remaining.map { ", \($0.formatted(.number.precision(.fractionLength(0...1))))% left" } ?? ""
    }
}
