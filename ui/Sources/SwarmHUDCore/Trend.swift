import Foundation

// Trend math ported from yelo apps/UsageHUD (Pressure.swift, HistoryStore.swift). The thresholds
// below were tuned on real traces there.

public struct HistorySample: Codable, Equatable, Sendable {
    public let account: String
    public let window: String
    public let pct: Int
    public let asOf: Double

    public init(account: String, window: String, pct: Int, asOf: Double) {
        self.account = account
        self.window = window
        self.pct = pct
        self.asOf = asOf
    }
}

/// Where a window is heading at its current pace.
public enum Pace: Equatable, Sendable {
    /// At 100% now.
    case full
    /// Reaches 100% this many seconds from now, before its reset (or with no reset time known).
    case fillsIn(TimeInterval)
    /// Reaches its reset first.
    case lasts

    /// Red when it fills within the hour or the window is already critical, else orange; a window
    /// that lasts is calm.
    public func severity(window: UsageWindow) -> Severity {
        switch self {
        case .full: return .critical
        case .lasts: return .calm
        case .fillsIn(let seconds): return seconds < 3600 || window.severity == .critical ? .critical : .warn
        }
    }
}

/// A drop larger than this from one sample to the next means the window reset.
private let resetDrop = 10
private let minBurnSpanMinutes = 10.0
/// A lone 1% tick between two samples is rounding noise.
private let minBurnDelta = 2

public func historyKey(_ window: WindowKind) -> String {
    switch window {
    case .session: return "session"
    case .week: return "week"
    case .fable: return "fable"
    }
}

/// Samples from every window with a reading time. A stale meter still carries the time of its real
/// reading, and duplicates of one reading collapse in `appendHistory`.
public func samples(from accounts: [UsageAccount]) -> [HistorySample] {
    accounts.flatMap { account in
        account.windows.compactMap { window in
            window.asOf.map {
                HistorySample(account: account.id, window: historyKey(window.kind), pct: window.usedPct,
                              asOf: $0.timeIntervalSince1970)
            }
        }
    }
}

/// Adds new samples, drops repeats of one reading, keeps 7 days and at most 5000 samples.
public func appendHistory(_ history: [HistorySample], _ new: [HistorySample], now: Date) -> [HistorySample] {
    let cutoff = now.timeIntervalSince1970 - 7 * 86400
    var seen = Set<String>()
    let kept = (history + new)
        .filter { $0.asOf >= cutoff && seen.insert("\($0.account)|\($0.window)|\($0.asOf)").inserted }
        .sorted { $0.asOf < $1.asOf }
    return Array(kept.suffix(5000))
}

/// The samples since the last reset, oldest first.
func currentSegment(_ samples: [HistorySample]) -> [HistorySample] {
    let sorted = samples.sorted { $0.asOf < $1.asOf }
    guard sorted.count > 1 else { return sorted }
    var start = 0
    for i in 1..<sorted.count where sorted[i - 1].pct - sorted[i].pct > resetDrop { start = i }
    return Array(sorted[start...])
}

func burnRate(_ segment: [HistorySample], lookbackHours: Double) -> Double? {
    guard segment.count >= 2, let latest = segment.last else { return nil }
    let cutoff = latest.asOf - lookbackHours * 3600
    guard let earliest = segment.first(where: { $0.asOf >= cutoff }) else { return nil }
    let span = latest.asOf - earliest.asOf
    guard span >= minBurnSpanMinutes * 60, abs(latest.pct - earliest.pct) >= minBurnDelta else { return nil }
    return Double(latest.pct - earliest.pct) / (span / 3600)
}

/// A week's pace is its whole-window average: an hour's burn stretched over days over-warns. No pace
/// before 24 h of the window have passed, because the division explodes early.
func weeklyPace(pct: Int, hoursToReset: Double) -> Double? {
    let elapsed = 168 - hoursToReset
    return elapsed >= 24 ? Double(pct) / elapsed : nil
}

