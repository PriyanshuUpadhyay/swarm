import Foundation
import Testing
@testable import BloomCore

@Suite("Swarm profiles", .tags(.agentProtocol))
struct SwarmProfilesTests {
    @Test("decodes the roles contract")
    func decodesRoles() async throws {
        let source = source(stdout: #"{"roles":[{"role":"code.complex","runner":"codex-sol-high-agent","provider":"codex","model":"gpt-5.6-sol","effort":"high","sandbox":"workspace-write","fallbacks":[]}] }"#)

        let roles = try await source.roles()

        #expect(roles == [SwarmRole(
            role: "code.complex", runner: "codex-sol-high-agent", provider: "codex",
            model: "gpt-5.6-sol", effort: "high", sandbox: "workspace-write", fallbacks: []
        )])
    }

    @Test("decodes the Claude accounts contract")
    func decodesClaudeAccounts() async throws {
        let source = source(stdout: #"{"provider":"claude","source":"yelo","accounts":[{"name":"sid","email":"someone@example.com","home":"/Users/me/.claude/.profiles/sid","env":{"CLAUDE_CONFIG_DIR":"/Users/me/.claude/.profiles/sid"},"signed_in":true,"remaining_pct":52,"summary":"5h 98% left · 7d 52% left"}],"auto":"sid"}"#)

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
        let source = source(stdout: #"{"provider":"agy","source":null,"accounts":[],"auto":null}"#)

        let accounts = try await source.accounts(provider: "agy")

        #expect(accounts == SwarmAccountList(provider: "agy", source: nil, accounts: [], auto: nil))
    }

    @Test("decodes a usage meter with no matched account")
    func decodesUsage() async throws {
        let source = source(stdout: #"{"meters":[{"provider":"claude","account":null,"label":"cl·work@example.com","window":"7d","used_pct":10,"resets_in":"4d22h","state":"ok","as_of":1789576942}]}"#)

        let meters = try await source.usage()

        #expect(meters == [SwarmUsageMeter(
            provider: "claude", account: nil, label: "cl·work@example.com", window: "7d",
            usedPct: 10, resetsIn: "4d22h", state: "ok", asOf: 1_789_576_942
        )])
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
        let source = source(status: 2, stderr: "routing config is missing\nmore detail\n")

        await #expect(throws: SwarmProfileError.failed("routing config is missing")) {
            try await source.usage()
        }
    }

    @Test("maps undecodable output to failed")
    func mapsInvalidJSON() async {
        let source = source(stdout: "not json")

        await #expect(throws: SwarmProfileError.failed("swarm returned invalid JSON")) {
            try await source.roles()
        }
    }

    private func source(
        status: Int32 = 0, stdout: String = "", stderr: String = ""
    ) -> SwarmCLIProfileSource {
        SwarmCLIProfileSource(environment: [:], cwd: "/tmp") { _, _, _ in
            ShellResult(status: status, stdout: stdout, stderr: stderr)
        }
    }
}
