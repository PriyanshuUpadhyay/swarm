import Testing
@testable import SwarmCore

private func meter(
    provider: String,
    account: String?,
    label: String,
    window: String?,
    usedPercent: Int?,
    resetsIn: String? = nil,
    state: String = "ok",
    reason: String? = nil
) -> SwarmUsageMeter {
    SwarmUsageMeter(
        provider: provider,
        account: account,
        label: label,
        window: window,
        usedPct: usedPercent,
        resetsIn: resetsIn,
        state: state,
        reason: reason,
        asOf: nil
    )
}

@Suite("Swarm usage board")
struct SwarmUsageBoardTests {
    @Test("an empty report makes an empty board")
    func emptyBoard() {
        #expect(SwarmUsageBoard.make(from: []).isEmpty)
    }

    @Test("groups providers and accounts in a stable order")
    func groupingAndOrdering() {
        let board = SwarmUsageBoard.make(from: [
            meter(provider: "codex", account: "CODER", label: "cx·coder", window: "7d", usedPercent: 30),
            meter(provider: "claude", account: nil, label: "cl·work@example.com", window: "7d", usedPercent: 10),
            meter(provider: "claude", account: "ORCHESTRATOR", label: "cl·orchestrator", window: "5h", usedPercent: 20),
        ])

        #expect(board.providers.map(\.key) == ["claude", "codex"])
        #expect(board.providers[0].accounts.map(\.title) == ["cl·work@example.com", "ORCHESTRATOR"])
        #expect(board.providers[1].accounts.map(\.title) == ["CODER"])
    }

    @Test("meter style changes the words and bar direction")
    func meterStyle() throws {
        let source = [
            meter(provider: "claude", account: "ORCHESTRATOR", label: "cl·orchestrator", window: "7d", usedPercent: 10),
        ]
        let left = try #require(SwarmUsageBoard.make(from: source).providers.first?.accounts.first?.meters.first)
        let used = try #require(SwarmUsageBoard.make(
            from: source,
            options: UsageDisplayOptions(meterStyle: .used)
        ).providers.first?.accounts.first?.meters.first)

        #expect(left.usedText == "90% left")
        #expect(left.fill == 0.9)
        #expect(used.usedText == "10% used")
        #expect(used.fill == 0.1)
    }

    @Test("layout orders providers and hides on demand rows until expanded")
    func layout() {
        var layout = UsageLayout(providerOrder: [.codex, .claudeCode])
        layout.setPlacement(.alwaysVisible, for: UsageMetricID("claudeCode/five_hour"))
        layout.setPlacement(.onDemand, for: UsageMetricID("claudeCode/seven_day"))
        let source = [
            meter(provider: "claude", account: "ORCHESTRATOR", label: "cl·orchestrator", window: "5h", usedPercent: 20),
            meter(provider: "claude", account: "ORCHESTRATOR", label: "cl·orchestrator", window: "7d", usedPercent: 30),
            meter(provider: "codex", account: "CODER", label: "cx·coder", window: "7d", usedPercent: 40),
        ]

        let collapsed = SwarmUsageBoard.make(from: source, layout: layout)
        #expect(collapsed.providers.map(\.key) == ["codex", "claude"])
        #expect(collapsed.providers[1].accounts[0].meters.map(\.window) == ["5h"])

        layout.expandedProviders.insert(.claudeCode)
        let expanded = SwarmUsageBoard.make(from: source, layout: layout)
        #expect(expanded.providers[1].accounts[0].meters.map(\.window) == ["5h", "7d"])
    }

    @Test("a hidden window stays hidden when its percent is absent")
    func hiddenUnmeasuredWindow() {
        let hidden = UsageMetricID("claudeCode/seven_day")
        var layout = UsageLayout()
        layout.setHidden(true, for: hidden)

        let board = SwarmUsageBoard.make(from: [
            meter(
                provider: "claude", account: "ORCHESTRATOR", label: "cl·orchestrator",
                window: "7d", usedPercent: nil, state: "stale"
            ),
        ], layout: layout)

        #expect(board.isEmpty)
    }

    @Test("repeated layout ids keep their first position")
    func repeatedLayoutIDs() {
        let sevenDay = UsageMetricID("claudeCode/seven_day")
        let fiveHour = UsageMetricID("claudeCode/five_hour")
        let layout = UsageLayout(
            providerOrder: [.codex, .claudeCode, .codex],
            metricOrder: [sevenDay, fiveHour, sevenDay]
        )
        let board = SwarmUsageBoard.make(from: [
            meter(provider: "claude", account: "ORCHESTRATOR", label: "cl·orchestrator", window: "5h", usedPercent: 20),
            meter(provider: "claude", account: "ORCHESTRATOR", label: "cl·orchestrator", window: "7d", usedPercent: 30),
            meter(provider: "codex", account: "CODER", label: "cx·coder", window: "7d", usedPercent: 40),
        ], layout: layout)

        #expect(board.providers.map(\.key) == ["codex", "claude"])
        #expect(board.providers[1].accounts[0].meters.map(\.window) == ["7d", "5h"])
    }

    @Test("status rows have one message and no bar values")
    func statusRow() throws {
        let board = SwarmUsageBoard.make(from: [
            meter(
                provider: "codex", account: "REVIEWER", label: "cx·reviewer",
                window: nil, usedPercent: nil, state: "logged_out", reason: "Sign in to Codex"
            ),
        ])
        let reading = try #require(board.providers.first?.accounts.first?.meters.first)

        #expect(reading.message == "Sign in to Codex")
        #expect(reading.usedText == nil)
        #expect(reading.fill == nil)
    }

    @Test("severity and stale state ignore meter style and letter case")
    func severityAndState() throws {
        let board = SwarmUsageBoard.make(from: [
            meter(
                provider: "claude", account: "ORCHESTRATOR", label: "cl·orchestrator",
                window: "7d", usedPercent: 97, resetsIn: "4d22h", state: "STALE"
            ),
        ])
        let reading = try #require(board.providers.first?.accounts.first?.meters.first)

        #expect(reading.severity == .critical)
        #expect(reading.severity?.word == "Nearly gone")
        #expect(reading.isStale)
        #expect(reading.statusText == "Stale")
        #expect(
            board.accessibilityLabel
                == "Claude, ORCHESTRATOR, 7d, 3% left, Nearly gone, Resets in 4d22h, Stale"
        )
    }

    @Test("equal account titles use their stable keys")
    func accountTie() {
        let board = SwarmUsageBoard.make(from: [
            meter(provider: "claude", account: nil, label: "sid", window: "7d", usedPercent: 10),
            meter(provider: "claude", account: "sid", label: "cl·sid", window: "5h", usedPercent: 20),
        ])

        #expect(board.providers[0].accounts.map(\.key) == ["account/sid", "label/sid"])
    }
}

@Suite("Swarm usage failures")
struct SwarmUsageFailureStateTests {
    @Test("the second failed ask marks old readings stale")
    func thresholdAndReset() {
        var failures = SwarmUsageFailureState()

        let first = failures.failed()
        let second = failures.failed()
        #expect(!first)
        #expect(second)
        failures.reset()
        let afterSuccess = failures.failed()
        #expect(!afterSuccess)
    }
}
