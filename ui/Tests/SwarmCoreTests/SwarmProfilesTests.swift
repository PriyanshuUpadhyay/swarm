import Foundation
import Testing
@testable import SwarmCore

@Suite("Swarm profiles")
struct SwarmProfilesTests {
    private enum RunnerFailure: Error { case lost }

    @Test("decodes the profiles contract with its revision and import")
    func decodesProfiles() async throws {
        let source = source(
            expectedArguments: ["roles", "--json"],
            stdout: #"{"revision":"a1b2c3d4e5f6","version":1,"min_usage_left_pct":5,"imported":{"from":"/h/roles.json","unmapped":["old.route"]},"profiles":[{"name":"chat","runners":[{"provider":"claude","model":"opus","effort":"high"},{"provider":"codex","model":"gpt-6.1-sol","effort":"high","sandbox":"workspace-write"}]}]}"#
        )

        let list = try await source.profiles()

        #expect(list.revision == "a1b2c3d4e5f6")
        #expect(list.minUsageLeftPct == 5)
        #expect(list.imported == SwarmProfileImport(from: "/h/roles.json", unmapped: ["old.route"]))
        #expect(list.profiles == [SwarmProfile(name: "chat", runners: [
            SwarmRunner(provider: "claude", model: "opus", effort: "high"),
            SwarmRunner(provider: "codex", model: "gpt-6.1-sol", effort: "high", sandbox: "workspace-write"),
        ])])
    }

    @Test("decodes the launch check, where a profile with no runner that can run has no pick")
    func decodesCheck() async throws {
        let source = source(
            expectedArguments: ["roles", "check", "--json"],
            stdout: #"{"profiles":[{"name":"chat","pick":1,"skipped":[{"index":0,"code":"low_usage","text":"usage 2% left (threshold 5%)"}]},{"name":"council.gemini","skipped":[{"index":0,"code":"cli_missing","text":"agy CLI not found on PATH"}]}]}"#
        )

        let checks = try await source.check()

        #expect(checks == [
            SwarmProfileCheck(name: "chat", pick: 1, skipped: [
                SwarmSkip(index: 0, code: "low_usage", text: "usage 2% left (threshold 5%)"),
            ]),
            SwarmProfileCheck(name: "council.gemini", pick: nil, skipped: [
                SwarmSkip(index: 0, code: "cli_missing", text: "agy CLI not found on PATH"),
            ]),
        ])
    }

    @Test("decodes the providers contract")
    func decodesProviders() async throws {
        let source = source(
            expectedArguments: ["providers", "--json"],
            stdout: #"{"providers":[{"id":"codex","label":"Codex","efforts":["low","high"],"default_effort":"medium","accounts":true,"fields":[{"name":"sandbox","label":"Sandbox","values":["read-only","workspace-write"],"default":"workspace-write"}]}]}"#
        )

        let providers = try await source.providers()

        #expect(providers.map(\.id) == ["codex"])
        #expect(providers[0].defaultEffort == "medium")
        #expect(providers[0].fields[0].default == "workspace-write")
        #expect(providers[0].runner(model: "gpt-6-luna")
            == SwarmRunner(provider: "codex", model: "gpt-6-luna", effort: "medium", sandbox: "workspace-write"))
    }

    @Test("reads provider models and each model's efforts")
    func decodesModels() async throws {
        let source = source(
            expectedArguments: ["models", "--provider", "codex", "--json"],
            stdout: #"{"provider":"codex","models":[{"id":"gpt-6-sol","label":"GPT-6-Sol","efforts":["low","ultra"]},{"id":"gpt-old","label":"gpt-old"}]}"#
        )
        #expect(try await source.models(provider: "codex") == [
            SwarmModel(id: "gpt-6-sol", label: "GPT-6-Sol", efforts: ["low", "ultra"]),
            SwarmModel(id: "gpt-old", label: "gpt-old"),
        ])
    }

    @Test("saves one whole profile against the revision it was read at")
    func savesProfile() async throws {
        let profile = SwarmProfile(name: "code.complex", runners: [
            SwarmRunner(provider: "claude", model: "opus", effort: "high", permission: "auto"),
        ])
        let source = SwarmCLIProfileSource(environment: [:], cwd: "/tmp") { _, arguments, _ in
            #expect(arguments.count == 5)
            #expect(Array(arguments.prefix(4)) == ["roles", "save", "--revision", "a1b2c3d4e5f6"])
            let sent = try JSONDecoder().decode(SwarmProfile.self, from: Data(arguments[4].utf8))
            #expect(sent == profile)
            #expect(!arguments[4].contains("\"id\""))
            return ShellResult(status: 0, stdout: #"{"revision":"ffeeddccbbaa"}"#, stderr: "")
        }

        #expect(try await source.save(profile, revision: "a1b2c3d4e5f6") == "ffeeddccbbaa")
    }

    @Test("a refused save keeps every broken rule")
    func failedSave() async {
        let source = SwarmCLIProfileSource(environment: [:], cwd: "/tmp") { _, _, _ in
            ShellResult(status: 1, stdout: "", stderr: "swarm: chat: runner 1 has no model\nchat: runner 2 has no effort\n")
        }

        await #expect(throws: SwarmProfileError.failed("swarm: chat: runner 1 has no model\nchat: runner 2 has no effort")) {
            try await source.save(SwarmProfile(name: "chat", runners: []), revision: "old")
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
            return ShellResult(status: 0, stdout: #"{"revision":"r","min_usage_left_pct":5,"profiles":[]}"#, stderr: "")
        }

        _ = try await source.profiles()
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

    @Test("falls back to the app's bundled swarm only when nothing else names one")
    func fallsBackToBundledBinary() {
        let bundledSwarm = URL(fileURLWithPath: "/bin/sh")
        let missingBundle = URL(fileURLWithPath: "/nonexistent/Swarm.app/Contents/MacOS/swarm")
        let notOnPath: (String) -> String? = { _ in nil }

        #expect(SwarmCLIBus.appEnvironment([:], bundled: bundledSwarm, which: notOnPath)
            == ["SWARM_BIN": "/bin/sh"])
        #expect(SwarmCLIBus.appEnvironment([:], bundled: bundledSwarm, which: { "/opt/bin/\($0)" })
            == [:])
        #expect(SwarmCLIBus.appEnvironment(
            ["SWARM_BIN": "/custom/swarm"], bundled: bundledSwarm, which: notOnPath
        ) == ["SWARM_BIN": "/custom/swarm"])
        #expect(SwarmCLIBus.appEnvironment([:], bundled: missingBundle, which: notOnPath) == [:])
    }

    @Test("maps a missing swarm binary to unavailable")
    func mapsMissingBinary() async {
        let source = SwarmCLIProfileSource(environment: [:], cwd: "/tmp") { executable, _, _ in
            throw ShellError(command: executable, status: 127, stderr: "swarm not found on PATH")
        }

        await #expect(throws: SwarmProfileError.unavailable("swarm not found on PATH")) {
            try await source.profiles()
        }
    }

    @Test("maps a non-zero exit to the whole stderr")
    func mapsFailedExit() async {
        let source = source(
            expectedArguments: ["usage", "--json"], status: 2,
            stderr: "routing config is missing\nmore detail\n"
        )

        await #expect(throws: SwarmProfileError.failed("routing config is missing\nmore detail")) {
            try await source.usage()
        }
    }

    @Test("falls back to stdout when a failed command has no stderr")
    func mapsFailedExitStdout() async {
        let source = source(
            expectedArguments: ["usage", "--json"], status: 137,
            stdout: "process was killed\nmore detail\n"
        )

        await #expect(throws: SwarmProfileError.failed("process was killed\nmore detail")) {
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
            try await source.profiles()
        }
    }

    @Test("maps another runner error to failed")
    func mapsRunnerError() async {
        let source = SwarmCLIProfileSource(environment: [:], cwd: "/tmp") { _, _, _ in
            throw RunnerFailure.lost
        }

        await #expect(throws: SwarmProfileError.failed("lost")) {
            try await source.profiles()
        }
    }

    @Test("maps undecodable output to failed")
    func mapsInvalidJSON() async {
        let source = source(expectedArguments: ["roles", "--json"], stdout: "not json")

        await #expect(throws: SwarmProfileError.failed("swarm returned invalid JSON")) {
            try await source.profiles()
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
