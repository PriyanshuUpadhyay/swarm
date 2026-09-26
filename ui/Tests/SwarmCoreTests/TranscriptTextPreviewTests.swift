import Testing
@testable import SwarmCore

struct TranscriptTextPreviewTests {
    @Test func preservesSmallOutput() {
        let text = "first\n\n  indented λ\n"
        #expect(TranscriptTextPreview(text).text == text)
        #expect(!TranscriptTextPreview(text).isTruncated)
        #expect(!TranscriptTextPreview("").isTruncated)
    }

    @Test func boundsLongAndMultilineOutput() {
        let longLine = String(repeating: "👩🏽‍💻", count: 12_001)
        let longPreview = TranscriptTextPreview(longLine)
        #expect(longPreview.text == String(longLine.prefix(12_000)))
        #expect(longPreview.isTruncated)
        let manyLines = (0..<121).map { "line \($0)" }.joined(separator: "\n")
        let preview = TranscriptTextPreview(manyLines)
        #expect(preview.text.hasSuffix("line 119"))
        #expect(preview.isTruncated)
    }
}
