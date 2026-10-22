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
    public let resetCredits: ResetCredits?

    public init(provider: String, account: String?, label: String, window: String?, windowMinutes: Int?,
                usedPct: Int?, resetTimeSeconds: Double?, state: String, reason: String? = nil,
                reasonCode: String? = nil, asOfSeconds: Double? = nil, resetCredits: ResetCredits? = nil) {
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
        self.resetCredits = resetCredits
    }
}

/// A Codex account's usage limit resets (`reset_credits`): swarm already dropped the expired ones
/// and sorted the list with a credit that never expires (nil) last.
public struct ResetCredits: Decodable, Equatable, Sendable {
    public let available: Int
    /// Nil when Codex gave only the count; it can be shorter than `available` (Codex caps it).
    public let expiryTimeSeconds: [Double?]?

    public init(available: Int, expiryTimeSeconds: [Double?]?) {
        self.available = available
        self.expiryTimeSeconds = expiryTimeSeconds
    }

    /// "3 resets · expire Oct 4, Oct 23, no expiry"; a list shorter than the count ends in "…".
    /// Nil when there is no reset.
    public func text(timeZone: TimeZone = .current) -> String? {
        guard available > 0 else { return nil }
        let head = "\(available) \(available == 1 ? "reset" : "resets")"
        guard let list = expiryTimeSeconds, !list.isEmpty else { return head }
        var dates = list.map { $0.map { Self.day($0, timeZone) } ?? "no expiry" }
        if dates.count < available { dates.append("…") }
        return "\(head) · \(available == 1 ? "expires" : "expire") \(dates.joined(separator: ", "))"
    }

    /// "3 resets · next Oct 4", for the line under an account name.
    public func short(timeZone: TimeZone = .current) -> String? {
        guard available > 0 else { return nil }
        let head = "\(available) \(available == 1 ? "reset" : "resets")"
        guard let next = expiryTimeSeconds?.compactMap({ $0 }).first else { return head }
        return "\(head) · next \(Self.day(next, timeZone))"
    }

