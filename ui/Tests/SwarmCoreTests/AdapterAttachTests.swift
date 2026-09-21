import Foundation
import Testing
@testable import SwarmCore

@Suite("Adapter attach capability")
struct AdapterAttachTests {
    private func adapterLines(named adapterConfigFile: String) throws -> [String] {
        let candidateRoots = [
            URL(fileURLWithPath: #filePath),
            URL(fileURLWithPath: #filePath).resolvingSymlinksInPath(),
        ]

        for candidateRoot in candidateRoots {
            var directory = candidateRoot.deletingLastPathComponent()
            for _ in 0..<8 {
                let candidate = directory.appendingPathComponent("adapters").appendingPathComponent(adapterConfigFile)
                if FileManager.default.fileExists(atPath: candidate.path) {
                    return try String(contentsOf: candidate, encoding: .utf8)
                        .components(separatedBy: "\n")
                }
                directory = directory.deletingLastPathComponent()
            }
        }

        throw CocoaError(.fileNoSuchFile)
    }

    @Test("tmux-solo adapter defines an attach command")
    func soloDefinesAttach() throws {
        let soloConfigurationLines = try adapterLines(named: "tmux-solo.conf")
        #expect(soloConfigurationLines.contains { $0.hasPrefix("attach =") })
    }

    @Test("plain tmux adapter defines no attach command")
    func plainTmuxHasNoAttach() throws {
        let plainTmuxConfigurationLines = try adapterLines(named: "tmux.conf")
        #expect(!plainTmuxConfigurationLines.contains { $0.hasPrefix("attach =") })
    }

    @Test("herdr adapter defines no attach command")
    func herdrHasNoAttach() throws {
        let herdrConfigurationLines = try adapterLines(named: "herdr.conf")
        #expect(!herdrConfigurationLines.contains { $0.hasPrefix("attach =") })
    }

    @Test("chair launches on the swarm server")
    func chairLaunchesOnTheSwarmServer() {
        let chairEnvironment = SwarmChairLaunch.environment(
            session: SwarmSessionID("session-1"),
            home: "/tmp/swarm-home"
        )
        #expect(chairEnvironment["SWARM_ADAPTER"] == "tmux-solo")
    }
}
