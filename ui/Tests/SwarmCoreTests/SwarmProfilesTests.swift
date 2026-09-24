import Foundation
import Testing
@testable import SwarmCore

@Suite("Swarm profiles")
struct SwarmProfilesTests {
    private enum RunnerFailure: Error { case lost }

    @Test("decodes the roles contract")
    func decodesRoles() async throws {
        let source = source(
            expectedArguments: ["roles", "--json"],
            stdout: #"{"roles":[{"role":"code.complex","runner":"codex-sol-high-agent","provider":"codex","model":"gpt-5.6-sol","effort":"high","sandbox":"workspace-write","fallbacks":[]}],"choices":[]}"#
        )

        let roles = try await source.roles()

        #expect(roles == [SwarmRole(
            role: "code.complex", runner: "codex-sol-high-agent", provider: "codex",
            model: "gpt-5.6-sol", effort: "high", sandbox: "workspace-write", fallbacks: []
        )])
    }

    @Test("reads provider models without using routed roles")
    func decodesModels() async throws {
        let source = source(
            expectedArguments: ["models", "--provider", "codex", "--json"],
            stdout: #"{"provider":"codex","models":[{"id":"gpt-6-sol","label":"GPT-6-Sol"}]}"#
        )
        #expect(try await source.models(provider: "codex") == [
            SwarmModel(id: "gpt-6-sol", label: "GPT-6-Sol")
        ])
    }

    @Test("decodes nullable role fields and fallbacks")
    func decodesRoleOptionals() async throws {
        let source = source(
            expectedArguments: ["roles", "--json"],
            stdout: #"{"roles":[{"role":"code.fast","runner":"claude-fast","provider":"claude","model":"haiku","effort":null,"sandbox":null,"fallbacks":["codex-fast"]}],"choices":[]}"#
        )

        let roles = try await source.roles()

        #expect(roles == [SwarmRole(
            role: "code.fast", runner: "claude-fast", provider: "claude", model: "haiku",
            effort: nil, sandbox: nil, fallbacks: ["codex-fast"]
        )])
    }

    @Test("model edits use the runner shared by routed profiles")
    func updatesRunnerModel() async throws {
        let source = source(
            expectedArguments: ["roles", "set-model", "codex-sol-high-agent", "gpt-6-sol"],
            stdout: "Set model 'gpt-6-sol' on: codex-sol-high-agent\n"
        )

        try await source.setModel(" gpt-6-sol ", for: "codex-sol-high-agent")
    }

    @Test("a failed model edit keeps the CLI error")
    func failedModelEdit() async {
        let source = source(
            expectedArguments: ["roles", "set-model", "codex-sol-high-agent", "bad-model"],
            status: 1, stderr: "agent-routing: invalid model\n"
        )

        await #expect(throws: SwarmProfileError.failed("agent-routing: invalid model")) {
            try await source.setModel("bad-model", for: "codex-sol-high-agent")
        }
    }

    @Test("decodes the Claude accounts contract")
    func decodesClaudeAccounts() async throws {
        let source = source(
            expectedArguments: ["accounts", "--provider", "claude", "--json"],
            stdout: #"{"provider":"claude","source":"yelo","accounts":[{"name":"sid","email":"someone@example.com","home":"/Users/me/.claude/.profiles/sid","env":{"CLAUDE_CONFIG_DIR":"/Users/me/.claude/.profiles/sid"},"signed_in":true,"remaining_pct":52,"summary":"5h 98% left · 7d 52% left"}],"auto":"sid"}"#
        )

        let accounts = try await source.accounts(provider: "claude")

        #expect(accounts == SwarmAccountList(
            provider: "claude", source: "yelo",
            accounts: [SwarmAccount(
                name: "sid", email: "someone@example.com", home: "/Users/me/.claude/.profiles/sid",
                env: ["CLAUDE_CONFIG_DIR": "/Users/me/.claude/.profiles/sid"], signedIn: true,
                remainingPct: 52, summary: "5h 98% left · 7d 52% left"
            )],
            auto: "sid"
        ))
    }

    @Test("decodes the empty AGY accounts contract")
    func decodesAGYAccounts() async throws {
        let source = source(
            expectedArguments: ["accounts", "--provider", "agy", "--json"],
            stdout: #"{"provider":"agy","source":null,"accounts":[],"auto":null}"#
        )

        let accounts = try await source.accounts(provider: "agy")

        #expect(accounts == SwarmAccountList(provider: "agy", source: nil, accounts: [], auto: nil))
    }

    @Test("decodes every usage contract row")
    func decodesUsage() async throws {
        let source = source(
            expectedArguments: ["usage", "--json"],
            stdout: #"{"meters":[{"provider":"claude","account":"work","label":"cl·work@example.com","window":"7d","used_pct":10,"resets_in":"4d22h","state":"ok","reason":null,"as_of":1789576942},{"provider":"claude","account":"sid","label":"cl·sid","window":null,"used_pct":null,"resets_in":null,"state":"logged_out","reason":"logged out","as_of":null}]}"#
        )

        let meters = try await source.usage()

        #expect(meters == [
            SwarmUsageMeter(
                provider: "claude", account: "work", label: "cl·work@example.com",
                window: "7d", usedPct: 10, resetsIn: "4d22h", state: "ok", reason: nil,
                asOf: 1_789_576_942
            ),
            SwarmUsageMeter(
                provider: "claude", account: "sid", label: "cl·sid", window: nil,
                usedPct: nil, resetsIn: nil, state: "logged_out", reason: "logged out", asOf: nil
            ),
        ])
    }

    @Test("uses a fresh scratch directory when no working directory is injected")
    func usesFreshScratchDirectory() async throws {
        let source = SwarmCLIProfileSource(environment: [:]) { _, arguments, cwd in
            #expect(arguments == ["roles", "--json"])
            #expect(cwd == AgentScratchDirectory.current())
            #expect(cwd != NSHomeDirectory())
            #expect(FileManager.default.fileExists(atPath: cwd))
            return ShellResult(status: 0, stdout: #"{"roles":[],"choices":[]}"#, stderr: "")
        }

        _ = try await source.roles()
    }

    @Test("uses SWARM_BIN and the contract arguments")
    func usesConfiguredBinary() async throws {
        let source = SwarmCLIProfileSource(
            environment: ["SWARM_BIN": "/custom/swarm"], cwd: "/tmp/profile-test"
        ) { executable, arguments, cwd in
            #expect(executable == "/custom/swarm")
            #expect(arguments == ["accounts", "--provider", "codex", "--json"])
            #expect(cwd == "/tmp/profile-test")
            return ShellResult(status: 0, stdout: #"{"provider":"codex","source":null,"accounts":[],"auto":null}"#, stderr: "")
        }

        _ = try await source.accounts(provider: "codex")
    }

    @Test("maps a missing swarm binary to unavailable")
    func mapsMissingBinary() async {
        let source = SwarmCLIProfileSource(environment: [:], cwd: "/tmp") { executable, _, _ in
            throw ShellError(command: executable, status: 127, stderr: "swarm not found on PATH")
        }

        await #expect(throws: SwarmProfileError.unavailable("swarm not found on PATH")) {
            try await source.roles()
        }
    }

    @Test("maps a non-zero exit to the first stderr line")
    func mapsFailedExit() async {
        let source = source(
            expectedArguments: ["usage", "--json"], status: 2,
            stderr: "routing config is missing\nmore detail\n"
        )

        await #expect(throws: SwarmProfileError.failed("routing config is missing")) {
            try await source.usage()
        }
    }

    @Test("falls back to stdout when a failed command has no stderr")
    func mapsFailedExitStdout() async {
        let source = source(
            expectedArguments: ["usage", "--json"], status: 137,
            stdout: "process was killed\nmore detail\n"
        )

        await #expect(throws: SwarmProfileError.failed("process was killed")) {
            try await source.usage()
        }
    }

    @Test("gives an empty failed command a useful message")
    func mapsEmptyFailedExit() async {
        let source = source(expectedArguments: ["usage", "--json"], status: 137)

        await #expect(throws: SwarmProfileError.failed("swarm exited 137")) {
            try await source.usage()
        }
    }

    @Test("preserves cancellation")
    func preservesCancellation() async {
        let source = SwarmCLIProfileSource(environment: [:], cwd: "/tmp") { _, _, _ in
            throw CancellationError()
        }

        await #expect(throws: CancellationError.self) {
            try await source.roles()
        }
    }

    @Test("maps another runner error to failed")
    func mapsRunnerError() async {
        let source = SwarmCLIProfileSource(environment: [:], cwd: "/tmp") { _, _, _ in
            throw RunnerFailure.lost
        }

        await #expect(throws: SwarmProfileError.failed("lost")) {
            try await source.roles()
        }
    }

    @Test("maps undecodable output to failed")
    func mapsInvalidJSON() async {
        let source = source(expectedArguments: ["roles", "--json"], stdout: "not json")

        await #expect(throws: SwarmProfileError.failed("swarm returned invalid JSON")) {
            try await source.roles()
        }
    }

    private func source(
        expectedArguments: [String], status: Int32 = 0, stdout: String = "", stderr: String = ""
    ) -> SwarmCLIProfileSource {
        SwarmCLIProfileSource(environment: [:], cwd: "/tmp") { _, arguments, _ in
            #expect(arguments == expectedArguments)
            return ShellResult(status: status, stdout: stdout, stderr: stderr)
        }
    }
}
