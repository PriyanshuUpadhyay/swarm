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
        #expect(preview.hiddenLineCount == 1)
        #expect(!TranscriptTextPreview("one\ntwo\nthree\nfour\nfive", lineLimit: 5).isTruncated)
        #expect(TranscriptTextPreview(String(repeating: "x", count: 12_001), lineLimit: 5).hiddenLineCount == 0)
    }

    @Test func trailingNewlineEndsTheLastLine() {
        let fiveLines = "a\nb\nc\nd\ne\n"
        let preview = TranscriptTextPreview(fiveLines, lineLimit: 5)
        #expect(preview.text == fiveLines)
        #expect(!preview.isTruncated)
        #expect(preview.lineCount == 5)
        #expect(preview.hiddenLineCount == 0)
        #expect(TranscriptTextPreview("\n").lineCount == 1)
    }

    @Test func characterCapCountsEveryLineItHides() {
        let longFirstLine = String(repeating: "x", count: 20_000)
        let shortLines = (0..<9).map { "line \($0)" }
        let capped = TranscriptTextPreview(([longFirstLine] + shortLines).joined(separator: "\n"), lineLimit: 5)
        #expect(capped.text == String(longFirstLine.prefix(12_000)))
        #expect(capped.hiddenLineCount == 9)
        let wideLine = String(repeating: "w", count: 290)
        let wideLines = TranscriptTextPreview(Array(repeating: wideLine, count: 60).joined(separator: "\n"), lineLimit: 50)
        // 12,000 characters hold 41 lines of 291 (with the "\n") and the start of the 42nd.
        #expect(wideLines.hiddenLineCount == 18)
    }

    @Test func windowsLineEndsCountAndFoldAsLines() {
        let preview = TranscriptTextPreview("one\r\ntwo\r\nthree", lineLimit: 2)
        #expect(preview.text == "one\r\ntwo\r")
        #expect(preview.isTruncated)
        #expect(preview.lineCount == 3)
        #expect(preview.hiddenLineCount == 1)
        #expect(TranscriptTextPreview("").lineCount == 1)
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
