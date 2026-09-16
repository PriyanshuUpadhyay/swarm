import Foundation

public struct SwarmUsageBoard: Sendable, Hashable {
    public struct Provider: Sendable, Hashable {
        public var key: String
        public var title: String
        public var kind: AgentKind?
        public var accounts: [Account]
    }

    public struct Account: Sendable, Hashable {
        public var key: String
        public var title: String
        public var meters: [Meter]
    }

    public struct Meter: Sendable, Hashable {
        public var sourceLabel: String
        public var window: String?
        public var usedPercent: Int?
        public var resetsIn: String?
        public var state: String
        public var reason: String?
        public var usedText: String?
        public var fill: Double?

        public var resetText: String? { resetsIn.map { "Resets in \($0)" } }
        public var isStale: Bool { state.caseInsensitiveCompare("stale") == .orderedSame }
        public var statusText: String? { window != nil && usedPercent != nil && isStale ? "Stale" : nil }
        public var severity: QuotaSeverity? {
            usedPercent.map { QuotaSeverity.of(Double($0) / 100) }
        }
        public var message: String? {
            guard window == nil || usedPercent == nil else { return nil }
            let detail = reason ?? Self.readable(state)
            return window.map { "\($0) · \(detail)" } ?? detail
        }

        fileprivate init(_ meter: SwarmUsageMeter, style: UsageMeterStyle) {
            sourceLabel = meter.label
            window = meter.window
            usedPercent = meter.usedPct
            resetsIn = meter.resetsIn
            state = meter.state
            reason = meter.reason
            if let used = meter.usedPct {
                let clamped = min(max(used, 0), 100)
                usedText = style == .left ? "\(100 - clamped)% left" : "\(clamped)% used"
                fill = Double(style == .left ? 100 - clamped : clamped) / 100
            } else {
                usedText = nil
                fill = nil
            }
        }

