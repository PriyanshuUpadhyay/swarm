import Testing
@testable import SwarmCore

@Suite("Onboarding agent choice")
struct OnboardingAgentChoiceTests {
    private func report(
        claude: SetupOutcome = .ready(detail: nil),
        codex: SetupOutcome = .ready(detail: nil),
        grok: SetupOutcome = .missing
    ) -> SetupReport {
        SetupReport(checks: [
            SetupCheck(tool: .git, outcome: .ready(detail: nil)),
            SetupCheck(tool: .claudeCode, outcome: claude),
            SetupCheck(tool: .codex, outcome: codex),
            SetupCheck(tool: .grok, outcome: grok),
            SetupCheck(tool: .gitHub, outcome: .ready(detail: nil)),
        ])
    }

    private let codexModels = [
        AgentModel(id: "gpt-5.5", displayName: "GPT-5.5", supportedEfforts: [
            AgentModelEffort(id: "medium", label: "Medium"),
        ], defaultEffort: "medium"),
        AgentModel(id: "gpt-5.6-sol", displayName: "GPT-5.6 Sol", isDefault: true, supportedEfforts: [
            AgentModelEffort(id: "low", label: "Low"), AgentModelEffort(id: "high", label: "High"),
        ], defaultEffort: "low"),
    ]

    @Test("Offered only between two or more ready agents")
    func offered() {
        #expect(OnboardingAgentChoice.candidates(in: report()) == [.claudeCode, .codex])
        #expect(OnboardingAgentChoice.isOffered(in: report()))
        #expect(!OnboardingAgentChoice.isOffered(in: report(codex: .missing)))
        #expect(!OnboardingAgentChoice.isOffered(in: report(codex: .needsSignIn(detail: nil))))
        #expect(OnboardingAgentChoice.candidates(in: report(grok: .ready(detail: nil))).count == 3)
    }

    @Test("Not offered while an agent row is still being looked at")
    func settling() {
        #expect(OnboardingAgentChoice.candidates(in: report(grok: .pending)).isEmpty)
    }

    @Test("Choosing Codex takes its server's default model and a level that model accepts")
    func choosingCodex() throws {
        let chosen = try #require(
            OnboardingAgentChoice.defaults(choosing: .codex, from: AppDefaults(), models: codexModels)
        )
        #expect(chosen.backend == .codex)
        #expect(chosen.model == "gpt-5.6-sol")
        #expect(chosen.effort == "high")
        #expect(chosen.storedModel == "gpt-5.6-sol")
        #expect(chosen.reviewBackend == .codex)
        #expect(chosen.reviewModel == "gpt-5.6-sol")
    }

    @Test("A fetched agent with no list yet cannot be answered")
    func notYetFetched() {
        #expect(OnboardingAgentChoice.defaults(choosing: .codex, from: AppDefaults(), models: []) == nil)
    }

    @Test("Confirming the agent already in force keeps the model somebody picked")
    func sameAgent() {
        let current = AppDefaults(model: "sonnet", effort: "medium")
        #expect(OnboardingAgentChoice.defaults(choosing: .claudeCode, from: current, models: []) == current)
    }

    @Test("A review deliberately on another agent stays there")
    func separateReview() throws {
        let current = AppDefaults(reviewModel: "sonnet")
        let chosen = try #require(
            OnboardingAgentChoice.defaults(choosing: .codex, from: current, models: codexModels)
        )
        #expect(chosen.reviewBackend == .claudeCode)
        #expect(chosen.reviewModel == "sonnet")
    }

    @Test("Going back to Claude Code uses Swarm's fallbacks rather than a Codex effort")
    func backToClaude() throws {
        let current = AppDefaults(model: "gpt-5.6-sol", effort: "ultra", backend: .codex,
                                  reviewModel: "gpt-5.6-sol", reviewEffort: "ultra", reviewBackend: .codex)
        let chosen = try #require(
            OnboardingAgentChoice.defaults(choosing: .claudeCode, from: current, models: [])
        )
        #expect(chosen.backend == .claudeCode)
        #expect(chosen.model == AppDefaults.fallbackModel)
        #expect(chosen.effort == AppDefaults.fallbackEffort)
        #expect(chosen.reviewBackend == .claudeCode)
    }
}
