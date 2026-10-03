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

    @Test func shellOutputFoldsAtFiftyLines() {
        let fifty = (0..<50).map { "line \($0)" }.joined(separator: "\n")
        #expect(!TranscriptTextPreview(fifty, lineLimit: 50).isTruncated)
        let preview = TranscriptTextPreview(fifty + "\nline 50", lineLimit: 50)
        #expect(preview.text == fifty)
        #expect(preview.isTruncated)
    }

    @Test func toolOutputFoldsAtFiveLines() {
        let preview = TranscriptTextPreview("one\ntwo\nthree\nfour\nfive\nsix", lineLimit: 5)
        #expect(preview.text == "one\ntwo\nthree\nfour\nfive")
        #expect(preview.isTruncated)
        #expect(!TranscriptTextPreview("one\ntwo\nthree\nfour\nfive", lineLimit: 5).isTruncated)
    }

    @Test func chunksKeepOrdinaryLinesTogether() {
        let chunks = TranscriptTextChunks("first\nsecond\nthird\n", limit: 10)
        #expect(chunks.pieces == ["first\n", "second\n", "third\n"])
    }

    @Test func chunksKeepEveryCharacterInOrder() {
        let source = "αβγ\n\n" + String(repeating: "👩🏽‍💻", count: 17) + "\nlast\n"
        let chunks = TranscriptTextChunks(source, limit: 5)
        #expect(chunks.pieces.joined() == source)
        #expect(chunks.pieces.allSatisfy { $0.count <= 5 })
        #expect(TranscriptTextChunks("", limit: 5).pieces.isEmpty)
    }
}
