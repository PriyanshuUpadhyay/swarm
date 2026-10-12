import Foundation
import Testing
@testable import SwarmCore

private let now = 1_000_000

private func sample(_ usedPct: Double, minutesAgo: Int, window: String = "5h") -> UsageHistorySample {
    UsageHistorySample(provider: "claude", account: "work", window: window, usedPct: usedPct,
                       asOfSeconds: now - minutesAgo * 60)
}

private func meter(
    window: String = "5h", minutes: Int? = 300, usedPct: Double? = 60, hoursToReset: Double? = 2,
    state: SwarmUsageState = .fresh
) -> SwarmUsageMeter {
    SwarmUsageMeter(provider: "claude", account: "work", label: "work", window: window, windowMinutes: minutes,
                    usedPct: usedPct, resetTimeSeconds: hoursToReset.map { now + Int($0 * 3_600) },
                    state: state, source: "claude_api", asOfSeconds: now)
}

private func pressure(
    _ meterID: String, usedPct: Double, burn: Double?, projected: Double? = nil, level: UsagePressureLevel
) -> UsageRowPressure {
    UsageRowPressure(meterID: meterID, usedPct: usedPct, burnPctPerHour: burn, hoursToReset: 2,
                     projectedPct: projected, hoursToFull: nil, level: level)
}

private func close(_ value: Double?, _ expected: Double) -> Bool {
    value.map { abs($0 - expected) < 1e-6 } ?? false
}

@Suite("Usage pressure")
struct UsagePressureTests {
    @Test("a drop of more than 10 points starts a new segment")
    func segments() {
        let reset = [sample(80, minutesAgo: 90), sample(85, minutesAgo: 60), sample(10, minutesAgo: 30), sample(20, minutesAgo: 0)]
        #expect(UsagePressure.currentSegment(reset).map(\.usedPct) == [10, 20])
        let steady = [sample(10, minutesAgo: 60), sample(20, minutesAgo: 30), sample(30, minutesAgo: 0)]
        #expect(UsagePressure.currentSegment(steady).count == 3)
    }

    @Test("burn rate is percent per hour and needs two samples, ten minutes and two points")
    func burnRate() {
        #expect(close(UsagePressure.burnRate([sample(10, minutesAgo: 60), sample(40, minutesAgo: 0)], lookbackHours: 1), 30))
        #expect(UsagePressure.burnRate([sample(40, minutesAgo: 0)], lookbackHours: 1) == nil)
        #expect(UsagePressure.burnRate([sample(10, minutesAgo: 5), sample(40, minutesAgo: 0)], lookbackHours: 1) == nil)
        // One point between two samples is rounding noise. Two points is a trend again.
        #expect(UsagePressure.burnRate([sample(3, minutesAgo: 43), sample(4, minutesAgo: 0)], lookbackHours: 1) == nil)
        #expect(UsagePressure.burnRate([sample(3, minutesAgo: 43), sample(5, minutesAgo: 0)], lookbackHours: 1) != nil)
    }

    @Test("weekly pace is the whole-window average and has no value in the first day")
    func weeklyPace() {
        #expect(close(UsagePressure.weeklyAveragePace(usedPct: 5, hoursToReset: 94), 5.0 / 74.0))
        #expect(UsagePressure.weeklyAveragePace(usedPct: 3, hoursToReset: 164) == nil)
    }

    @Test("red when the limit comes before the reset, amber when the projection is 85 or more")
    func classify() {
        let red = UsagePressure.classify(usedPct: 50, burn: 20, hoursToReset: 5)
        #expect(close(red.hoursToFull, 2.5) && red.level == .red)
        let amber = UsagePressure.classify(usedPct: 80, burn: 5, hoursToReset: 2)
        #expect(close(amber.projectedPct, 90) && amber.level == .amber)
        #expect(UsagePressure.classify(usedPct: 30, burn: 2, hoursToReset: 3).level == .green)
        let full = UsagePressure.classify(usedPct: 100, burn: 10, hoursToReset: 1)
        #expect(close(full.projectedPct, 110) && close(full.hoursToFull, 0) && full.level == .red)
    }

    @Test("no burn, a burn that is not positive and a missing reset keep their own results")
    func classifyBoundaries() {
        let noBurn = UsagePressure.classify(usedPct: 95, burn: nil, hoursToReset: 1)
        #expect(noBurn.projectedPct == nil && noBurn.hoursToFull == nil && noBurn.level == .green)
        let falling = UsagePressure.classify(usedPct: 95, burn: -3, hoursToReset: 1)
        #expect(close(falling.projectedPct, 92) && falling.hoursToFull == nil && falling.level == .amber)
        let flat = UsagePressure.classify(usedPct: 95, burn: 0, hoursToReset: 1)
        #expect(close(flat.projectedPct, 95) && flat.hoursToFull == nil && flat.level == .amber)
        let noReset = UsagePressure.classify(usedPct: 50, burn: 10, hoursToReset: nil)
        #expect(noReset.projectedPct == nil && close(noReset.hoursToFull, 5) && noReset.level == .green)
    }

