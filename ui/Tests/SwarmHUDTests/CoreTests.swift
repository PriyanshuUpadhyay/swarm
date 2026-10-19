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

@Test func expiredClaudeTokenOffersRenewWithTheExactAccountArgument() throws {
    let json = #"{"schema":"swarm.usage.v1","meters":[{"provider":"claude","account":"work space; $(echo no)","label":"Work","state":"failed","reason":"New warning text","reason_code":"token_expired"}]}"#
    let list = accounts(from: try UsageSnapshot.decode(Data(json.utf8)).meters)
    #expect(list[0].problem == "New warning text")
    #expect(list[0].renewalArguments == ["run", "claude", "--account", "work space; $(echo no)", "--", "-p", "/usage"])
}

@Test func renewRequiresClaudeAndTheExpiredReasonCode() {
    for provider in ["claude", "codex"] {
        for code in [nil, "relogin_required", "refresh_failed", "no_credentials", "future_code", "token_expired"] {
            let meter = UsageMeter(provider: provider, account: "a", label: "a", window: nil, windowMinutes: nil,
                                   usedPct: nil, resetTimeSeconds: nil, state: "failed", reason: "Token expired", reasonCode: code)
            let row = accounts(from: [meter])[0]
            #expect((row.renewalArguments != nil) == (provider == "claude" && code == "token_expired"))
        }
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

private func window(_ kind: WindowKind, _ pct: Int, resetIn hours: Double? = nil, now: Date = Date(timeIntervalSince1970: 100_000),
                    fresh: Bool = true) -> UsageWindow {
    UsageWindow(kind: kind, usedPct: pct, resetAt: hours.map { now.addingTimeInterval($0 * 3600) }, asOf: now, isFresh: fresh)
}

private func account(_ provider: String, _ name: String, _ windows: [UsageWindow]) -> UsageAccount {
    UsageAccount(provider: provider, name: name, windows: windows, problem: windows.isEmpty ? "Token expired" : nil,
                 renewalArguments: nil)
}

@Test func historyDropsRepeatsAndOldSamples() {
    let now = Date(timeIntervalSince1970: 10 * 86400)
    let old = HistorySample(account: "a", window: "week", pct: 1, asOf: now.timeIntervalSince1970 - 8 * 86400)
    let one = HistorySample(account: "a", window: "week", pct: 5, asOf: now.timeIntervalSince1970 - 60)
    #expect(appendHistory([old, one], [one], now: now) == [one])
    let many = (0..<6000).map { HistorySample(account: "a", window: "session", pct: 1, asOf: now.timeIntervalSince1970 - Double($0)) }
    #expect(appendHistory(many, [], now: now).count == 5000)
}

@Test func sessionPaceFillsBeforeTheResetFromTheLastHour() {
    let now = Date(timeIntervalSince1970: 100_000)
    let owner = account("claude", "a", [])
    let history = [
        // Older than the hour: left out of the burn.
        HistorySample(account: "claude/a", window: "session", pct: 10, asOf: 100_000 - 3 * 3600),
        HistorySample(account: "claude/a", window: "session", pct: 40, asOf: 100_000 - 3600),
        HistorySample(account: "claude/a", window: "session", pct: 70, asOf: 100_000),
    ]
    #expect(pace(account: owner, window: window(.session, 70, resetIn: 3), history: history, now: now) == .fillsIn(3600))
    #expect(pace(account: owner, window: window(.session, 70, resetIn: 0.5), history: history, now: now) == .lasts)
    #expect(pace(account: owner, window: window(.session, 70, resetIn: 3), history: [], now: now) == nil)
}

@Test func aResetStartsANewSegment() {
    let samples = [90, 95, 10, 20].enumerated().map {
        HistorySample(account: "a", window: "session", pct: $1, asOf: Double($0) * 600)
    }
    #expect(currentSegment(samples).map(\.pct) == [10, 20])
    // Right after a reset there is one sample in the new segment, so no pace from the old one.
    let now = Date(timeIntervalSince1970: 1200)
    let afterReset = Array(samples.prefix(3)).map { HistorySample(account: "claude/a", window: $0.window, pct: $0.pct, asOf: $0.asOf) }
    #expect(pace(account: account("claude", "a", []), window: window(.session, 10, resetIn: 5, now: now), history: afterReset, now: now) == nil)
}

@Test func weekPaceWaitsForADayThenUsesTheWholeWindow() {
    let now = Date(timeIntervalSince1970: 100_000)
    let owner = account("claude", "a", [])
    // 20 h into the week: no pace yet.
    #expect(pace(account: owner, window: window(.week, 50, resetIn: 148), history: [], now: now) == nil)
    // 48 h in at 48%: 1%/h, full in 52 h, before the reset 120 h away.
    #expect(pace(account: owner, window: window(.fable, 48, resetIn: 120), history: [], now: now) == .fillsIn(52 * 3600))
    // 96 h in at 24%: 0.25%/h lasts to the reset.
    #expect(pace(account: owner, window: window(.week, 24, resetIn: 72), history: [], now: now) == .lasts)
    #expect(pace(account: owner, window: window(.week, 100, resetIn: 72), history: [], now: now) == .full)
}

@Test func paceColourTurnsRedWithinTheHourOrWhenCritical() {
    #expect(Pace.fillsIn(2 * 3600).severity(window: window(.week, 50)) == .warn)
    #expect(Pace.fillsIn(1800).severity(window: window(.week, 50)) == .critical)
    #expect(Pace.fillsIn(5 * 3600).severity(window: window(.week, 96)) == .critical)
    #expect(Pace.lasts.severity(window: window(.week, 96)) == .calm)
}

@Test func bestAccountIsTheLowestFullestWindowFirstOnATie() {
    let now = Date(timeIntervalSince1970: 100_000)
    let busy = account("claude", "busy", [window(.session, 10, resetIn: 2), window(.week, 60, resetIn: 50)])
    let tiedFirst = account("codex", "tied-first", [window(.week, 30, resetIn: 160)])
    let tiedSecond = account("claude", "tied-second", [window(.session, 30, resetIn: 2)])
    let signedOut = account("claude", "signed-out", [])
    let best = bestAccount([signedOut, busy, tiedFirst, tiedSecond], history: [], now: now)
    #expect(best == BestAccount(account: tiedFirst, outlook: .lasts))
    #expect(bestAccount([signedOut], history: [], now: now) == nil)
    // Its shortest fill time before a reset: the fable window (1%/h, 52 h) beats a week that lasts.
    let filling = account("claude", "filling", [window(.week, 24, resetIn: 72), window(.fable, 48, resetIn: 120)])
    #expect(bestAccount([filling], history: [], now: now)?.outlook == .left(52 * 3600))
}

@Test func allFullShowsTheEarliestTimeAnAccountIsUsableAgain() {
    let now = Date(timeIntervalSince1970: 100_000)
    // Usable again when its later full window resets: 30 h.
    let both = account("claude", "both", [window(.session, 100, resetIn: 2), window(.week, 100, resetIn: 30)])
    // One full window resetting in 5 h.
    let one = account("codex", "one", [window(.week, 100, resetIn: 5), window(.session, 40, resetIn: 1)])
    let best = bestAccount([both, one], history: [], now: now)
    #expect(best?.account.name == "one")
    #expect(best?.outlook == .allFull(resetAt: now.addingTimeInterval(5 * 3600)))
    let unknown = account("claude", "unknown", [window(.week, 100)])
    #expect(bestAccount([unknown], history: [], now: now)?.outlook == .allFull(resetAt: nil))
    // One account with room left ends the all-full state.
    let room = account("codex", "room", [window(.week, 99, resetIn: 5)])
    #expect(bestAccount([both, room], history: [], now: now)?.account.name == "room")
}

@Test func columnsGroupByProviderClaudeFirst() {
    let codexA = account("codex", "a", [window(.week, 1)])
    let claudeA = account("claude", "a", [window(.session, 1), window(.fable, 1)])
    let codexB = account("codex", "b", [window(.session, 1), window(.week, 1)])
    let claudeB = account("claude", "b", [window(.week, 1)])
    let signedOut = account("claude", "c", [])
    let groups = providerGroups([codexA, claudeA, codexB, signedOut, claudeB])
    #expect(groups.map(\.provider) == ["claude", "codex"])
    #expect(groups[0].kinds == [.session, .week, .fable])
    #expect(groups[0].accounts.map(\.name) == ["a", "c", "b"])
    #expect(groups[1].kinds == [.session, .week])
    #expect(groups[1].accounts.map(\.name) == ["a", "b"])
    #expect(providerGroups([account("agy", "x", []), codexA]).map(\.provider) == ["codex", "agy"])
}

@Test func workingReadsV1WithAndWithoutWaitingAndFailsQuiet() {
    let read = { (json: String) in WorkingSnapshot.read(Data(json.utf8)) }
    #expect(read(#"{"schema":"swarm.working.v1","working":{"claude":2,"codex":1}}"#)
            == WorkingSnapshot(working: ["claude": 2, "codex": 1]))
    #expect(read(#"{"schema":"swarm.working.v1","working":{"claude":2},"waiting":{"codex":1,"claude":0}}"#)
            == WorkingSnapshot(working: ["claude": 2], waiting: ["codex": 1]))
    #expect(read(#"{"schema":"swarm.working.v1","working":{},"waiting":{"codex":1}}"#).isActive)
    #expect(!read(#"{"schema":"swarm.working.v1","working":{}}"#).isActive)
    #expect(read(#"{"schema":"swarm.working.v2","working":{"claude":2}}"#) == WorkingSnapshot())
    #expect(read("swarm: unknown command") == WorkingSnapshot())
}

