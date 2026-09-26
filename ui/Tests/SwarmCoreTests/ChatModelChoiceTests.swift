import Foundation
import Testing
import TranscriptTool
@testable import SwarmCore

@Suite("Chat model selection")
struct ChatModelChoiceTests {
    @Test("The current model wins even when it is absent from the catalog")
    func currentModel() {
        let catalog = [SwarmModel(id: "sonnet", label: "Sonnet")]
        #expect(ChatModelChoice.initial(current: "claude-opus-4-6", saved: "sonnet", models: catalog)
            == "claude-opus-4-6")
        #expect(ChatModelChoice.initial(current: nil, saved: "opus", models: catalog) == "opus")
        #expect(ChatModelChoice.initial(current: nil, saved: nil, models: catalog) == "sonnet")
        #expect(ChatModelChoice.initial(current: "", saved: nil, models: []) == "")
    }

    @Test("Model choices survive reopening and remain separate for each provider")
    @MainActor func preferences() throws {
        let suite = "swarm-model-test-\(UUID().uuidString)"
        let defaults = try #require(UserDefaults(suiteName: suite))
        defer { defaults.removePersistentDomain(forName: suite) }
        let preferences = ChatModelPreferences(defaults: defaults)
        preferences.remember(provider: "claude", model: "opus")
        preferences.remember(provider: "codex", model: "gpt-6-astra")
        let reopened = ChatModelPreferences(defaults: defaults)
        #expect(reopened.model(for: "claude") == "opus")
        #expect(reopened.model(for: "codex") == "gpt-6-astra")
        #expect(reopened.provider == "codex")
        reopened.remember(provider: "claude", model: "--bad")
        #expect(reopened.model(for: "claude") == "opus")
    }

    @Test("Only reported models set the active label")
    func reportedModel() {
        let records = [
            TranscriptRecord(event: .sessionInfo(kind: "model", value: "old", meta: Meta()), rawLine: ""),
            TranscriptRecord(event: .sessionInfo(kind: "model", value: "new", meta: Meta()), rawLine: ""),
            TranscriptRecord(event: .sessionInfo(kind: "model", value: "<synthetic>", meta: Meta()), rawLine: ""),
            TranscriptRecord(event: .agentMessageChunk(text: "model: fake", meta: Meta()), rawLine: ""),
        ]
        #expect(ChatModelChoice.latest(in: records) == "new")
        #expect(ChatModelChoice.latest(in: []) == nil)
    }
}
