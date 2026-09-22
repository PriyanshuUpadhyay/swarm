import Testing
import TranscriptTool
@testable import SwarmCore

@Suite("Transcript rows")
struct TranscriptRowBuilderTests {
    @Test("Agent chunks with one UUID form one row")
    func agentChunksFold() {
        let first = TranscriptEvent.agentMessageChunk(
            text: "Hello ", meta: Meta(agentSessionID: "s1", uuid: "turn-1", timestamp: "t1")
        )
        let second = TranscriptEvent.agentMessageChunk(
            text: "world!", meta: Meta(agentSessionID: "s1", uuid: "turn-1", timestamp: "t2")
        )
        let rows = TranscriptRowBuilder.rows(from: [first, second])
        #expect(rows == [TranscriptRow(kind: .assistant, text: "Hello world!", eventID: "turn-1")])
    }

    @Test("A different UUID starts a new row")
    func distinctChunksStayApart() {
        let first = TranscriptEvent.agentMessageChunk(text: "A", meta: Meta(uuid: "one"))
        let second = TranscriptEvent.agentMessageChunk(text: "B", meta: Meta(uuid: "two"))
        #expect(TranscriptRowBuilder.rows(from: [first, second]).map(\.text) == ["A", "B"])
    }
}
