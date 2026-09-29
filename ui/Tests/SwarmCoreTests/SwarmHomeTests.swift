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
        ("feat/" + a(235), ".swarm-feat-" + a(59) + "+412b964b"),
        (a(64), ".swarm-" + a(64)),
        (a(65), ".swarm-" + a(64) + "+2dd603ec"),
        (a(64) + String(repeating: "b", count: 36), ".swarm-" + a(64) + "+9c728705"),
        (a(64) + String(repeating: "c", count: 36), ".swarm-" + a(64) + "+2410b2b9"),
    ]

    static func a(_ count: Int) -> String { String(repeating: "a", count: count) }

    @Test("Branch folders match the shared vectors", arguments: vectors)
    func folderMatchesVector(vector: (branch: String, folder: String?)) {
        let folder = SwarmHome.folder(branch: vector.branch)
        #expect(folder == vector.folder)
        #expect((folder?.utf8.count ?? 0) <= 255)
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
