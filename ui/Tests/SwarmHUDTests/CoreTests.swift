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
    #expect(WindowKind(window: "spark:primary", minutes: 60) == nil)
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

@Test func worstWindowKeepsTheFirstOnATie() {
    let window = { (kind: WindowKind, pct: Int) in UsageWindow(kind: kind, usedPct: pct, resetAt: nil, asOf: nil, isFresh: true) }
    let firstTied = UsageAccount(provider: "claude", name: "first", windows: [window(.session, 40), window(.week, 81)], problem: nil)
    let secondTied = UsageAccount(provider: "codex", name: "second", windows: [window(.week, 81)], problem: nil)
    let signedOut = UsageAccount(provider: "claude", name: "signed-out", windows: [], problem: "Token expired")
    #expect(worstWindow([firstTied, secondTied, signedOut]).map { "\($0.account.name) \($0.window.kind.title)" } == "first Week")
    #expect(worstWindow([signedOut]) == nil)
}

@Test func twoLimitsOfOneLengthKeepTheFullerInOneColumn() {
    let meter = { (window: String, pct: Int) in
        UsageMeter(provider: "codex", account: "two-limits", label: "two-limits", window: window, windowMinutes: 10080,
                   usedPct: pct, resetTimeSeconds: nil, state: "fresh")
    }
    let list = accounts(from: [meter("codex:primary", 20), meter("spark:primary", 70), meter("other:primary", 40)])
    #expect(list[0].windows.map(\.kind) == [.week])
    #expect(list[0].windows[0].usedPct == 70)
}

@Test func shortDurationSurvivesAnOutOfRangeResetTime() {
    let now = Date(timeIntervalSince1970: 0)
    #expect(shortDuration(until: Date(timeIntervalSince1970: 1e300), now: now) == "99d")
    #expect(shortDuration(until: Date(timeIntervalSince1970: -1e300), now: now) == "now")
    #expect(shortDuration(until: Date(timeIntervalSince1970: 9_000), now: now) == "2h 30m")
    #expect(shortDuration(until: Date(timeIntervalSince1970: 59.5), now: now) == "now")
    #expect(shortDuration(until: Date(timeIntervalSince1970: 60), now: now) == "1m")
}
