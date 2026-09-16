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
        let controls = try #require(CODER.applyingToLaunchControls(ComposerControls()))

        #expect(controls.agentKind == .codex)
        #expect(controls.model == "gpt-5.6-sol")
        #expect(controls.effort == "xhigh")
        #expect(CODER.launchDisabledReason == nil)
    }

    @Test("an unsupported provider stays visible with a short reason")
    func unsupportedRole() {
        #expect(RESEARCHER.launchAgentKind == nil)
        #expect(RESEARCHER.launchDisabledReason == "Bloom cannot run agy roles")
        #expect(RESEARCHER.applyingToLaunchControls(ComposerControls()) == nil)
    }

    @Test("no matching role leaves the resolved controls unchanged")
    func noInitialRoleFallback() {
        var controls = ComposerControls()
        controls.agentKind = .claudeCode
        controls.model = "sonnet"

        #expect(SwarmRole.initialLaunchRole(
            in: [CODER, RESEARCHER], controls: controls
        ) == nil)
    }

    @Test("the current model chooses its exact role")
    func initialRole() {
        var controls = ComposerControls()
        controls.agentKind = .codex
        controls.model = CODER.model
        controls.effort = CODER.effort ?? ""

        #expect(SwarmRole.initialLaunchRole(
            in: [ORCHESTRATOR, CODER, RESEARCHER], controls: controls
        )?.id == CODER.id)
    }

    @Test("Auto names its account and each account shows usage left")
    func accountChoices() throws {
        let list = accountList(auto: "personal")

        let choices = SwarmAccountOption.choices(from: list)
        #expect(choices.map(\.label) == [
            "Auto (personal), 81% left", "work, 52% left", "personal, 81% left"
        ])
        #expect(try #require(choices.first?.account).name == "personal")
        #expect(SwarmAccountOption.initialSelection(in: choices) == .auto)
    }

    @Test("a null Auto is absent and a signed-in named account is selected")
    func nullAuto() {
        let choices = SwarmAccountOption.choices(from: accountList(auto: nil))

        #expect(!choices.contains { $0.selection == .auto })
        #expect(SwarmAccountOption.initialSelection(in: choices) == .named("work"))
    }

    @Test("a signed-out account cannot be selected")
    func signedOutAccount() {
        var list = accountList(auto: nil)
        list.accounts[0].signedIn = false
        let choices = SwarmAccountOption.choices(from: list)
        let work = choices.first { $0.selection == .named("work") }

        #expect(work?.disabledReason == "Not signed in")
        #expect(work?.account == nil)
        #expect(SwarmAccountOption.account(for: .named("work"), in: choices) == nil)
    }

    @Test("only the provider contract key reaches the launch environment")
    func filtersEnvironment() throws {
        var list = accountList(auto: "personal")
        list.accounts[1].env["PATH"] = "/tmp/untrusted"
        let account = try #require(SwarmLaunchAccount.resolve(.auto, from: list))

        #expect(account.environment == ["CLAUDE_CONFIG_DIR": "/tmp/claude-personal"])
        #expect(account.merging(into: ["PATH": "/usr/bin"]) == [
            "PATH": "/usr/bin", "CLAUDE_CONFIG_DIR": "/tmp/claude-personal",
        ])
    }

    private func accountList(auto: String?) -> SwarmAccountList {
        SwarmAccountList(
            provider: "claude", source: "yelo",
            accounts: [
                SwarmAccount(
                    name: "work", email: "work@example.test", home: "/tmp/claude-work",
                    env: ["CLAUDE_CONFIG_DIR": "/tmp/claude-work"], signedIn: true,
                    remainingPct: 52, summary: nil
                ),
                SwarmAccount(
                    name: "personal", email: nil, home: "/tmp/claude-personal",
                    env: ["CLAUDE_CONFIG_DIR": "/tmp/claude-personal"], signedIn: true,
                    remainingPct: 81, summary: nil
                ),
            ],
            auto: auto
        )
    }
}

@Suite("Swarm launch account storage", .tags(.persistence), .scratchDirectory)
struct SwarmLaunchAccountStoreTests {
    @Test("the selected account survives a new Store instance")
    func roundTrip() async throws {
        let path = TestScratch.unique("swarm-launch-account") + ".sqlite"
        let sessionID = SessionID("launch-account-session")
        let account = try #require(SwarmLaunchAccount(
            name: "work", provider: "codex",
            environment: ["CODEX_HOME": "/tmp/codex-work", "PATH": "/tmp/untrusted"]
        ))

        let first = try Store(path: path)
        await account.store(sessionID: sessionID, in: first)
        let second = try Store(path: path)

        #expect(await SwarmLaunchAccount.load(sessionID: sessionID, from: second) == account)
        #expect(account.environment == ["CODEX_HOME": "/tmp/codex-work"])
    }
}
