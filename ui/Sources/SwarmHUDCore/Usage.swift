import Foundation

/// One meter of `swarm usage --json` (`docs/usage-format.md`, schema `swarm.usage.v1`).
public struct UsageMeter: Decodable, Equatable, Sendable {
    public let provider: String
    public let account: String?
    public let label: String
    public let window: String?
    public let windowMinutes: Int?
    public let usedPct: Int?
    public let resetTimeSeconds: Double?
    public let state: String
    public let reason: String?
    public let reasonCode: String?
    public let asOfSeconds: Double?

    public init(provider: String, account: String?, label: String, window: String?, windowMinutes: Int?,
                usedPct: Int?, resetTimeSeconds: Double?, state: String, reason: String? = nil,
                reasonCode: String? = nil, asOfSeconds: Double? = nil) {
        self.provider = provider
        self.account = account
        self.label = label
        self.window = window
        self.windowMinutes = windowMinutes
        self.usedPct = usedPct
        self.resetTimeSeconds = resetTimeSeconds
        self.state = state
        self.reason = reason
        self.reasonCode = reasonCode
        self.asOfSeconds = asOfSeconds
    }
}

public struct UsageSnapshot: Decodable, Sendable {
    public let schema: String
    public let meters: [UsageMeter]

    /// A reader of `v1` ignores fields it does not know, and refuses another major version.
    public static func decode(_ data: Data) throws -> UsageSnapshot {
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        let snapshot = try decoder.decode(UsageSnapshot.self, from: data)
        guard snapshot.schema == "swarm.usage.v1" else {
            throw DecodingError.dataCorrupted(.init(codingPath: [], debugDescription: "unknown schema \(snapshot.schema)"))
        }
        return snapshot
    }
}

public enum WindowKind: Equatable, Hashable, Sendable {
    case session, week, fable
    case other(String)

    /// Claude names its windows; Codex names a slot (`primary`), so its length decides the kind.
    init(window: String, minutes: Int?) {
        switch (window, minutes) {
        case ("5h", _): self = .session
        case ("7d", _): self = .week
        case ("fb", _): self = .fable
        case (_, 300?): self = .session
        case (_, 10080?): self = .week
        default: self = .other(window)
        }
    }

    public var title: String {
        switch self {
        case .session: return "Session"
        case .week: return "Week"
        case .fable: return "Fable"
        case .other(let id): return id
        }
    }

    /// Columns in the open HUD, so the same window lines up across accounts.
    public var order: Int {
        switch self {
        case .session: return 0
        case .week: return 1
        case .fable: return 2
        case .other: return 3
        }
    }
}

public enum Severity: Int, Comparable, Sendable {
    case calm, warn, critical

    /// Fixed in every theme, so a warning is never recoloured by the time of day.
    public init(pct: Int) {
        self = pct >= 95 ? .critical : pct >= 80 ? .warn : .calm
    }

    public static func < (a: Severity, b: Severity) -> Bool { a.rawValue < b.rawValue }
}

public struct UsageWindow: Equatable, Identifiable, Sendable {
    public let kind: WindowKind
    public let usedPct: Int
    public let resetAt: Date?
    public let asOf: Date?
    /// `fresh` is a reading from the last 300 s; anything else is shown with its age.
    public let isFresh: Bool

    public var id: WindowKind { kind }
    public var severity: Severity { Severity(pct: usedPct) }
}

public struct UsageAccount: Equatable, Identifiable, Sendable {
    public let provider: String
    public let name: String
    public let windows: [UsageWindow]
    /// Why the account has no reading, such as "Token expired"; nil when it has one.
    public let problem: String?

    public var id: String { "\(provider)/\(name)" }
    public var worst: UsageWindow? { windows.max { ($0.usedPct, -$0.kind.order) < ($1.usedPct, -$1.kind.order) } }
}

/// One account per provider and name, windows in column order. A provider row with no account and
/// no source (AGY) is left out: it has nothing to show.
public func accounts(from meters: [UsageMeter]) -> [UsageAccount] {
    var order: [String] = []
    var grouped: [String: [UsageMeter]] = [:]
    for meter in meters where meter.state != "no_source" {
        let key = "\(meter.provider)/\(meter.account ?? meter.label)"
        if grouped[key] == nil { order.append(key) }
        grouped[key, default: []].append(meter)
    }
    return order.map { key in
        let group = grouped[key]!
        let windows = group.compactMap { meter -> UsageWindow? in
            guard let window = meter.window, let pct = meter.usedPct else { return nil }
            return UsageWindow(
                kind: WindowKind(window: window, minutes: meter.windowMinutes),
                usedPct: min(max(pct, 0), 100),
                resetAt: meter.resetTimeSeconds.map(Date.init(timeIntervalSince1970:)),
                asOf: meter.asOfSeconds.map(Date.init(timeIntervalSince1970:)),
                isFresh: meter.state == "fresh"
            )
        }.sorted { $0.kind.order < $1.kind.order }
        let first = group[0]
        return UsageAccount(
            provider: first.provider,
            name: first.account ?? first.label,
            windows: windows,
            problem: windows.isEmpty ? (group.compactMap(\.reason).first ?? "No reading") : nil
        )
    }
}

/// The window the closed HUD shows: the highest use, and on a tie the first in list order, so the
/// wings do not jump between equal readings.
public func worstWindow(_ accounts: [UsageAccount]) -> (account: UsageAccount, window: UsageWindow)? {
    var best: (account: UsageAccount, window: UsageWindow)?
    for account in accounts {
        for window in account.windows where window.usedPct > (best?.window.usedPct ?? -1) {
            best = (account, window)
        }
    }
    return best
}

/// "2h 44m", "3d 20h", "12m", "now": the two largest units, for a reset time.
public func shortDuration(until date: Date, now: Date) -> String {
    let seconds = Int(date.timeIntervalSince(now))
    guard seconds > 59 else { return "now" }
    let days = seconds / 86400, hours = seconds % 86400 / 3600, minutes = seconds % 3600 / 60
    if days > 0 { return hours > 0 ? "\(days)d \(hours)h" : "\(days)d" }
    if hours > 0 { return minutes > 0 ? "\(hours)h \(minutes)m" : "\(hours)h" }
    return "\(minutes)m"
}