    private static func day(_ seconds: Double, _ timeZone: TimeZone) -> String {
        let formatter = DateFormatter()
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.timeZone = timeZone
        formatter.dateFormat = "MMM d"
        return formatter.string(from: Date(timeIntervalSince1970: seconds))
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

public enum WindowKind: Int, CaseIterable, Hashable, Sendable {
    case session, week, fable

    /// Claude names its windows; Codex names a slot (`primary`), so its length decides the kind.
    /// A window of another length has no column in the HUD, so it is left out.
    init?(window: String, minutes: Int?) {
        switch (window, minutes) {
        case ("5h", _): self = .session
        case ("7d", _): self = .week
        case ("fb", _): self = .fable
        case (_, 300?): self = .session
        case (_, 10080?): self = .week
        default: return nil
        }
    }

    public var title: String {
        switch self {
        case .session: return "Session"
        case .week: return "Week"
        case .fable: return "Fable"
        }
    }

    /// Columns in the open HUD, so the same window lines up across a provider's accounts.
    public var order: Int { rawValue }
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
    /// A Claude token can be renewed with a print-mode usage call, without a model turn.
    public let renewalArguments: [String]?
    /// Codex only; nil when the reader gave none.
    public var resetCredits: ResetCredits? = nil

    public var id: String { "\(provider)/\(name)" }
    public var worst: UsageWindow? { windows.max { ($0.usedPct, -$0.kind.order) < ($1.usedPct, -$1.kind.order) } }
}

/// One account per provider and name, windows in column order. A provider row with no account and
/// no source (AGY) is left out: it has nothing to show.
public func accounts(from meters: [UsageMeter]) -> [UsageAccount] {
    var order: [String] = []
    var grouped: [String: [UsageMeter]] = [:]
    // Only the providers the HUD can draw; a new provider shows once it has a mark.
    for meter in meters where meter.state != "no_source" && ["claude", "codex"].contains(meter.provider) {
        let key = "\(meter.provider)/\(meter.account ?? meter.label)"
        if grouped[key] == nil { order.append(key) }
        grouped[key, default: []].append(meter)
    }
    return order.map { key in
        let group = grouped[key]!
        var byKind: [WindowKind: UsageWindow] = [:]
        for meter in group {
            guard let window = meter.window, let pct = meter.usedPct,
                  let kind = WindowKind(window: window, minutes: meter.windowMinutes) else { continue }
            let reading = UsageWindow(
                kind: kind,
                usedPct: min(max(pct, 0), 100),
                resetAt: meter.resetTimeSeconds.map(Date.init(timeIntervalSince1970:)),
                asOf: meter.asOfSeconds.map(Date.init(timeIntervalSince1970:)),
                isFresh: meter.state == "fresh"
            )
            // Two Codex limits of one length share a column; the fuller one is the one to watch.
            if reading.usedPct > byKind[kind]?.usedPct ?? -1 { byKind[kind] = reading }
        }
        let windows = byKind.values.sorted { $0.kind.order < $1.kind.order }
        let first = group[0]
        return UsageAccount(
            provider: first.provider,
            name: first.account ?? first.label,
            windows: windows,
            problem: windows.isEmpty ? (group.compactMap(\.reason).first ?? "No reading") : nil,
            renewalArguments: first.provider == "claude" && group.contains(where: { $0.reasonCode == "token_expired" })
                ? ["run", "claude", "--account", first.account ?? first.label, "--", "-p", "/usage"] : nil,
            resetCredits: group.lazy.compactMap(\.resetCredits).first
        )
    }
}

/// One block of the open table: a provider's accounts under its own column titles.
public struct ProviderGroup: Equatable, Identifiable, Sendable {
    public let provider: String
    /// Only the windows some account of this provider has, in column order.
    public let kinds: [WindowKind]
    public let accounts: [UsageAccount]

    public var id: String { provider }
}

/// Claude, then Codex, then any other provider; accounts keep the reader's order inside a group.
public func providerGroups(_ accounts: [UsageAccount]) -> [ProviderGroup] {
    var providers: [String] = []
    for account in accounts where !providers.contains(account.provider) { providers.append(account.provider) }
    let known = ["claude", "codex"]
    return (known.filter(providers.contains) + providers.filter { !known.contains($0) }).map { provider in
        let members = accounts.filter { $0.provider == provider }
        let kinds = Set(members.flatMap { $0.windows.map(\.kind) }).sorted { $0.order < $1.order }
        return ProviderGroup(provider: provider, kinds: kinds, accounts: members)
    }
}

/// The open card's one table: a column per limit kind any account has, so Week sits in one place
/// for every provider, and a provider without a kind shows a dash there.
public struct Ledger: Equatable, Sendable {
    public let kinds: [WindowKind]
    /// Accounts with a reading, Claude, then Codex, then any other provider.
    public let rows: [UsageAccount]
    /// Accounts with no reading, folded into one line.
    public let signedOut: [UsageAccount]
}

public func ledger(_ accounts: [UsageAccount]) -> Ledger {
    let known = ["claude", "codex"]
    var providers = known
    for account in accounts where !providers.contains(account.provider) { providers.append(account.provider) }
    let ordered = providers.flatMap { provider in accounts.filter { $0.provider == provider } }
    return Ledger(kinds: Set(accounts.flatMap { $0.windows.map(\.kind) }).sorted { $0.order < $1.order },
                  rows: ordered.filter { !$0.windows.isEmpty }, signedOut: ordered.filter(\.windows.isEmpty))
}

/// "2h 44m", "3d 20h", "12m", "now": the two largest units, for a reset time.
public func shortDuration(until date: Date, now: Date) -> String {
    let interval = date.timeIntervalSince(now)
    // The reset time comes from a provider reply; Int(Double) traps on a value it cannot hold.
    guard interval.isFinite, interval >= 60 else { return "now" }
    let seconds = Int(min(interval, 99 * 86400))
    let days = seconds / 86400, hours = seconds % 86400 / 3600, minutes = seconds % 3600 / 60
    if days > 0 { return hours > 0 ? "\(days)d \(hours)h" : "\(days)d" }
    if hours > 0 { return minutes > 0 ? "\(hours)h \(minutes)m" : "\(hours)h" }
    return "\(minutes)m"
}
