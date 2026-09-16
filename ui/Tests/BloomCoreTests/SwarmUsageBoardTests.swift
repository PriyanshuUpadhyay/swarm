import Testing
@testable import BloomCore

private struct FakeSwarmProfileSource: SwarmProfileSource {
    let meters: [SwarmUsageMeter]

    func roles() async throws -> [SwarmRole] { [] }

    func accounts(provider: String) async throws -> SwarmAccountList {
        SwarmAccountList(provider: provider, source: nil, accounts: [], auto: nil)
    }

    func usage() async throws -> [SwarmUsageMeter] { meters }
}

private func meter(
    provider: String,
    account: String?,
    label: String,
    window: String,
    usedPercent: Int,
    resetsIn: String? = nil,
    state: String = "ok"
) -> SwarmUsageMeter {
    SwarmUsageMeter(
        provider: provider,
        account: account,
        label: label,
        window: window,
        usedPct: usedPercent,
        resetsIn: resetsIn,
        state: state,
        asOf: nil
    )
}

@Suite("Swarm usage board")
struct SwarmUsageBoardTests {
    @Test("groups providers and accounts in a stable order")
    func groupingAndOrdering() async throws {
        let source: any SwarmProfileSource = FakeSwarmProfileSource(meters: [
            meter(provider: "codex", account: "CODER", label: "cx·coder", window: "7d", usedPercent: 30),
            meter(provider: "claude", account: nil, label: "cl·work@example.com", window: "7d", usedPercent: 10),
            meter(provider: "claude", account: "ORCHESTRATOR", label: "cl·orchestrator", window: "5h", usedPercent: 20),
        ])

        let board = SwarmUsageBoard.make(from: try await source.usage())

        #expect(board.providers.map(\.key) == ["claude", "codex"])
        #expect(board.providers[0].accounts.map(\.title) == ["cl·work@example.com", "ORCHESTRATOR"])
        #expect(board.providers[1].accounts.map(\.title) == ["CODER"])
    }

    @Test("orders known windows before unknown windows")
    func windowOrdering() {
        let board = SwarmUsageBoard.make(from: [
            meter(provider: "claude", account: "ORCHESTRATOR", label: "cl·orchestrator", window: "other", usedPercent: 40),
            meter(provider: "claude", account: "ORCHESTRATOR", label: "cl·orchestrator", window: "fb", usedPercent: 30),
            meter(provider: "claude", account: "ORCHESTRATOR", label: "cl·orchestrator", window: "7d", usedPercent: 20),
            meter(provider: "claude", account: "ORCHESTRATOR", label: "cl·orchestrator", window: "5h", usedPercent: 10),
        ])

        #expect(board.providers[0].accounts[0].meters.map(\.window) == ["5h", "7d", "fb", "other"])
    }

    @Test("formats use, reset and stale state")
    func textFormatting() throws {
        let board = SwarmUsageBoard.make(from: [
            meter(
                provider: "claude",
                account: "ORCHESTRATOR",
                label: "cl·orchestrator",
                window: "7d",
                usedPercent: 10,
                resetsIn: "4d22h",
                state: "stale"
            ),
        ])
        let reading = try #require(board.providers.first?.accounts.first?.meters.first)

        #expect(reading.usedText == "10% used")
        #expect(reading.resetText == "Resets in 4d22h")
        #expect(reading.statusText == "Stale")
        #expect(reading.fill == 0.1)
        #expect(board.accessibilityLabel == "Claude, ORCHESTRATOR, 7d, 10% used, Resets in 4d22h, Stale")
    }
}
