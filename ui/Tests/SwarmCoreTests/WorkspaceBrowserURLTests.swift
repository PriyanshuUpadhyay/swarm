import Testing
import Foundation
@testable import SwarmCore

/// What a browser pane opens on, and the order the two sources are asked in.
@Suite("Workspace browser URL", .scratchDirectory)
struct WorkspaceBrowserURLTests {
    private let environment = [
        "SWARM_UI_PORT": "3100",
        "SWARM_UI_PROJECT_NAME": "shop",
        "SWARM_UI_WORKSPACE_NAME": "feature-checkout",
    ]

    // MARK: - Which source answers

    @Test("the port is the answer when nothing states an address")
    func portIsTheFallback() {
        let address = WorkspaceBrowserURL.resolve(
            written: nil, stated: nil, environment: environment, port: 3100
        )
        #expect(address == "http://localhost:3100")
    }

    @Test("a workspace holding no port opens on nothing, as it did before any of this")
    func noPortIsEmpty() {
        let address = WorkspaceBrowserURL.resolve(
            written: nil, stated: nil, environment: [:], port: 0
        )
        #expect(address.isEmpty)
    }

    @Test("the settings file beats the port")
    func statedBeatsThePort() {
        let address = WorkspaceBrowserURL.resolve(
            written: nil, stated: "http://localhost:$SWARM_UI_PORT/admin",
            environment: environment, port: 3100
        )
        #expect(address == "http://localhost:3100/admin")
    }

    @Test("what the script wrote beats the settings file")
    func writtenBeatsStated() {
        let address = WorkspaceBrowserURL.resolve(
            written: "https://shop-a1b2c3d4.test\n", stated: "http://localhost:$SWARM_UI_PORT",
            environment: environment, port: 3100
        )
        #expect(address == "https://shop-a1b2c3d4.test")
    }

    /// A script that ran, decided it had nothing to say and truncated the file is a script saying
    /// nothing, not a script saying "open on the empty string".
    @Test("an empty file falls through to the settings file")
    func emptyWrittenFallsThrough() {
        let address = WorkspaceBrowserURL.resolve(
            written: "\n  \n", stated: "https://stated.test", environment: environment, port: 3100
        )
        #expect(address == "https://stated.test")
    }

    // MARK: - What counts as an address

    @Test("the first line is the address, whatever follows it")
    func firstLineWins() {
        let address = WorkspaceBrowserURL.resolve(
            written: "https://shop.test/login\nnot this\n", stated: nil,
            environment: environment, port: 3100
        )
        #expect(address == "https://shop.test/login")
    }

    @Test("a missing scheme is filled in rather than refused")
    func schemeIsFilledIn() {
        let written = WorkspaceBrowserURL.resolve(
            written: "shop.test", stated: nil, environment: environment, port: 0
        )
        #expect(written == "http://shop.test")

        let stated = WorkspaceBrowserURL.resolve(
            written: nil, stated: "localhost:$SWARM_UI_PORT", environment: environment, port: 3100
        )
        #expect(stated == "http://localhost:3100")
    }

    // MARK: - Expansion

    @Test("both spellings of a variable expand")
    func bothSpellingsExpand() {
        let expanded = WorkspaceBrowserURL.expand(
            "https://${SWARM_UI_PROJECT_NAME}-$SWARM_UI_WORKSPACE_NAME.test:$SWARM_UI_PORT/",
            with: environment
        )
        #expect(expanded == "https://shop-feature-checkout.test:3100/")
    }

    /// Blanking it would produce `http://localhost:` and a question about Swarm. Left alone it
    /// produces an address bar with the typo in it.
    @Test("a name nothing sets is left as it was typed")
    func unknownNamesSurvive() {
        #expect(
            WorkspaceBrowserURL.expand("http://localhost:$SWARM_UI_PROT", with: environment)
                == "http://localhost:$SWARM_UI_PROT"
        )
        #expect(
            WorkspaceBrowserURL.expand("http://localhost:${SWARM_UI_PORT", with: environment)
                == "http://localhost:${SWARM_UI_PORT"
        )
        #expect(WorkspaceBrowserURL.expand("cost: $5", with: environment) == "cost: $5")
        #expect(WorkspaceBrowserURL.expand("trailing $", with: environment) == "trailing $")
    }

    // MARK: - The file on disk

    @Test("the file the scripts write is read from the worktree's scratch folder")
    func readsTheFile() throws {
        let worktree = TestScratch.unique("swarm-browser-url")
        let path = WorkspaceBrowserURL.path(inWorktree: worktree)
        try FileManager.default.createDirectory(
            atPath: (path as NSString).deletingLastPathComponent, withIntermediateDirectories: true
        )
        try "https://written.test\n".write(toFile: path, atomically: true, encoding: .utf8)

        var settings = RepoSettings()
        settings.browserURL = "https://stated.test"
        let address = WorkspaceBrowserURL.read(
            worktree: worktree, settings: settings, environment: environment, port: 3100
        )
        #expect(address == "https://written.test")
    }

    @Test("a worktree with no such file uses the settings file")
    func missingFileFallsThrough() {
        var settings = RepoSettings()
        settings.browserURL = "http://localhost:$SWARM_UI_PORT/admin"
        let address = WorkspaceBrowserURL.read(
            worktree: TestScratch.unique("swarm-browser-url-empty"),
            settings: settings, environment: environment, port: 3100
        )
        #expect(address == "http://localhost:3100/admin")
    }

    /// The address is a fact about one machine, and one arriving in somebody's pull request is
    /// what `WorktreeScratch` exists to stop.
    @Test("the file git cannot see is where the address goes")
    func theFileIsShielded() {
        #expect(WorktreeScratch.isShielded(WorkspaceBrowserURL.file))
    }
}
