import Testing
@testable import SwarmCore

@Suite("Switch model readiness")
struct ModelSwitchChoiceTests {
    let codexChat = ModelSwitchChoice(currentProvider: "codex", currentModel: "gpt-5.5")

    @Test("An unselected live chat stays enabled until its detail can decide")
    func unselectedChat() {
        #expect(tabReason(agents: []) == nil)
        #expect(tabReason(agents: [agent("orchestrator", state: "done")]) == nil)
        #expect(tabReason(agents: [agent("worker", state: "working")]) == nil)
    }

    @Test("A known working or waiting chair disables its tab, but an ended chair does not")
    func chairState() {
        let busy = ModelSwitchChoice.disabledReason(readOnlyReason: nil, waitingForModel: false,
                                                    isSending: false, isRunning: true)
        #expect(tabReason(agents: [agent("orchestrator", state: "working")]) == busy)
        #expect(tabReason(agents: [agent("orchestrator", state: "waiting")]) == busy)
        #expect(tabReason(agents: [agent("orchestrator", state: "working", alive: false)]) == nil)
    }

    @Test("The session's custom chair decides readiness, and a read-only reason wins")
    func customChair() {
        #expect(tabReason(agents: [agent("custom-chair", state: "working")], chairID: "custom-chair") != nil)
        #expect(tabReason(agents: [agent("custom-chair", state: "done"), agent("worker", state: "working")],
                         chairID: "custom-chair") == nil)
        #expect(tabReason(agents: [agent("custom-chair", state: "working")], chairID: "custom-chair",
                         readOnly: "Archived") == "Archived")
    }

    private func tabReason(agents: [SwarmAgent], chairID: String? = nil, readOnly: String? = nil) -> String? {
        let session = SwarmSession(id: .init("chat"), talkMode: "lane", adapter: "herdr",
                                   cwd: "/fixture/workspace", createdAt: 1,
                                   chairID: chairID.map(SwarmChairID.init), chairLog: nil,
                                   agents: agents.count, messages: 0, lastMessageAt: nil)
        return ModelSwitchChoice.tabDisabledReason(readOnlyReason: readOnly,
            chat: SwarmProjectSession(sessions: [session], title: "Work", isRunning: true), agents: agents)
    }

    private func agent(_ id: String, state: String, alive: Bool = true) -> SwarmAgent {
        SwarmAgent(id: .init(id), role: "work", pane: "pane", alive: alive, state: state)
    }

    @Test("Composer and tab menu share the model switch reason")
    func disabledReason() {
        func reason(readOnly: String? = nil, waiting: Bool = false, sending: Bool = false,
                    running: Bool = false) -> String? {
            ModelSwitchChoice.disabledReason(readOnlyReason: readOnly, waitingForModel: waiting,
                                             isSending: sending, isRunning: running)
        }
        #expect(reason() == nil)
        #expect(reason(readOnly: "Archived", waiting: true, sending: true, running: true) == "Archived")
        #expect(reason(waiting: true, sending: true, running: true)
            == "Waiting for this chat's model information.")
        let busy = "Wait for the reply to finish, or stop it before switching model."
        #expect(reason(sending: true) == busy)
        #expect(reason(running: true) == busy)
    }

    @Test("The chat's own provider and model leave Switch disabled")
    func samePick() {
        #expect(!codexChat.canSwitch(provider: "codex", model: "gpt-5.5", isSwitching: false))
    }

    @Test("A different model is ready before the model and provider lists come in")
    func differentModelWhileLoading() {
        let provider = codexChat.initialProvider(offered: ModelSwitchChoice.offered(nil))
        #expect(provider == "codex")
        #expect(codexChat.initialModel(provider: provider, models: []) == "gpt-5.5")
        #expect(codexChat.canSwitch(provider: "codex", model: "gpt-5.5-mini", isSwitching: false))
        #expect(codexChat.canSwitch(provider: "claude", model: "gpt-5.5", isSwitching: false))
    }

    @Test("An invalid model name leaves Switch disabled")
    func invalidModel() {
        #expect(!codexChat.canSwitch(provider: "claude", model: "", isSwitching: false))
        #expect(!codexChat.canSwitch(provider: "claude", model: "--opus", isSwitching: false))
    }

    @Test("A running switch leaves Switch disabled")
    func switchRunning() {
        #expect(!codexChat.canSwitch(provider: "claude", model: "opus", isSwitching: true))
    }

    @Test("Another provider opens on its first listed model")
    func otherProviderModel() {
        let models = [SwarmModel(id: "opus", label: "Opus")]
        #expect(codexChat.initialModel(provider: "claude", models: models) == "opus")
        #expect(codexChat.initialModel(provider: "claude", models: []) == "")
    }
}
