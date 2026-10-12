import Foundation

public enum UsagePressureLevel: Sendable, Equatable { case red, amber, green }

/// The tint of a bar and its text. A percent alone carries no trend, so it maps through fixed bands.
public enum UsageSeverity: Sendable, Equatable {
    case calm, warn, danger

    /// At or above this percent a meter is danger, whatever its trend.
    public static let dangerPct = 90.0

    public init(usedPct: Double, level: UsagePressureLevel?) {
        if usedPct >= Self.dangerPct || level == .red {
            self = .danger
        } else if usedPct >= 70 || level == .amber {
            self = .warn
        } else {
            self = .calm
        }
    }
}

public struct UsageRowPressure: Sendable, Equatable {
    /// `SwarmUsageMeter.id` of the meter this row describes.
    public let meterID: String
    public let usedPct: Double
    /// nil means no trend yet.
    public let burnPctPerHour: Double?
    public let hoursToReset: Double?
    public let projectedPct: Double?
    public let hoursToFull: Double?
    public let level: UsagePressureLevel

    public init(
        meterID: String, usedPct: Double, burnPctPerHour: Double?, hoursToReset: Double?,
        projectedPct: Double?, hoursToFull: Double?, level: UsagePressureLevel
    ) {
        self.meterID = meterID
        self.usedPct = usedPct
        self.burnPctPerHour = burnPctPerHour
        self.hoursToReset = hoursToReset
        self.projectedPct = projectedPct
        self.hoursToFull = hoursToFull
        self.level = level
    }
}

/// What the one-glance tint comes from. With no trend, or with no reset time to project to, the
/// level is an empty `.green`, so the raw percent band decides. Otherwise the level decides.
public enum UsageBubbleBasis: Sendable, Equatable {
    case severity(Double)
    case pressure(UsagePressureLevel)

    public var severity: UsageSeverity {
        switch self {
        case .severity(let usedPct): UsageSeverity(usedPct: usedPct, level: nil)
        case .pressure(.red): .danger
        case .pressure(.amber): .warn
        case .pressure(.green): .calm
        }
    }
}

/// Trend math for usage meters: how fast a window fills, and whether it fills before it resets.
public enum UsagePressure {
    /// The two window lengths that have a trend rule and a title of their own.
    public static let fiveHourMinutes = 300
    public static let weekMinutes = 10_080
    /// A drop of more than this from the previous sample means the window reset.
    private static let resetDropPct = 10.0
    /// A shorter span is noise, not a trend.
    private static let minBurnSpanSeconds = 600
    /// The CLI rounds to whole points, so one point between two samples is rounding noise.
    private static let minBurnDeltaPct = 2.0

    /// The samples of one meter since its window last reset, oldest first.
    public static func currentSegment(_ samples: [UsageHistorySample]) -> [UsageHistorySample] {
        let sorted = samples.sorted { $0.asOfSeconds < $1.asOfSeconds }
        guard sorted.count > 1 else { return sorted }
        var start = 0
        for index in 1..<sorted.count where sorted[index - 1].usedPct - sorted[index].usedPct > resetDropPct {
            start = index
        }
        return Array(sorted[start...])
    }

    /// Percent per hour from the oldest sample inside the lookback to the latest one.
    public static func burnRate(_ segment: [UsageHistorySample], lookbackHours: Double) -> Double? {
        guard segment.count >= 2, let latest = segment.last else { return nil }
        let cutoff = Double(latest.asOfSeconds) - lookbackHours * 3_600
        guard let earliest = segment.first(where: { Double($0.asOfSeconds) >= cutoff }) else { return nil }
        let spanSeconds = latest.asOfSeconds - earliest.asOfSeconds
        guard spanSeconds >= minBurnSpanSeconds,
              abs(latest.usedPct - earliest.usedPct) >= minBurnDeltaPct else { return nil }
        return (latest.usedPct - earliest.usedPct) / (Double(spanSeconds) / 3_600)
    }

