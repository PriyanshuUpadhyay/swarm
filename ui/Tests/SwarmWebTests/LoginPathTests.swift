import Foundation
import Testing
@testable import SwarmWeb

@Suite("Login PATH")
struct LoginPathTests {
    @Test("reads PATH from env -0 output after a banner, keeping absolute entries once")
    func environmentDump() {
        let dump = Data("Welcome banner with no newline\nHOME=/Users/owner\0PATH=/opt/homebrew/bin:relative:/usr/bin:/opt/homebrew/bin\0TERM=dumb\0".utf8)
        #expect(LoginPath.entries(inEnvironmentDump: dump) == ["/opt/homebrew/bin", "/usr/bin"])
        #expect(LoginPath.entries(inEnvironmentDump: Data("HOME=/Users/owner\0".utf8)).isEmpty)
    }

    @Test("puts the login shell first, then the inherited PATH, then the guesses")
    func mergeOrder() {
        let path = LoginPath.merged(login: ["/custom/bin", "/usr/bin"], inherited: "/usr/bin:/bin", home: "/Users/owner")
        let entries = path.components(separatedBy: ":")
        #expect(Array(entries.prefix(3)) == ["/custom/bin", "/usr/bin", "/bin"])
        #expect(entries.contains("/opt/homebrew/bin"))
        #expect(entries.contains("/Users/owner/.cargo/bin"))
        #expect(Set(entries).count == entries.count)
    }

    @Test("gives up on a command that outlives its timeout")
    func timeout() {
        let start = Date()
        #expect(LoginPath.output("/bin/sleep", ["5"], timeout: 0.2) == nil)
        #expect(Date().timeIntervalSince(start) < 2)
        #expect(LoginPath.output("/bin/echo", ["hi"], timeout: 2) == Data("hi\n".utf8))
        #expect(LoginPath.output("/usr/bin/false", [], timeout: 2) == nil)
    }
}

@Suite("PATH swarm check")
final class PathDriftTests {
    let folder: URL
    let helper: String

    init() throws {
        folder = FileManager.default.temporaryDirectory.appendingPathComponent("path-drift-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        helper = try Self.standIn(in: folder, named: "helper-swarm", script: "echo 'swarm 0.9.0 abc1234 '")
    }

    deinit { try? FileManager.default.removeItem(at: folder) }

    /// A folder holding one `swarm` that prints `script`'s output, as a login PATH entry.
    func pathEntry(_ name: String, script: String) throws -> String {
        let entry = folder.appendingPathComponent(name)
        try FileManager.default.createDirectory(at: entry, withIntermediateDirectories: true)
        _ = try Self.standIn(in: entry, named: "swarm", script: script)
        return entry.path
    }

    static func standIn(in folder: URL, named name: String, script: String) throws -> String {
        let path = folder.appendingPathComponent(name).path
        try "#!/bin/sh\n\(script)\n".write(toFile: path, atomically: true, encoding: .utf8)
        try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: path)
        return path
    }

    func check(_ loginPath: [String], branch: String = "", dismissed: [String] = []) -> PathDrift? {
        PathDrift.check(branch: branch, loginPath: loginPath, helper: helper, dismissed: dismissed)
    }

    @Test("an older swarm on the login PATH is drift with both version lines")
    func olderSwarmIsDrift() throws {
        let older = try pathEntry("old", script: "echo 'swarm 0.7.3 def5678 '")
        let drift = try #require(check(["/nonexistent", older]))
        #expect(drift.path == "\(older)/swarm")
        #expect(drift.pathLine == "swarm 0.7.3 def5678")
        #expect(drift.helperLine == "swarm 0.9.0 abc1234")
        #expect(drift.key == "\(older)/swarm|swarm 0.7.3 def5678|swarm 0.9.0 abc1234")
    }

    @Test("the cask link to the helper itself is no drift")
    func linkToHelperIsNoDrift() throws {
        let entry = folder.appendingPathComponent("cask-bin")
        try FileManager.default.createDirectory(at: entry, withIntermediateDirectories: true)
        try FileManager.default.createSymbolicLink(atPath: entry.appendingPathComponent("swarm").path, withDestinationPath: helper)
        #expect(check([entry.path]) == nil)
    }

    @Test("same line, no swarm, an unreadable version, or a branch build is no drift")
    func quietCases() throws {
        #expect(check([try pathEntry("copy", script: "echo 'swarm 0.9.0 abc1234 '")]) == nil)
        #expect(check([folder.appendingPathComponent("empty").path]) == nil)
        #expect(check([try pathEntry("broken", script: "exit 1")]) == nil)
        let older = try pathEntry("old", script: "echo 'swarm 0.7.3 def5678 '")
        #expect(check([older], branch: "main") == nil)
    }

    @Test("a dismissed pair is no drift, and a new release asks again")
    func dismissedPair() throws {
        let older = try pathEntry("old", script: "echo 'swarm 0.7.3 def5678 '")
        let key = try #require(check([older])).key
        #expect(check([older], dismissed: [key]) == nil)
        _ = try Self.standIn(in: URL(fileURLWithPath: older), named: "swarm", script: "echo 'swarm 0.8.0 0123abc '")
        #expect(check([older], dismissed: [key]) != nil)
    }

    @Test("a Homebrew keg gets the brew command, any other copy is named first")
    func fixCommand() {
        let keg = PathDrift(path: "/opt/homebrew/bin/swarm", resolved: "/opt/homebrew/Cellar/swarm/0.7.3/bin/swarm",
                            pathLine: "swarm 0.7.3", helperLine: "swarm 0.9.0")
        #expect(keg.fixCommand == PathDrift.brewFix)
        let cargo = PathDrift(path: "/Users/owner/.cargo/bin/swarm", resolved: "/Users/owner/.cargo/bin/swarm",
                              pathLine: "swarm 0.7.3", helperLine: "swarm 0.9.0")
        #expect(cargo.fixCommand == "Remove /Users/owner/.cargo/bin/swarm, then run \(PathDrift.brewFix)")
    }

    @Test("the README and make install name the alert's brew command", arguments: ["README.md", "ui/Tools/build.sh"])
    func sameBrewFixEverywhere(file: String) throws {
        let repo = URL(fileURLWithPath: #filePath).deletingLastPathComponent()
            .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
        let text = try String(contentsOf: repo.appendingPathComponent(file), encoding: .utf8)
        #expect(text.contains(PathDrift.brewFix))
    }
}
