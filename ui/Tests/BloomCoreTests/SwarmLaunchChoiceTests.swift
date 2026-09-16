import Testing
@testable import BloomCore

@Suite("Swarm launch choices")
struct SwarmLaunchChoiceTests {
    private let ORCHESTRATOR = SwarmRole(
        role: "orchestrator", runner: "claude-orchestrator", provider: "claude",
        model: "claude-opus-4-1", effort: "high", sandbox: nil, fallbacks: []
    )
    private let CODER = SwarmRole(
        role: "coder", runner: "codex-coder", provider: "codex",
        model: "gpt-5.6-sol", effort: "xhigh", sandbox: "workspace-write", fallbacks: []
    )
    private let RESEARCHER = SwarmRole(
        role: "researcher", runner: "agy-researcher", provider: "agy",
        model: "gemini-3.1-pro", effort: nil, sandbox: nil, fallbacks: []
    )

    @Test("a runnable role sets the backend, model and effort")
    func roleAppliesControls() throws {
        let choice = SwarmLaunchRole(CODER)
        let controls = try #require(choice.applying(to: ComposerControls()))

        #expect(controls.agentKind == .codex)
        #expect(controls.model == "gpt-5.6-sol")
        #expect(controls.effort == "xhigh")
        #expect(choice.disabledReason == nil)
    }

    @Test("an unsupported provider gives a short reason and changes nothing")
    func unsupportedRole() {
        let choice = SwarmLaunchRole(RESEARCHER)

        #expect(choice.agentKind == nil)
        #expect(choice.disabledReason == "Bloom cannot run agy roles")
        #expect(choice.applying(to: ComposerControls()) == nil)
    }

    @Test("the current model chooses its matching role before the first runnable role")
    func initialRole() {
        var controls = ComposerControls()
        controls.agentKind = .codex
        controls.model = CODER.model
        controls.effort = CODER.effort ?? ""

        #expect(SwarmLaunchRole.initial(
            in: [ORCHESTRATOR, CODER, RESEARCHER], controls: controls
        )?.id == CODER.id)
    }

    @Test("Auto names its concrete account and every account shows usage left")
    func accountChoices() throws {
        let claudeWorkAccount = SwarmAccount(
            name: "work", email: "work@example.test", home: "/tmp/claude-work",
            env: ["CLAUDE_CONFIG_DIR": "/tmp/claude-work"], signedIn: true,
            remainingPct: 52, summary: nil
        )
        let claudePersonalAccount = SwarmAccount(
            name: "personal", email: nil, home: "/tmp/claude-personal",
            env: ["CLAUDE_CONFIG_DIR": "/tmp/claude-personal"], signedIn: true,
            remainingPct: 81, summary: nil
        )
        let list = SwarmAccountList(
            provider: "claude", source: "yelo",
            accounts: [claudeWorkAccount, claudePersonalAccount], auto: "personal"
        )

        let choices = SwarmAccountOption.choices(from: list)
        #expect(choices.map(\.label) == [
            "Auto (personal), 81% left", "work, 52% left", "personal, 81% left"
        ])
        #expect(try #require(choices.first?.account).name == "personal")
    }
}

@Suite("Swarm launch account storage", .tags(.persistence), .scratchDirectory)
struct SwarmLaunchAccountStoreTests {
    @Test("the selected account survives a new Store instance")
    func roundTrip() async throws {
        let path = TestScratch.unique("swarm-launch-account") + ".sqlite"
        let sessionID = SessionID("launch-account-session")
        let account = SwarmLaunchAccount(
            name: "work", environment: ["CODEX_HOME": "/tmp/codex-work"]
        )

        let first = try Store(path: path)
        await account.store(sessionID: sessionID, in: first)
        let second = try Store(path: path)

        #expect(await SwarmLaunchAccount.load(sessionID: sessionID, from: second) == account)
    }
}