@Test func mascotFamilyGoesWorkingThenWaitingThenLastThenBestThenClaude() {
    let best = BestAccount(account: account("codex", "x", [window(.week, 1)]), outlook: .lasts)
    let both = WorkingSnapshot(working: ["claude": 1, "codex": 3], waiting: ["claude": 5])
    #expect(mascotFamily(both, lastWorking: "claude", best: nil) == "codex")
    #expect(mascotFamily(WorkingSnapshot(working: ["claude": 2, "codex": 2]), lastWorking: nil, best: nil) == "claude")
    #expect(mascotFamily(WorkingSnapshot(), lastWorking: "claude", best: best) == "claude")
    #expect(mascotFamily(WorkingSnapshot(), lastWorking: nil, best: best) == "codex")
    #expect(mascotFamily(WorkingSnapshot(), lastWorking: nil, best: nil) == "claude")
}

@Test func mascotSleepsFiveMinutesAfterTheLastActiveAgent() {
    let last = Date(timeIntervalSince1970: 100_000)
    let idle = WorkingSnapshot()
    #expect(mascotState(idle, lastActive: last, now: last.addingTimeInterval(4 * 60 + 59)) == .idle)
    #expect(mascotState(idle, lastActive: last, now: last.addingTimeInterval(5 * 60 + 1)) == .sleeping)
    let late = last.addingTimeInterval(3600)
    #expect(mascotState(WorkingSnapshot(waiting: ["codex": 1]), lastActive: last, now: late) == .waiting)
    #expect(mascotState(WorkingSnapshot(working: ["claude": 1], waiting: ["codex": 1]), lastActive: last, now: late) == .working)
}