        private static func readable(_ state: String) -> String {
            state.replacingOccurrences(of: "_", with: " ").capitalized
        }
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
                        meter.severity?.word, meter.message, meter.resetText, meter.statusText,
                    ].compactMap { $0 }.joined(separator: ", ")
                }
            }
        }.joined(separator: ". ")
    }

    /// Groups meters by provider and account, then applies the saved menu layout.
    public static func make(
        from meters: [SwarmUsageMeter],
        options: UsageDisplayOptions = UsageDisplayOptions(),
        layout: UsageLayout = UsageLayout()
    ) -> SwarmUsageBoard {
        let providerPositions = Dictionary(
            uniqueKeysWithValues: layout.orderedProviders().enumerated().map { ($1, $0) }
        )
        let metricPositions = Dictionary(
            uniqueKeysWithValues: layout.metricOrder.enumerated().map { ($1, $0) }
        )
        let byProvider = Dictionary(grouping: meters, by: \.provider)
        let providers = byProvider.compactMap { provider, sourceMeters -> Provider? in
            let kind = providerKind(provider)
            guard kind.map(layout.isEnabled) ?? true else { return nil }
            let visible = visibleMeters(sourceMeters, provider: kind, layout: layout)
            let byAccount = Dictionary(grouping: visible) { meter in
                meter.account.map { "account/\($0)" } ?? "label/\(meter.label)"
            }
            let accounts = byAccount.map { key, meters in
                let first = meters[0]
                return Account(
                    key: key,
                    title: first.account ?? first.label,
                    meters: meters.map { Meter($0, style: options.meterStyle) }
                        .sorted { meterComesFirst($0, $1, kind: kind, positions: metricPositions) }
                )
            }.sorted(by: accountComesFirst)
            guard !accounts.isEmpty else { return nil }
            return Provider(key: provider, title: providerTitle(provider), kind: kind, accounts: accounts)
        }.sorted { providerComesFirst($0, $1, positions: providerPositions) }
        return SwarmUsageBoard(providers: providers)
    }

    private static func visibleMeters(
        _ meters: [SwarmUsageMeter],
        provider: AgentKind?,
        layout: UsageLayout
    ) -> [SwarmUsageMeter] {
        guard let provider else { return meters }
        let status = meters.filter { metricID(for: $0, provider: provider) == nil }
        let measured = meters.compactMap { meter -> (meter: SwarmUsageMeter, id: UsageMetricID)? in
            guard let id = metricID(for: meter, provider: provider), !layout.isHidden(id) else { return nil }
            return (meter, id)
        }
        var always = measured.filter { layout.placement(of: $0.id) == .alwaysVisible }.map(\.meter)
        var demand = measured.filter { layout.placement(of: $0.id) == .onDemand }.map(\.meter)
        if always.isEmpty, !demand.isEmpty {
            always = demand
            demand = []
        }
        return status + always + (layout.expandedProviders.contains(provider) ? demand : [])
    }

    private static func metricID(for meter: SwarmUsageMeter, provider: AgentKind) -> UsageMetricID? {
        metricID(window: meter.window, provider: provider)
    }

    private static func metricID(window: String?, provider: AgentKind) -> UsageMetricID? {
        guard let window = window?.lowercased() else { return nil }
        let key: String
        switch (provider, window) {
        case (.claudeCode, "5h"): key = "five_hour"
        case (.claudeCode, "7d"): key = "seven_day"
        case (.codex, "5h"): key = "session"
        case (.codex, "7d"): key = "weekly"
        default: key = window
        }
        return UsageMetricID("\(provider.rawValue)/\(key)")
    }

    private static func providerKind(_ provider: String) -> AgentKind? {
        switch provider.lowercased() {
        case "claude": .claudeCode
        case "codex": .codex
        default: nil
        }
    }

    private static func providerTitle(_ provider: String) -> String {
        switch provider.lowercased() {
        case "claude": "Claude"
        case "codex": "Codex"
        case "agy": "AGY"
        default: provider.prefix(1).uppercased() + provider.dropFirst()
        }
    }

    private static func providerComesFirst(
        _ lhs: Provider,
        _ rhs: Provider,
        positions: [AgentKind: Int]
    ) -> Bool {
        switch (lhs.kind.flatMap { positions[$0] }, rhs.kind.flatMap { positions[$0] }) {
        case (let left?, let right?) where left != right: return left < right
        case (_?, nil): return true
        case (nil, _?): return false
        default: return ordered(lhs.title, before: rhs.title)
        }
    }

    private static func accountComesFirst(_ lhs: Account, _ rhs: Account) -> Bool {
        if lhs.title.caseInsensitiveCompare(rhs.title) != .orderedSame {
            return ordered(lhs.title, before: rhs.title)
        }
        return ordered(lhs.key, before: rhs.key)
    }

    private static func meterComesFirst(
        _ lhs: Meter,
        _ rhs: Meter,
        kind: AgentKind?,
        positions: [UsageMetricID: Int]
    ) -> Bool {
        if lhs.window == nil, rhs.window != nil { return true }
        if lhs.window != nil, rhs.window == nil { return false }
        let leftID = kind.flatMap { metricID(window: lhs.window, provider: $0) }
        let rightID = kind.flatMap { metricID(window: rhs.window, provider: $0) }
        switch (leftID.flatMap { positions[$0] }, rightID.flatMap { positions[$0] }) {
        case (let left?, let right?) where left != right: return left < right
        case (_?, nil): return true
        case (nil, _?): return false
        default: break
        }
        let order = ["5h": 0, "7d": 1, "fb": 2]
        let leftWindow = lhs.window ?? ""
        let rightWindow = rhs.window ?? ""
        let left = order[leftWindow.lowercased()] ?? Int.max
        let right = order[rightWindow.lowercased()] ?? Int.max
        if left != right { return left < right }
        if leftWindow.caseInsensitiveCompare(rightWindow) != .orderedSame {
            return ordered(leftWindow, before: rightWindow)
        }
        return ordered(lhs.sourceLabel, before: rhs.sourceLabel)
    }

    private static func ordered(_ lhs: String, before rhs: String) -> Bool {
        let left = lhs.lowercased()
        let right = rhs.lowercased()
        return left == right ? lhs < rhs : left < right
    }
}
