import Foundation
import Testing
@testable import SwarmHUDCore

private let fixture = """
{"schema":"swarm.usage.v1","future_field":1,"meters":[
 {"provider":"codex","account":"default","label":"default","window":"codex:primary","window_minutes":10080,
  "used_pct":27,"reset_time_seconds":1792473726,"state":"stale","source":"codex_app_server",
  "reason":"Usage refresh failed; old reading","reason_code":"refresh_failed","as_of_seconds":1792008335},
 {"provider":"claude","account":"work","label":"work","window":"fb","window_minutes":10080,"used_pct":96,
  "reset_time_seconds":1792382399,"state":"fresh","source":"claude_api","reason":null,"reason_code":null,"as_of_seconds":1792050000},
 {"provider":"claude","account":"work","label":"work","window":"5h","window_minutes":300,"used_pct":13,
  "reset_time_seconds":1792060199,"state":"fresh","source":"claude_api","reason":null,"reason_code":null,"as_of_seconds":1792050000},
 {"provider":"claude","account":"spare","label":"spare","window":null,"window_minutes":null,"used_pct":null,
  "reset_time_seconds":null,"state":"failed","source":"claude_api","reason":"Sign in again","reason_code":"relogin_required","as_of_seconds":null},
 {"provider":"agy","account":null,"label":"agy","window":null,"window_minutes":null,"used_pct":null,
  "reset_time_seconds":null,"state":"no_source","source":null,"reason":"No usage source","reason_code":"no_source","as_of_seconds":null}
]}
"""

@Test func decodesV1AndGroupsAccountsInColumnOrder() throws {
    let snapshot = try UsageSnapshot.decode(Data(fixture.utf8))
    let list = accounts(from: snapshot.meters)
    #expect(list.map(\.id) == ["codex/default", "claude/work", "claude/spare"])
    #expect(list[0].windows.map(\.kind) == [.week])
    #expect(list[0].windows[0].isFresh == false)
    #expect(list[1].windows.map(\.kind) == [.session, .fable])
    #expect(list[1].worst?.usedPct == 96)
    #expect(list[2].windows.isEmpty && list[2].problem == "Sign in again")
}

@Test func refusesAnotherSchema() {
    #expect(throws: (any Error).self) {
        try UsageSnapshot.decode(Data(#"{"schema":"swarm.usage.v2","meters":[]}"#.utf8))
    }
}

@Test func codexSlotsTakeTheirKindFromTheWindowLength() {
    #expect(WindowKind(window: "codex:primary", minutes: 300) == .session)
    #expect(WindowKind(window: "codex:secondary", minutes: 10080) == .week)
    #expect(WindowKind(window: "spark:primary", minutes: 60) == .other("spark:primary"))
}

@Test func severityBandsAreFixed() {
    #expect(Severity(pct: 79) == .calm)
    #expect(Severity(pct: 80) == .warn)
    #expect(Severity(pct: 94) == .warn)
    #expect(Severity(pct: 95) == .critical)
}

@Test func shortDurationKeepsTwoUnits() {
    let now = Date(timeIntervalSince1970: 0)
    #expect(shortDuration(until: Date(timeIntervalSince1970: 9840), now: now) == "2h 44m")
    #expect(shortDuration(until: Date(timeIntervalSince1970: 3 * 86400 + 20 * 3600 + 59), now: now) == "3d 20h")
    #expect(shortDuration(until: Date(timeIntervalSince1970: 720), now: now) == "12m")
    #expect(shortDuration(until: Date(timeIntervalSince1970: 30), now: now) == "now")
}

@Test func dayPhaseHours() {
    #expect([4, 5, 8, 9, 16, 17, 20, 21, 0].map { DayPhase.at(hour: $0) }
            == [.night, .dawn, .dawn, .day, .day, .dusk, .dusk, .night, .night])
}

@Test func nextPhaseChangeWrapsToTomorrowDawn() {
    var calendar = Calendar(identifier: .gregorian)
    calendar.timeZone = TimeZone(identifier: "UTC")!
    let at = { (h: Int, m: Int) in calendar.date(from: DateComponents(year: 2026, month: 10, day: 15, hour: h, minute: m))! }
    #expect(DayPhase.nextChange(after: at(14, 52), calendar: calendar) == at(17, 0))
    #expect(DayPhase.nextChange(after: at(22, 10), calendar: calendar)
            == calendar.date(from: DateComponents(year: 2026, month: 10, day: 16, hour: 5))!)
}

@Test func historyDropsRepeatsAndOldSamples() {
    let now = Date(timeIntervalSince1970: 10 * 86400)
    let old = HistorySample(account: "a", window: "week", pct: 1, asOf: now.timeIntervalSince1970 - 8 * 86400)
    let one = HistorySample(account: "a", window: "week", pct: 5, asOf: now.timeIntervalSince1970 - 60)
    #expect(appendHistory([old, one], [one], now: now) == [one])
}

@Test func sessionBurnFlagsAFullWindowBeforeItsReset() {
    let account = UsageAccount(provider: "claude", name: "a", windows: [], problem: nil)
    let now = Date(timeIntervalSince1970: 100_000)
    let history = [
        HistorySample(account: "claude/a", window: "session", pct: 40, asOf: 100_000 - 3600),
        HistorySample(account: "claude/a", window: "session", pct: 70, asOf: 100_000),
    ]
    let window = UsageWindow(kind: .session, usedPct: 70, resetAt: now.addingTimeInterval(3 * 3600), asOf: now, isFresh: true)
    let result = trend(account: account, window: window, history: history, now: now)
    #expect(result.burn == 30)
    #expect(result.hoursToFull == 1)
    #expect(result.pressure == .hot)
}

@Test func aResetStartsANewSegment() {
    let samples = [90, 95, 10, 20].enumerated().map {
        HistorySample(account: "a", window: "session", pct: $1, asOf: Double($0) * 600)
    }
    #expect(currentSegment(samples).map(\.pct) == [10, 20])
}

@Test func weeklyPaceWaitsForADay() {
    #expect(weeklyPace(pct: 50, hoursToReset: 150) == nil)
    #expect(weeklyPace(pct: 48, hoursToReset: 120) == 1)
}

@Test func worstWindowKeepsTheFirstOnATie() {
    let window = { (kind: WindowKind, pct: Int) in UsageWindow(kind: kind, usedPct: pct, resetAt: nil, asOf: nil, isFresh: true) }
    let a = UsageAccount(provider: "claude", name: "a", windows: [window(.session, 40), window(.week, 81)], problem: nil)
    let b = UsageAccount(provider: "codex", name: "b", windows: [window(.week, 81)], problem: nil)
    let c = UsageAccount(provider: "claude", name: "c", windows: [], problem: "Token expired")
    #expect(worstWindow([a, b, c]).map { "\($0.account.name) \($0.window.kind.title)" } == "a Week")
    #expect(worstWindow([c]) == nil)
}