@Test func mascotTurnMovesOnEveryTenMinutesAndWrapsAround() {
    let start = Date(timeIntervalSinceReferenceDate: 600 * 17 * 1000)
    #expect(mascotTurn(at: start, count: 17) == 0)
    #expect(mascotTurn(at: start.addingTimeInterval(599), count: 17) == 0)
    #expect(mascotTurn(at: start.addingTimeInterval(600), count: 17) == 1)
    #expect(mascotTurn(at: start.addingTimeInterval(600 * 17), count: 17) == 0)
    #expect(mascotTurn(at: start, count: 0) == 0)
}

@Test func ringShowsTheFullestLimitAllFullAndOldReadings() {
    let now = Date(timeIntervalSince1970: 100_000)
    let calm = account("claude", "pro2", [window(.session, 82, resetIn: 0.5), window(.week, 30, resetIn: 100)])
    #expect(usageRing(bestAccount([calm], history: [], now: now), readFailed: false)
            == .reading(usedPct: 82, severity: .warn, old: false))
    #expect(usageRing(bestAccount([calm], history: [], now: now), readFailed: true)
            == .reading(usedPct: 82, severity: .warn, old: true))
    let stale = account("codex", "x", [window(.week, 10, resetIn: 100, fresh: false)])
    #expect(usageRing(bestAccount([stale], history: [], now: now), readFailed: false)
            == .reading(usedPct: 10, severity: .calm, old: true))
    // 48% that fills in 52 h, before its reset, warns although the percent is calm.
    let filling = account("claude", "f", [window(.fable, 48, resetIn: 120)])
    #expect(usageRing(bestAccount([filling], history: [], now: now), readFailed: false)
            == .reading(usedPct: 48, severity: .warn, old: false))
    let full = account("claude", "full", [window(.week, 100, resetIn: 5)])
    #expect(usageRing(bestAccount([full], history: [], now: now), readFailed: false)
            == .reading(usedPct: 100, severity: .critical, old: false))
    #expect(usageRing(nil, readFailed: true) == .none)
}

@Test func codexMetersCarryResetCreditsToTheAccount() throws {
    let json = #"{"schema":"swarm.usage.v1","meters":[{"provider":"codex","account":"w","label":"w","window":"primary","window_minutes":300,"used_pct":40,"state":"fresh","reset_credits":{"available":2,"expiry_time_seconds":[1792703000,null]}},{"provider":"claude","account":"c","label":"c","window":"5h","used_pct":1,"state":"fresh","reset_credits":null}]}"#
    let list = accounts(from: try UsageSnapshot.decode(Data(json.utf8)).meters)
    #expect(list[0].resetCredits == ResetCredits(available: 2, expiryTimeSeconds: [1792703000, nil]))
    #expect(list[1].resetCredits == nil)
}

