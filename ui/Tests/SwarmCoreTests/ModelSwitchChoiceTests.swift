import Testing
@testable import SwarmCore

@Suite("Switch model readiness")
struct ModelSwitchChoiceTests {
    let codexChat = ModelSwitchChoice(currentProvider: "codex", currentModel: "gpt-5.5")

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
