import Testing
@testable import SwarmCore

@Suite("Chat chain usage")
struct UsageSummaryTests {
    private func session(_ name: String, provider: String) -> SwarmSession {
        SwarmSession(id: .init(name), talkMode: "lane", adapter: "tmux-solo", cwd: "/repo",
                     createdAt: 100, chairProvider: provider, chairLog: nil, agents: 1, messages: 0, lastMessageAt: nil)
    }

    private func chair(tokens: Int64?, costUsd: Double?) -> SwarmAgent {
        var agent = SwarmAgent(id: SwarmPanePolicy.chair, role: "chat", pane: "%0", alive: true)
        agent.tokens = tokens
        agent.costUsd = costUsd
        return agent
    }

    @Test("A model switch sums each session snapshot once and keeps chain order")
    func modelSwitch() {
        let current = session("current", provider: "codex")
        let earlier = session("earlier", provider: "claude")
        let unrelated = session("unrelated", provider: "claude")
        let chain = UsageSummary.chain(sessions: [current, earlier], usageBySession: [
            current.id: chair(tokens: 120, costUsd: 1.25), earlier.id: chair(tokens: 80, costUsd: 2.5),
            unrelated.id: chair(tokens: 900, costUsd: 10)
        ])
        #expect(chain.tokens == 200)
        #expect(chain.costUsd == 3.75)
        #expect(chain.sessions.map(\.id) == [current.id, earlier.id])
        #expect(chain.sessions.map(\.tokens) == [120, 80])
        #expect(chain.sessions.map(\.costUsd) == [1.25, 2.5])
        #expect(chain.sessions.map(\.provider) == ["codex", "claude"])
    }

    @Test("A one-session chain preserves reported zero")
    func oneSession() {
        let current = session("current", provider: "claude")
        let chain = UsageSummary.chain(sessions: [current], usageBySession: [current.id: chair(tokens: 0, costUsd: 0)])
        #expect(chain.tokens == 0 && chain.costUsd == 0)
        #expect(chain.sessions.count == 1)
        #expect(chain.sessions.first?.tokens == 0 && chain.sessions.first?.costUsd == 0)
    }

    @Test("A session without usage stays in the list and is not reported as zero")
    func missingUsage() {
        let current = session("current", provider: "codex")
        let earlier = session("earlier", provider: "claude")
        let chain = UsageSummary.chain(sessions: [current, earlier], usageBySession: [earlier.id: chair(tokens: 80, costUsd: nil)])
        #expect(chain.tokens == 80 && chain.costUsd == nil)
        #expect(chain.sessions.first?.tokens == nil && chain.sessions.first?.costUsd == nil)
        #expect(chain.sessions.count == 2)
        let unknown = UsageSummary.chain(sessions: [current], usageBySession: [current.id: chair(tokens: nil, costUsd: nil)])
        #expect(unknown.tokens == nil && unknown.costUsd == nil)
        let empty = UsageSummary.chain(sessions: [], usageBySession: [:])
        #expect(empty.tokens == nil && empty.costUsd == nil && empty.sessions.isEmpty)
    }

    @Test("Large token counts saturate without a crash")
    func tokenOverflow() {
        let current = session("current", provider: "codex")
        let earlier = session("earlier", provider: "claude")
        let chain = UsageSummary.chain(sessions: [current, earlier], usageBySession: [
            current.id: chair(tokens: .max, costUsd: nil), earlier.id: chair(tokens: 1, costUsd: nil)
        ])
        #expect(chain.tokens == .max)
    }
}
