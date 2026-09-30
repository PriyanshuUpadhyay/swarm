import Foundation
import Testing
import TranscriptTool
@testable import SwarmCore

@Suite("Chat model selection")
struct ChatModelChoiceTests {
    @Test("The current model wins even when it is absent from the catalog")
    func currentModel() {
        let catalog = [SwarmModel(id: "sonnet", label: "Sonnet")]
        #expect(ChatModelChoice.initial(current: "claude-opus-4-6", models: catalog) == "claude-opus-4-6")
        #expect(ChatModelChoice.initial(current: "--bad", models: catalog) == "sonnet")
        #expect(ChatModelChoice.initial(current: nil, models: catalog) == "sonnet")
        #expect(ChatModelChoice.initial(current: "", models: []) == "")
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
