// Core tests for mascot state, turns, usage rings and pet manifests.
import Foundation
import Testing
@testable import SwarmHUDCore

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
                 #"{"spritesheetPath":"~/x.webp"}"#, #"{"spritesheetPath":"a/../../x.webp"}"#,
                 #"{"frame":{"width":0,"height":208,"columns":8,"rows":9}}"#,
                 #"{"animations":{"idle":{"frames":[72]}}}"#, #"{"animations":{"idle":{"frames":[]}}}"#] {
        #expect(throws: (any Error).self) { try CodexPetManifest.parse(Data(json.utf8), folder: "p") }
    }
}