    @Test("the binding row has the worst pressure, then the higher projection, then the higher percent")
    func binding() {
        let green = pressure("a", usedPct: 40, burn: 1, projected: 43, level: .green)
        let red = pressure("b", usedPct: 60, burn: 20, projected: 160, level: .red)
        #expect(UsagePressure.binding([green, red])?.meterID == "b")
        let lower = pressure("a", usedPct: 50, burn: 10, projected: 90, level: .amber)
        let higher = pressure("b", usedPct: 55, burn: 10, projected: 90, level: .amber)
        #expect(UsagePressure.binding([lower, higher])?.meterID == "b")
        #expect(UsagePressure.binding([]) == nil)
    }

    @Test("a five-hour window takes its burn from the last hour of history")
    func shortWindow() {
        let rows = UsagePressure.rows(
            meters: [meter()], history: [sample(30, minutesAgo: 60), sample(60, minutesAgo: 0)], nowSeconds: now
        )
        #expect(rows.count == 1)
        #expect(rows[0].meterID == "claude:work:5h")
        #expect(close(rows[0].burnPctPerHour, 30))
        #expect(close(rows[0].hoursToReset, 2))
        // 40 points left at 30 per hour is 1.33 h, before the reset in 2 h.
        #expect(rows[0].level == .red)
    }

    @Test("a meter that is not fresh gives no pressure row")
    func gatedMeters() {
        let history = [sample(30, minutesAgo: 60), sample(60, minutesAgo: 0)]
        for state in [SwarmUsageState.stale, .missing, .failed, .unavailable, .noSource] {
            #expect(UsagePressure.rows(meters: [meter(state: state)], history: history, nowSeconds: now).isEmpty)
        }
        #expect(UsagePressure.rows(meters: [meter(usedPct: nil)], history: history, nowSeconds: now).isEmpty)
    }

    @Test("weekly windows use the whole-window pace and ignore short history")
    func weeklyWindows() {
        let fable = UsagePressure.rows(
            meters: [meter(window: "fb", minutes: 10_080, usedPct: 60, hoursToReset: 72)], history: [], nowSeconds: now
        )
        #expect(close(fable[0].burnPctPerHour, 60.0 / 96.0) && fable[0].level == .red)
        // Two points in the last hour must not project across four days.
        let low = UsagePressure.rows(
            meters: [meter(window: "7d", minutes: 10_080, usedPct: 5, hoursToReset: 94)],
            history: [sample(3, minutesAgo: 60, window: "7d"), sample(5, minutesAgo: 0, window: "7d")], nowSeconds: now
        )
        #expect(close(low[0].burnPctPerHour, 5.0 / 74.0) && low[0].level == .green)
        let high = UsagePressure.rows(
            meters: [meter(window: "7d", minutes: 10_080, usedPct: 60, hoursToReset: 94)], history: [], nowSeconds: now
        )
        #expect(close(high[0].hoursToFull, 40.0 / (60.0 / 74.0)) && high[0].level == .red)
        let early = UsagePressure.rows(
            meters: [meter(window: "7d", minutes: 10_080, usedPct: 3, hoursToReset: 164)], history: [], nowSeconds: now
        )
        #expect(early[0].burnPctPerHour == nil && early[0].level == .green)
    }

    @Test("an unknown window length gives no trend, and a passed reset is zero hours")
    func otherWindows() {
        let history = [sample(30, minutesAgo: 60, window: "1h"), sample(60, minutesAgo: 0, window: "1h")]
        for minutes in [60, nil] as [Int?] {
            let rows = UsagePressure.rows(meters: [meter(window: "1h", minutes: minutes)], history: history, nowSeconds: now)
            #expect(rows.count == 1 && rows[0].burnPctPerHour == nil && rows[0].level == .green)
        }
        let passed = UsagePressure.rows(meters: [meter(hoursToReset: -1)], history: [], nowSeconds: now)
        #expect(passed[0].hoursToReset == 0)
        let unknown = UsagePressure.rows(meters: [meter(hoursToReset: nil)], history: [], nowSeconds: now)
        #expect(unknown[0].hoursToReset == nil)
    }

    @Test("with no trend the tint comes from the percent band, else from the pressure")
    func basis() {
        // 95% with no history is green by trend, but the tint must come from the 95% band.
        #expect(UsagePressure.basis(pressure("a", usedPct: 95, burn: nil, level: .green)) == .severity(95))
        #expect(UsagePressure.basis(pressure("a", usedPct: 60, burn: 20, level: .red)) == .pressure(.red))
        #expect(UsagePressure.basis(nil) == nil)
        #expect(UsageBubbleBasis.severity(95).severity == .danger)
        #expect(UsageBubbleBasis.severity(75).severity == .warn)
        #expect(UsageBubbleBasis.severity(69).severity == .calm)
        #expect(UsageBubbleBasis.pressure(.amber).severity == .warn)
        #expect(UsageSeverity(usedPct: 10, level: .red) == .danger)
    }
}
