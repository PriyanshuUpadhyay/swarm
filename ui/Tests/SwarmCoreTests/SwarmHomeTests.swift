import Testing
@testable import SwarmCore

@Suite("Swarm home")
struct SwarmHomeTests {
    /// Shared with the vectors in `src/paths.rs`; keep both lists the same.
    static let vectors: [(branch: String, folder: String?)] = [
        ("main", ".swarm-main"),
        ("", nil),
        ("HEAD", ".swarm-head+4b9253d8ff1ee183"),
        ("unknown", ".swarm-unknown"),
        ("ui-polish", ".swarm-ui-polish"),
        ("feat/login", ".swarm-feat-login+407712bf7898fb7f"),
        ("feat-login", ".swarm-feat-login"),
        ("feat/👩‍💻", ".swarm-feat------------+351989ced13b5f34"),
        ("..", ".swarm-..+07da1a07b4a03f2d"),
        ("-x", ".swarm--x+07d04207b4982ea0"),
        ("\"main\"", ".swarm--main-+f2c462bd1704f4de"),
        ("a'b", ".swarm-a-b+e63cb31904812ee9"),
        ("feat/" + a(235), ".swarm-feat-" + a(195) + "+92be3c58bd6b9ccb"),
        (a(64) + "e6uomhlyrr3q", ".swarm-" + a(64) + "e6uomhlyrr3q"),
        (a(64) + "zimprpqj6tk7", ".swarm-" + a(64) + "zimprpqj6tk7"),
        (a(200), ".swarm-" + a(200)),
        (a(201), ".swarm-" + a(200) + "+9a253eda0ce95884"),
        ("Feature", ".swarm-feature+43e05bec7713cffd"),
        ("feature", ".swarm-feature"),
        ("UI-Polish", ".swarm-ui-polish+39b24d57f056cb17"),
        (a(64) + "X", ".swarm-" + a(64) + "x+808822a889f90227"),
        (a(64) + "x", ".swarm-" + a(64) + "x"),
    ]

    static func a(_ count: Int) -> String { String(repeating: "a", count: count) }

    @Test("Branch folders match the shared vectors", arguments: vectors)
    func folderMatchesVector(vector: (branch: String, folder: String?)) {
        let folder = SwarmHome.folder(branch: vector.branch)
        #expect(folder == vector.folder)
        #expect((folder?.utf8.count ?? 0) <= 255)
    }

    @Test("Branch folders stay distinct on a case-insensitive disk")
    func foldersIgnoringCaseAreDistinct() {
        let folders = Self.vectors.compactMap { SwarmHome.folder(branch: $0.branch)?.lowercased() }
        #expect(Set(folders).count == folders.count)
    }

    @Test("A release build uses HOME, a dev build a folder in it")
    func homeOrFolder() {
        #expect(SwarmHome.resolve(swarmHome: nil, home: { "/home-dir" }, branch: "") == "/home-dir")
        #expect(SwarmHome.resolve(swarmHome: nil, home: { "/home-dir" }, branch: "main") == "/home-dir/.swarm-main")
        #expect(
            SwarmHome.resolve(swarmHome: nil, home: { "/home-dir" }, branch: "HEAD")
                == "/home-dir/.swarm-head+4b9253d8ff1ee183"
        )
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
