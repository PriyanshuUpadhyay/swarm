import Testing
@testable import SwarmCore

@Suite("Swarm home")
struct SwarmHomeTests {
    @Test("A main, detached, or unknown build uses HOME", arguments: ["main", "HEAD", "", "unknown", nil])
    func mainUsesHome(branch: String?) {
        #expect(SwarmHome.resolve(swarmHome: nil, home: "/home-dir", branch: branch) == "/home-dir")
    }

    @Test("A feature branch build uses its own home")
    func featureBranchUsesOwnHome() {
        #expect(
            SwarmHome.resolve(swarmHome: nil, home: "/home-dir", branch: "ui-polish")
                == "/home-dir/.swarm-ui-polish"
        )
    }

    @Test("Unsafe branch characters become dashes")
    func unsafeCharactersBecomeDashes() {
        #expect(
            SwarmHome.resolve(swarmHome: nil, home: "/home-dir", branch: "feat/x y")
                == "/home-dir/.swarm-feat-x-y"
        )
    }

    @Test("An explicit SWARM_HOME wins", arguments: ["ui-polish", "main"])
    func explicitWins(branch: String) {
        #expect(
            SwarmHome.resolve(swarmHome: "/tmp/explicit", home: "/home-dir", branch: branch)
                == "/tmp/explicit"
        )
    }
}