    /// The average pace of a weekly window. One active hour projected across days turns every
    /// weekly meter red, so the weekly trend is percent over hours elapsed. The first 24 hours give
    /// no trend, because the division is too large there.
    public static func weeklyAveragePace(usedPct: Double, hoursToReset: Double) -> Double? {
        let elapsed = 168 - hoursToReset
        guard elapsed >= 24 else { return nil }
        return usedPct / elapsed
    }

    /// Red when the window fills before it resets. Amber when the projection at reset is 85 or more.
    public static func classify(
        usedPct: Double, burn: Double?, hoursToReset: Double?
    ) -> (projectedPct: Double?, hoursToFull: Double?, level: UsagePressureLevel) {
        let projected = burn.flatMap { burn in hoursToReset.map { usedPct + burn * $0 } }
        let hoursToFull = burn.flatMap { $0 > 0 ? (100 - usedPct) / $0 : nil }
        let level: UsagePressureLevel
        if let hoursToFull, let hoursToReset, hoursToFull < hoursToReset {
            level = .red
        } else if let projected, projected >= 85 {
            level = .amber
        } else {
            level = .green
        }
        return (projected, hoursToFull, level)
    }

    /// One row per fresh meter. An old or failed meter gives no row, so it cannot project a trend.
    public static func rows(
        meters: [SwarmUsageMeter], history: [UsageHistorySample], nowSeconds: Int
    ) -> [UsageRowPressure] {
        meters.compactMap { meter in
            guard meter.state == .fresh, let window = meter.window, let usedPct = meter.usedPct else { return nil }
            let hoursToReset = meter.resetTimeSeconds.map { Double(max(0, $0 - nowSeconds)) / 3_600 }
            let burn: Double?
            switch meter.windowMinutes {
            case fiveHourMinutes:
                let samples = history.filter {
                    $0.provider == meter.provider && $0.account == meter.account ?? "" && $0.window == window
                }
                burn = burnRate(currentSegment(samples), lookbackHours: 1)
            case weekMinutes:
                burn = hoursToReset.flatMap { weeklyAveragePace(usedPct: usedPct, hoursToReset: $0) }
            default:
                // A window of unknown length gets no trend, not a guess.
                burn = nil
            }
            let result = classify(usedPct: usedPct, burn: burn, hoursToReset: hoursToReset)
            return UsageRowPressure(
                meterID: meter.id, usedPct: usedPct, burnPctPerHour: burn, hoursToReset: hoursToReset,
                projectedPct: result.projectedPct, hoursToFull: result.hoursToFull, level: result.level
            )
        }
    }

    /// The row that the one-glance tint follows: the worst tint, then the worse level, then the
    /// higher projection, then the higher percent. The tint is that of `basis`, so a row with no
    /// trend ranks by its percent band and 95 percent outranks an amber row.
    public static func binding(_ rows: [UsageRowPressure]) -> UsageRowPressure? {
        func rank(_ row: UsageRowPressure) -> (Int, Int) {
            let tint = switch basis(row)?.severity {
            case .danger: 0
            case .warn: 1
            case .calm, nil: 2
            }
            let level = switch row.level {
            case .red: 0
            case .amber: 1
            case .green: 2
            }
            return (tint, level)
        }
        return rows.min { first, second in
            if rank(first) != rank(second) { return rank(first) < rank(second) }
            let projections = (first.projectedPct ?? 0, second.projectedPct ?? 0)
            if projections.0 != projections.1 { return projections.0 > projections.1 }
            return first.usedPct > second.usedPct
        }
    }

    public static func basis(_ binding: UsageRowPressure?) -> UsageBubbleBasis? {
        guard let binding else { return nil }
        return binding.burnPctPerHour == nil || binding.hoursToReset == nil || binding.usedPct >= UsageSeverity.dangerPct
            ? .severity(binding.usedPct) : .pressure(binding.level)
    }
}
