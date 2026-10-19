import Foundation
import Testing
@testable import SwarmHUD
@testable import SwarmHUDCore

@Test func renewalUsesHomeNullStreamsAndSeparateArguments() throws {
    let directory = FileManager.default.temporaryDirectory.appending(path: UUID().uuidString)
    try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
    defer { try? FileManager.default.removeItem(at: directory) }
    let script = directory.appending(path: "swarm")
    // The log path is an argument so even spaces and shell syntax stay literal.
    try Data(#"""
    #!/bin/sh
    log="$1"
    shift
    printf '%s\n' "$PWD" "$@" > "$log"
    if read line; then printf 'unexpected stdin\n' >> "$log"; fi
    for stream in 0 1 2; do /usr/sbin/lsof -a -p $$ -d "$stream" -Fn >> "$log"; done
    """#.utf8).write(to: script)
    try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: script.path)
    let log = directory.appending(path: "log")
    runRenewal(script.path, [log.path, "run", "claude", "--account", "a space; $(echo no)", "--", "-p", "/usage"])
    let lines = try String(contentsOf: log, encoding: .utf8).split(separator: "\n").map(String.init)
    #expect(Array(lines.prefix(8)) == [FileManager.default.homeDirectoryForCurrentUser.path,
                                     "run", "claude", "--account", "a space; $(echo no)", "--", "-p", "/usage"])
    #expect(!lines.contains("unexpected stdin"))
    #expect(lines.filter { $0 == "n/dev/null" }.count == 3)
}

@Test func renewalStopsAtItsDeadlineAndHandlesMissingExecutables() throws {
    let directory = FileManager.default.temporaryDirectory.appending(path: UUID().uuidString)
    try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
    defer { try? FileManager.default.removeItem(at: directory) }
    let script = directory.appending(path: "swarm")
    try Data("#!/bin/sh\nexec /bin/sleep 10\n".utf8).write(to: script)
    try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: script.path)
    let start = Date()
    runRenewal(script.path, [], timeout: 0.05)
    #expect(Date().timeIntervalSince(start) < 2)
    runRenewal(nil, [])
    runRenewal(directory.appending(path: "missing").path, [])
}

@Test(arguments: [true, false]) @MainActor
func renewalRunsOncePerAccountAndRefreshesDuringCooldown(renewed: Bool) async throws {
    let directory = FileManager.default.temporaryDirectory.appending(path: UUID().uuidString)
    try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
    defer { try? FileManager.default.removeItem(at: directory) }
    let script = directory.appending(path: "swarm")
    let log = directory.appending(path: "log")
    let release = directory.appending(path: "release")
    let expired = #"{"schema":"swarm.usage.v1","meters":[{"provider":"claude","account":"a","label":"a","state":"failed","reason":"Token expired","reason_code":"token_expired"},{"provider":"claude","account":"b","label":"b","state":"failed","reason":"Token expired","reason_code":"token_expired"}]}"#
    let reply = renewed ? #"{"schema":"swarm.usage.v1","meters":[]}"# : expired
    // UUID paths have no shell syntax. Each renewal waits until both accounts are running.
    try Data("""
    #!/bin/sh
    case "$1" in
      run)
        printf 'renew:%s\\n' "$4" >> '\(log.path)'
        while ! test -f '\(release.path)'; do /bin/sleep 0.01; done
        exit \(renewed ? 0 : 1)
        ;;
      usage)
        if test "$2" = '--refresh'; then printf 'refresh\\n' >> '\(log.path)'; fi
        printf '%s' '\(reply)'
        ;;
    esac
    """.utf8).write(to: script)
    try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: script.path)
    let model = UsageModel(environment: ["SWARM_BIN": script.path],
                           historyURL: script.deletingLastPathComponent().appending(path: "history.json"))
    model.history = []
    model.accounts = accounts(from: try UsageSnapshot.decode(Data(expired.utf8)).meters)
    model.nextRefresh["claude"] = Date().addingTimeInterval(300)
    model.refreshError = "Previous error"
    model.renew(model.accounts[0])
    model.renew(model.accounts[0])
    model.renew(model.accounts[1])
    #expect(model.renewing == ["claude/a", "claude/b"])
    for _ in 0..<200 {
        if (try? String(contentsOf: log, encoding: .utf8).split(separator: "\n").count) == 2 { break }
        try await Task.sleep(for: .milliseconds(10))
    }
    try Data().write(to: release)
    for _ in 0..<200 {
        if model.renewing.isEmpty && model.refreshing.isEmpty && model.readAt != nil { break }
        try await Task.sleep(for: .milliseconds(10))
    }
    let lines = try String(contentsOf: log, encoding: .utf8).split(separator: "\n")
    #expect(lines.filter { $0 == "renew:a" }.count == 1)
    #expect(lines.filter { $0 == "renew:b" }.count == 1)
    #expect(lines.filter { $0 == "refresh" }.count == 2)
    #expect(model.renewing.isEmpty && model.refreshing.isEmpty && model.readAt != nil)
    if renewed {
        #expect(model.accounts.isEmpty)
    } else {
        #expect(model.accounts.count == 2)
        #expect(model.accounts.allSatisfy { $0.problem == "Token expired" && $0.renewalArguments != nil })
    }
    #expect(model.refreshError == nil)
}

@Test func codexPetsLoadFromTheirFoldersAndABadOneIsLeftOut() throws {
    let home = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    defer { try? FileManager.default.removeItem(at: home) }
    let blob = home.appendingPathComponent("pets/blob"), bad = home.appendingPathComponent("pets/bad")
    try FileManager.default.createDirectory(at: blob, withIntermediateDirectories: true)
    try FileManager.default.createDirectory(at: bad, withIntermediateDirectories: true)
    // A 2x1 grid of 1x1 frames.
    let png = "iVBORw0KGgoAAAANSUhEUgAAAAIAAAABCAYAAAD0In+KAAAADklEQVR4nGP4z8AAQv8BD/kD/YURmXYAAAAASUVORK5CYII="
    try Data(base64Encoded: png)!.write(to: blob.appendingPathComponent("s.png"))
    try Data(#"{"displayName":"Blob","spritesheetPath":"s.png","frame":{"width":1,"height":1,"columns":2,"rows":1}}"#.utf8)
        .write(to: blob.appendingPathComponent("pet.json"))
    try Data(#"{"spritesheetPath":"../blob/s.png","frame":{"width":1,"height":1,"columns":2,"rows":1}}"#.utf8)
        .write(to: bad.appendingPathComponent("pet.json"))
    let pets = Guest.codexPets(home: home)
    #expect(pets.map(\.name) == ["Blob"])
    #expect(pets.first?.smooth == true && pets.first?.credit == "Your Codex pet")
    #expect(pets.first?.frame(.working, tick: 1) != nil)
}

@Test func aStrainedPetSweatsWhileItWorksOrIdlesButNotWhileItWaitsOrSleeps() {
    #expect(mascotCue(.working, seconds: nil, ownZ: false, strained: true)?.left == true)
    #expect(mascotCue(.idle, seconds: 1, ownZ: false, strained: true)?.glyph == MascotArt.drop)
    #expect(mascotCue(.working, seconds: nil, ownZ: false, strained: false) == nil)
    #expect(mascotCue(.waiting, seconds: nil, ownZ: false, strained: true)?.glyph == MascotArt.bang)
    #expect(mascotCue(.sleeping, seconds: nil, ownZ: false, strained: true)?.glyph == MascotArt.zee)
}

@Test func theCastIsTheFamilyThenTheBeeThenTheGuests() {
    #expect(MascotCast.name(0) == "Swarm" && MascotCast.guest(0) == nil)
    #expect(MascotCast.name(MascotCast.beeTurn) == "Swarm bee" && MascotCast.family("pi", turn: MascotCast.beeTurn) == "bee")
    #expect(MascotCast.family("pi", turn: 0) == "pi")
    #expect(MascotCast.guest(MascotCast.ownTurns)?.name == SheetMascot.all[0].name)
    #expect(MascotCast.count == MascotCast.guests.count + MascotCast.ownTurns)
}

@Test func theCastTakesAtMostSixteenCodexPetsInFolderOrder() throws {
    let home = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    defer { try? FileManager.default.removeItem(at: home) }
    let png = Data(base64Encoded: "iVBORw0KGgoAAAANSUhEUgAAAAIAAAABCAYAAAD0In+KAAAADklEQVR4nGP4z8AAQv8BD/kD/YURmXYAAAAASUVORK5CYII=")!
    for n in 0..<(Guest.maxCodexPets + 2) {
        let folder = home.appendingPathComponent(String(format: "pets/p%02d", n))
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        try png.write(to: folder.appendingPathComponent("s.png"))
        try Data(#"{"spritesheetPath":"s.png","frame":{"width":1,"height":1,"columns":2,"rows":1}}"#.utf8)
            .write(to: folder.appendingPathComponent("pet.json"))
    }
    let pets = Guest.codexPets(home: home)
    #expect(pets.count == Guest.maxCodexPets)
    #expect(pets.first?.name == "p00" && pets.last?.name == "p15")
}
