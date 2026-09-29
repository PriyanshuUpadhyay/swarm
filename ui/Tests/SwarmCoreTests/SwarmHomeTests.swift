import Testing
@testable import SwarmCore

@Suite("Swarm home")
struct SwarmHomeTests {
    /// Shared with the vectors in `src/paths.rs`; keep both lists the same.
    static let vectors: [(branch: String, folder: String?)] = [
        ("main", nil),
        ("", nil),
        ("unknown", ".swarm-unknown"),
        ("ui-polish", ".swarm-ui-polish"),
        ("feat/login", ".swarm-feat-login+a15997df"),
        ("feat-login", ".swarm-feat-login"),
        ("feat/👩‍💻", ".swarm-feat------------+2df12934"),
        ("..", ".swarm-..+a3d4a70d"),
        ("-x", ".swarm--x+4bcd60c0"),
        ("\"main\"", ".swarm--main-+0c126bfe"),
        ("a'b", ".swarm-a-b+2aa1e449"),
    ]

    @Test("Branch folders match the shared vectors", arguments: vectors)
    func folderMatchesVector(vector: (branch: String, folder: String?)) {
        #expect(SwarmHome.folder(branch: vector.branch) == vector.folder)
    }

    @Test("Main and no branch use HOME, others a folder in it")
    func homeOrFolder() {
        #expect(SwarmHome.resolve(swarmHome: nil, home: { "/home-dir" }, branch: "main") == "/home-dir")
        #expect(SwarmHome.resolve(swarmHome: nil, home: { "/home-dir" }, branch: "") == "/home-dir")
        #expect(
            SwarmHome.resolve(swarmHome: nil, home: { "/home-dir" }, branch: "ui-polish")
                == "/home-dir/.swarm-ui-polish"
        )
    }

    @Test("An explicit SWARM_HOME wins without reading HOME", arguments: ["ui-polish", "main"])
    func explicitWins(branch: String) {
        let home: () -> String = { Issue.record("HOME was read"); return "/home-dir" }
        #expect(SwarmHome.resolve(swarmHome: "/tmp/explicit", home: home, branch: branch) == "/tmp/explicit")
    }
}