@Test func resetCreditsTextListsDatesWithNoExpiryLastAndAShortForm() {
    let utc = TimeZone(identifier: "UTC")!
    let oct4 = 1791115200.0, oct23 = 1792756800.0 // 2026-10-04 and 2026-10-23, 12:00 UTC
    let three = ResetCredits(available: 3, expiryTimeSeconds: [oct4, oct23, nil])
    #expect(three.text(timeZone: utc) == "3 resets · expire Oct 4, Oct 23, no expiry")
    #expect(three.short(timeZone: utc) == "3 resets · next Oct 4")
    #expect(ResetCredits(available: 1, expiryTimeSeconds: [oct4]).text(timeZone: utc) == "1 reset · expires Oct 4")
    #expect(ResetCredits(available: 3, expiryTimeSeconds: [oct4]).text(timeZone: utc) == "3 resets · expire Oct 4, …")
    #expect(ResetCredits(available: 2, expiryTimeSeconds: nil).text(timeZone: utc) == "2 resets")
    #expect(ResetCredits(available: 2, expiryTimeSeconds: [nil, nil]).short(timeZone: utc) == "2 resets")
    #expect(ResetCredits(available: 0, expiryTimeSeconds: [oct4]).text(timeZone: utc) == nil)
}

@Test func agyAndPiAgentsGetTheirOwnFamily() {
    #expect(mascotFamily(WorkingSnapshot(working: ["agy": 4], waiting: ["codex": 1]), lastWorking: "claude", best: nil) == "agy")
    #expect(mascotFamily(WorkingSnapshot(working: ["pi": 3, "claude": 1]), lastWorking: nil, best: nil) == "pi")
    #expect(mascotFamily(WorkingSnapshot(working: ["other": 9]), lastWorking: nil, best: nil) == "claude")
}

@Test func codexPetManifestTakesDefaultsAndCustomAnimations() throws {
    let plain = try CodexPetManifest.parse(Data(#"{"id":"chefito","displayName":"Chefito"}"#.utf8), folder: "chef")
    #expect(plain.name == "Chefito" && plain.sheetPath == "spritesheet.webp")
    #expect(plain.frame == CodexPetManifest.Frame(width: 192, height: 208, columns: 8, rows: 9))
    #expect(plain.animations[.working] == .init(frames: Array(56...61), fps: 8))
    #expect(plain.animations[.waiting] == .init(frames: Array(48...53), fps: 6.67))
    #expect(plain.animations[.idle]?.frames == Array(0...5))
    #expect(plain.animations[.sleeping] == .init(frames: [0], fps: 1))
    let custom = try CodexPetManifest.parse(Data(#"""
        {"spritesheetPath":"art/sheet.png","frame":{"width":32,"height":32,"columns":4,"rows":2},
         "animations":{"running":{"frames":[4,5,6],"fps":30},"idle":{"frames":[0,1]}}}
        """#.utf8), folder: "blob")
    #expect(custom.name == "blob" && custom.sheetPath == "art/sheet.png")
    #expect(custom.animations[.working] == .init(frames: [4, 5, 6], fps: 8))
    #expect(custom.animations[.idle] == .init(frames: [0, 1], fps: 8))
    // A default animation that falls outside a small sheet uses the idle frames instead.
    #expect(custom.animations[.waiting] == .init(frames: [0, 1], fps: 8))
}

@Test func codexPetManifestRefusesAPathOutsideItsFolderAndBadFrames() {
    for json in [#"{"spritesheetPath":"../x.webp"}"#, #"{"spritesheetPath":"/etc/x.webp"}"#,
                 #"{"frame":{"width":0,"height":208,"columns":8,"rows":9}}"#,
                 #"{"animations":{"idle":{"frames":[72]}}}"#, #"{"animations":{"idle":{"frames":[]}}}"#] {
        #expect(throws: (any Error).self) { try CodexPetManifest.parse(Data(json.utf8), folder: "p") }
    }
}

@Test func aStrainedCodexBestAccountWithAResetOffersIt() {
    var codex = account("codex", "w", [window(.session, 90)])
    codex.resetCredits = ResetCredits(available: 2, expiryTimeSeconds: nil)
    #expect(BestAccount(account: codex, outlook: .lasts).strained)
    #expect(BestAccount(account: codex, outlook: .lasts).offersReset)
    var calm = account("codex", "w", [window(.session, 10)])
    calm.resetCredits = codex.resetCredits
    #expect(!BestAccount(account: calm, outlook: .lasts).strained)
    #expect(BestAccount(account: calm, outlook: .left(1800)).offersReset)
    #expect(!BestAccount(account: calm, outlook: .lasts).offersReset)
    #expect(!BestAccount(account: account("codex", "w", [window(.session, 90)]), outlook: .lasts).offersReset)
    #expect(!BestAccount(account: account("claude", "c", [window(.session, 99)]), outlook: .lasts).offersReset)
}