/// nil while there is not enough signal for a pace.
public func pace(account: UsageAccount, window: UsageWindow, history: [HistorySample], now: Date) -> Pace? {
    if window.usedPct >= 100 { return .full }
    let hoursToReset = window.resetAt.map { max(0, $0.timeIntervalSince(now) / 3600) }
    let burn: Double?
    switch window.kind {
    case .session:
        let key = historyKey(window.kind)
        burn = burnRate(currentSegment(history.filter { $0.account == account.id && $0.window == key }),
                        lookbackHours: 1)
    case .week, .fable:
        burn = hoursToReset.flatMap { weeklyPace(pct: window.usedPct, hoursToReset: $0) }
    }
    guard let burn else { return nil }
    guard burn > 0 else { return .lasts }
    let hoursToFull = (100 - Double(window.usedPct)) / burn
    if let hoursToReset, hoursToFull >= hoursToReset { return .lasts }
    return .fillsIn(hoursToFull * 3600)
}

/// The account to work in next, for the closed wing and the open status line.
public struct BestAccount: Equatable, Sendable {
    public enum Outlook: Equatable, Sendable {
        /// The shortest time to 100% among its windows that fill before their reset.
        case left(TimeInterval)
        /// Every window lasts to its reset or has no pace yet.
        case lasts
        /// Every account has a full window; `account` is usable again first, at this time (nil: no
        /// reset time known).
        case allFull(resetAt: Date?)
    }

    public let account: UsageAccount
    public let outlook: Outlook
}

/// The signed-in account whose fullest window is the lowest; on a tie the first in reader order.
public func bestAccount(_ accounts: [UsageAccount], history: [HistorySample], now: Date) -> BestAccount? {
    let ready = accounts.filter { !$0.windows.isEmpty }
    let fullest = { (account: UsageAccount) in account.windows.map(\.usedPct).max() ?? 0 }
    var best: UsageAccount?
    for account in ready where fullest(account) < best.map(fullest) ?? Int.max { best = account }
    guard let best else { return nil }
    if fullest(best) >= 100 {
        // An account is usable again when the last of its full windows resets.
        let freed = ready.map { account -> (UsageAccount, Date?) in
            let full = account.windows.filter { $0.usedPct >= 100 }
            return (account, full.contains { $0.resetAt == nil } ? nil : full.compactMap(\.resetAt).max())
        }
        let first = freed.filter { $0.1 != nil }.min { $0.1! < $1.1! }
        return BestAccount(account: first?.0 ?? best, outlook: .allFull(resetAt: first?.1))
    }
    let left = best.windows.compactMap { window -> TimeInterval? in
        if case .fillsIn(let seconds) = pace(account: best, window: window, history: history, now: now) { return seconds }
        return nil
    }.min()
    return BestAccount(account: best, outlook: left.map { .left($0) } ?? .lasts)
}

/// "3h left", "40m left": whole units, rounded down, so it never promises more time than there is.
public func leftText(_ seconds: TimeInterval) -> String {
    let seconds = seconds.isFinite ? min(max(seconds, 0), 999 * 3600) : 0
    return seconds >= 3600 ? "\(Int(seconds / 3600))h left" : "\(max(Int(seconds / 60), 1))m left"
}

/// `swarm working --json`: running agents per provider. A provider with none is absent.
public struct WorkingSnapshot: Decodable, Sendable {
    public let schema: String
    public let working: [String: Int]

    /// Any failure or another schema reads as nothing working; the wing only loses its motion.
    public static func counts(_ data: Data) -> [String: Int] {
        guard let snapshot = try? JSONDecoder().decode(WorkingSnapshot.self, from: data),
              snapshot.schema == "swarm.working.v1" else { return [:] }
        return snapshot.working.filter { $0.value > 0 }
    }
}

/// The provider the closed wing shows: the one with the most working agents (claude first on a tie),
/// else the best account's. Only providers with a mark count.
public func markProvider(working: [String: Int], best: BestAccount?) -> String? {
    let busiest = ["claude", "codex"].filter { working[$0, default: 0] > 0 }
        .max { working[$0]! < working[$1]! }
    return busiest ?? best?.account.provider
}
