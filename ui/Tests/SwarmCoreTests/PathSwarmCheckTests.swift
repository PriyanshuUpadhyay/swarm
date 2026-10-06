import Foundation
import Testing
@testable import SwarmCore

@Suite("PATH swarm check")
struct PathSwarmCheckTests {
    let folder: URL
    let helper: String

    init() throws {
        folder = FileManager.default.temporaryDirectory
            .appendingPathComponent("path-swarm-check-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        helper = try Self.standIn(in: folder, named: "helper-swarm", script: "echo 'swarm 0.9.0 abc1234 '")
    }

    static func standIn(in folder: URL, named name: String, script: String) throws -> String {
        let path = folder.appendingPathComponent(name).path
        try "#!/bin/sh\n\(script)\n".write(toFile: path, atomically: true, encoding: .utf8)
        try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: path)
        return path
    }

    func check(_ pathSwarm: String?, branch: String = "", dismissed: [String] = []) async -> PathSwarmDrift? {
        await PathSwarmCheck.drift(branch: branch, pathSwarm: pathSwarm, helper: helper, dismissed: dismissed)
    }

    @Test("An older swarm on PATH is drift with both version lines")
    func olderSwarmIsDrift() async throws {
        let older = try Self.standIn(in: folder, named: "old-swarm", script: "echo 'swarm 0.7.3 def5678 '")
        let drift = try #require(await check(older))
        #expect(drift.path == older)
        #expect(drift.pathLine == "swarm 0.7.3 def5678")
        #expect(drift.helperLine == "swarm 0.9.0 abc1234")
        #expect(drift.key == "\(older)|swarm 0.7.3 def5678|swarm 0.9.0 abc1234")
    }

    @Test("The cask link to the helper itself is no drift")
    func linkToHelperIsNoDrift() async throws {
        // Prints the name it was run by, so only the same-file rule, not equal lines, passes this.
        let selfNaming = try Self.standIn(in: folder, named: "self-naming-helper", script: "echo \"swarm 0.9.0 $0\"")
        let link = folder.appendingPathComponent("linked-swarm").path
        try FileManager.default.createSymbolicLink(atPath: link, withDestinationPath: selfNaming)
        let drift = await PathSwarmCheck.drift(branch: "", pathSwarm: link, helper: selfNaming, dismissed: [])
        #expect(drift == nil)
    }

    @Test("Another file with the same version line is no drift")
    func sameLineIsNoDrift() async throws {
        let copy = try Self.standIn(in: folder, named: "cargo-swarm", script: "echo 'swarm 0.9.0 abc1234 '")
        #expect(await check(copy) == nil)
    }

    @Test("No swarm on PATH is no drift")
    func absentIsNoDrift() async {
        #expect(await check(nil) == nil)
    }

    @Test("A dismissed pair is no drift, and a new release asks again")
    func dismissedPairIsNoDrift() async throws {
        let older = try Self.standIn(in: folder, named: "old-swarm", script: "echo 'swarm 0.7.3 def5678 '")
        let key = try #require(await check(older)).key
        #expect(await check(older, dismissed: [key]) == nil)
        let newer = try Self.standIn(in: folder, named: "old-swarm", script: "echo 'swarm 0.8.0 0123abc '")
        #expect(await check(newer, dismissed: [key]) != nil)
    }

    @Test("A branch build never checks")
    func branchBuildIsSilent() async throws {
        let older = try Self.standIn(in: folder, named: "old-swarm", script: "echo 'swarm 0.7.3 def5678 '")
        #expect(await check(older, branch: "main") == nil)
        #expect(await check(older, branch: "unify-swarm") == nil)
    }

    @Test("An unreadable PATH version is no drift", arguments: ["exit 1", "true", "sleep 5"])
    func unreadableIsNoDrift(script: String) async throws {
        let broken = try Self.standIn(in: folder, named: "broken-swarm", script: script)
        #expect(await check(broken) == nil)
    }

    @Test("Only the login shell's PATH names the swarm, so a failed probe or a PATH without one is silent")
    func loginPathOnly() throws {
        let bin = folder.appendingPathComponent("bin")
        try FileManager.default.createDirectory(at: bin, withIntermediateDirectories: true)
        let swarm = try Self.standIn(in: bin, named: "swarm", script: "echo 'swarm 0.7.3 def5678 '")
        #expect(PathSwarmCheck.pathSwarm(loginPath: ["/nonexistent", bin.path]) == swarm)
        #expect(PathSwarmCheck.pathSwarm(loginPath: []) == nil)
        #expect(PathSwarmCheck.pathSwarm(loginPath: [folder.path]) == nil)
    }

    @Test("A missing helper is no drift")
    func missingHelperIsNoDrift() async throws {
        let older = try Self.standIn(in: folder, named: "old-swarm", script: "echo 'swarm 0.7.3 def5678 '")
        let drift = await PathSwarmCheck.drift(
            branch: "", pathSwarm: older, helper: folder.appendingPathComponent("absent").path, dismissed: []
        )
        #expect(drift == nil)
    }

    @Test("A Homebrew keg gets the brew command, any other file gets a remove step")
    func fixCommand() {
        let keg = PathSwarmDrift(
            path: "/opt/homebrew/bin/swarm", resolved: "/opt/homebrew/Cellar/swarm/0.7.3/bin/swarm",
            pathLine: "swarm 0.7.3", helperLine: "swarm 0.9.0"
        )
        #expect(
            keg.fixCommand
                == "brew uninstall priyanshuupadhyay/tap/swarm && brew reinstall --cask priyanshuupadhyay/tap/swarm-app"
        )
        let cargo = PathSwarmDrift(
            path: "/Users/owner/.cargo/bin/swarm", resolved: "/Users/owner/.cargo/bin/swarm",
            pathLine: "swarm 0.7.3", helperLine: "swarm 0.9.0"
        )
        #expect(
            cargo.fixCommand
                == "Remove /Users/owner/.cargo/bin/swarm, then run brew reinstall --cask priyanshuupadhyay/tap/swarm-app"
        )
    }
}
