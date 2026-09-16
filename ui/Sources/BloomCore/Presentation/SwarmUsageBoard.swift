import Foundation

public struct SwarmUsageBoard: Sendable, Hashable {
    public struct Provider: Sendable, Hashable {
        public var key: String
        public var title: String
        public var accounts: [Account]
    }

    public struct Account: Sendable, Hashable {
        public var key: String
        public var title: String
        public var meters: [Meter]
    }

    public struct Meter: Sendable, Hashable {
        public var window: String
        public var usedPercent: Int
        public var resetsIn: String?
        public var isStale: Bool

        public var usedText: String { "\(usedPercent)% used" }
        public var resetText: String? { resetsIn.map { "Resets in \($0)" } }
        public var statusText: String? { isStale ? "Stale" : nil }
        public var fill: Double { min(max(Double(usedPercent) / 100, 0), 1) }
    }

    public var providers: [Provider]

    public init(providers: [Provider] = []) {
        self.providers = providers
    }

    public var isEmpty: Bool { providers.isEmpty }

    public var accessibilityLabel: String {
        providers.flatMap { provider in
            provider.accounts.flatMap { account in
                account.meters.map { meter in
                    [
                        provider.title, account.title, meter.window, meter.usedText,
                        meter.resetText, meter.statusText,
                    ].compactMap { $0 }.joined(separator: ", ")
                }
            }
        }.joined(separator: ". ")
    }

    /// Groups meters by provider and account, with known providers and windows first.
    public static func make(from meters: [SwarmUsageMeter]) -> SwarmUsageBoard {
        let byProvider = Dictionary(grouping: meters, by: \.provider)
        let providers = byProvider.map { provider, meters in
            let byAccount = Dictionary(grouping: meters) { meter in
                meter.account.map { "account/\($0)" } ?? "label/\(meter.label)"
            }
            let accounts = byAccount.map { key, meters in
                let first = meters[0]
                return Account(
                    key: key,
                    title: first.account ?? first.label,
                    meters: meters.map(Meter.init).sorted(by: meterComesFirst)
                )
            }.sorted(by: accountComesFirst)
            return Provider(key: provider, title: providerTitle(provider), accounts: accounts)
        }.sorted(by: providerComesFirst)
        return SwarmUsageBoard(providers: providers)
    }

    private static func providerTitle(_ provider: String) -> String {
        switch provider.lowercased() {
        case "claude": "Claude"
        case "codex": "Codex"
        case "agy": "AGY"
        default: provider.prefix(1).uppercased() + provider.dropFirst()
        }
    }

    private static func providerComesFirst(_ lhs: Provider, _ rhs: Provider) -> Bool {
        let order = ["claude": 0, "codex": 1, "agy": 2]
        let left = order[lhs.key.lowercased()] ?? Int.max
        let right = order[rhs.key.lowercased()] ?? Int.max
        if left != right { return left < right }
        return ordered(lhs.title, before: rhs.title)
    }

    private static func accountComesFirst(_ lhs: Account, _ rhs: Account) -> Bool {
        ordered(lhs.title, before: rhs.title)
    }

    private static func meterComesFirst(_ lhs: Meter, _ rhs: Meter) -> Bool {
        let order = ["5h": 0, "7d": 1, "fb": 2]
        let left = order[lhs.window.lowercased()] ?? Int.max
        let right = order[rhs.window.lowercased()] ?? Int.max
        if left != right { return left < right }
        return ordered(lhs.window, before: rhs.window)
    }

    private static func ordered(_ lhs: String, before rhs: String) -> Bool {
        let left = lhs.lowercased()
        let right = rhs.lowercased()
        return left == right ? lhs < rhs : left < right
    }
}

private extension SwarmUsageBoard.Meter {
    init(_ meter: SwarmUsageMeter) {
        window = meter.window
        usedPercent = meter.usedPct
        resetsIn = meter.resetsIn
        isStale = meter.state == "stale"
    }
}
