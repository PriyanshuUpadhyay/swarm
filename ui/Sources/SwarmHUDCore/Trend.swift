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

public enum Pressure: Int, Comparable, Sendable {
    case calm, warm, hot

    public static func < (a: Pressure, b: Pressure) -> Bool { a.rawValue < b.rawValue }
}

public struct WindowTrend: Equatable, Sendable {
    /// Percent per hour, or nil when there is not enough signal.
    public let burn: Double?
    /// The percent the window reaches at its reset at this pace.
    public let projected: Double?
    /// Hours until 100% at this pace.
    public let hoursToFull: Double?
    public let pressure: Pressure
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
    case .other(let id): return id
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

/// hot: full before the reset; warm: projected at 85% or more; calm otherwise.
func classify(pct: Int, burn: Double?, hoursToReset: Double?) -> WindowTrend {
    let projected = burn.flatMap { b in hoursToReset.map { Double(pct) + b * $0 } }
    let hoursToFull = burn.flatMap { $0 > 0 ? (100 - Double(pct)) / $0 : nil }
    let pressure: Pressure
    if let hoursToFull, let hoursToReset, hoursToFull < hoursToReset {
        pressure = .hot
    } else if let projected, projected >= 85 {
        pressure = .warm
    } else {
        pressure = .calm
    }
    return WindowTrend(burn: burn, projected: projected, hoursToFull: hoursToFull, pressure: pressure)
}

public func trend(account: UsageAccount, window: UsageWindow, history: [HistorySample], now: Date) -> WindowTrend {
    let hoursToReset = window.resetAt.map { max(0, $0.timeIntervalSince(now) / 3600) }
    let burn: Double?
    switch window.kind {
    case .session:
        let key = historyKey(window.kind)
        burn = burnRate(currentSegment(history.filter { $0.account == account.id && $0.window == key }),
                        lookbackHours: 1)
    case .week, .fable:
        burn = hoursToReset.flatMap { weeklyPace(pct: window.usedPct, hoursToReset: $0) }
    case .other:
        burn = nil
    }
    return classify(pct: window.usedPct, burn: burn, hoursToReset: hoursToReset)
}
