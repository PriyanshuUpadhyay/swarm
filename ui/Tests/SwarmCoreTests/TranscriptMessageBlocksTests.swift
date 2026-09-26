import Foundation
import Testing
@testable import SwarmCore

struct TranscriptMessageBlocksTests {
    @Test func emptyInputProducesNoBlocks() {
        #expect(TranscriptMessageBlocks.parse("").isEmpty)
        #expect(TranscriptMessageBlocks.parse("   \n\n\t  ").isEmpty)
    }

    @Test func singleAndMultipleParagraphs() {
        let single = TranscriptMessageBlocks.parse("Hello, this is a plain paragraph.")
        #expect(single.count == 1)
        if case let .paragraph(id, text) = single.first {
            #expect(id == "block-0")
            #expect(text == "Hello, this is a plain paragraph.")
        } else {
            Issue.record("Expected paragraph block")
        }

        let multi = TranscriptMessageBlocks.parse("Paragraph one.\n\nParagraph two.\n\nParagraph three.")
        #expect(multi.count == 3)
        #expect(multi == [
            .paragraph(id: "block-0", text: "Paragraph one."),
            .paragraph(id: "block-1", text: "Paragraph two."),
            .paragraph(id: "block-2", text: "Paragraph three."),
        ])
    }

    @Test func headingsLevels() {
        let text = """
        # Heading 1
        ## Heading 2
        ### Heading 3
        #### Heading 4
        ##### Heading 5
        ###### Heading 6
        #tag Not a heading
        """
        let blocks = TranscriptMessageBlocks.parse(text)
        #expect(blocks.count == 7)

        for i in 1...6 {
            if case let .heading(_, level, headingText) = blocks[i - 1] {
                #expect(level == i)
                #expect(headingText == "Heading \(i)")
            } else {
                Issue.record("Expected heading level \(i)")
            }
        }

        if case let .paragraph(_, pText) = blocks[6] {
            #expect(pText == "#tag Not a heading")
        } else {
            Issue.record("Expected hashtag to be parsed as paragraph")
        }
    }

    @Test func fencedCodeBlocks() {
        let text = """
        ```swift
        func test() {
            let value = 42
            print(value)
        }
        ```
        """
        let blocks = TranscriptMessageBlocks.parse(text)
        #expect(blocks.count == 1)
        if case let .codeBlock(_, language, code) = blocks.first {
            #expect(language == "swift")
            #expect(code == "func test() {\n    let value = 42\n    print(value)\n}")
        } else {
            Issue.record("Expected code block")
        }

        let tildeText = """
        ~~~bash
        echo "hello"
        ~~~
        """
        let tildeBlocks = TranscriptMessageBlocks.parse(tildeText)
        #expect(tildeBlocks.count == 1)
        if case let .codeBlock(_, language, code) = tildeBlocks.first {
            #expect(language == "bash")
            #expect(code == "echo \"hello\"")
        } else {
            Issue.record("Expected tilde code block")
        }
    }

    @Test func streamingIncompleteFences() {
        let stream = """
        Here is the code:
        ```python
        def greet():
            return "streaming..."
        """
        let blocks = TranscriptMessageBlocks.parse(stream)
        #expect(blocks.count == 2)
        if case let .paragraph(_, text) = blocks[0] {
            #expect(text == "Here is the code:")
        } else {
            Issue.record("Expected leading paragraph")
        }
        if case let .codeBlock(_, language, code) = blocks[1] {
            #expect(language == "python")
            #expect(code == "def greet():\n    return \"streaming...\"")
        } else {
            Issue.record("Expected incomplete streaming code block")
        }
    }

    @Test func blockquotes() {
        let text = """
        > This is a quote.
        > It spans multiple lines.
        >
        > And keeps paragraphs.
        """
        let blocks = TranscriptMessageBlocks.parse(text)
        #expect(blocks.count == 1)
        if case let .blockquote(_, quote) = blocks.first {
            #expect(quote == "This is a quote.\nIt spans multiple lines.\n\nAnd keeps paragraphs.")
        } else {
            Issue.record("Expected blockquote")
        }
    }

    @Test func unorderedLists() {
        let text = """
        - Dash item 1
        - Dash item 2
          with continuation line
        * Star item
        + Plus item
        """
        let blocks = TranscriptMessageBlocks.parse(text)
        #expect(blocks.count == 1)
        if case let .unorderedList(_, items) = blocks.first {
            #expect(items.count == 4)
            #expect(items[0] == "Dash item 1")
            #expect(items[1] == "Dash item 2 with continuation line")
            #expect(items[2] == "Star item")
            #expect(items[3] == "Plus item")
        } else {
            Issue.record("Expected unordered list")
        }
    }

    @Test func orderedLists() {
        let text = """
        3. Step three
        4. Step four
           with extra detail
        5. Step five
        """
        let blocks = TranscriptMessageBlocks.parse(text)
        #expect(blocks.count == 1)
        if case let .orderedList(_, startIndex, items) = blocks.first {
            #expect(startIndex == 3)
            #expect(items.count == 3)
            #expect(items[0] == "Step three")
            #expect(items[1] == "Step four with extra detail")
            #expect(items[2] == "Step five")
        } else {
            Issue.record("Expected ordered list")
        }
    }

    @Test func markdownTable() {
        let text = """
        | Header A | Header B | Header C |
        | :--- | :---: | ---: |
        | Cell 1 | Cell 2 | Cell 3 |
        | Cell 4 | Cell 5 |
        """
        let blocks = TranscriptMessageBlocks.parse(text)
        #expect(blocks.count == 1)
        if case let .table(_, headers, rows, _) = blocks.first {
            #expect(headers == ["Header A", "Header B", "Header C"])
            #expect(rows.count == 2)
            #expect(rows[0] == ["Cell 1", "Cell 2", "Cell 3"])
            #expect(rows[1] == ["Cell 4", "Cell 5", ""]) // missing cell padded
        } else {
            Issue.record("Expected table block")
        }
    }

    @Test func rawMonospaceTable() {
        let text = """
        +------+------+
        | Col1 | Col2 |
        +------+------+
        | Val1 | Val2 |
        +------+------+
        """
        let blocks = TranscriptMessageBlocks.parse(text)
        #expect(blocks.count == 1)
        if case let .rawMonospace(_, monoText) = blocks.first {
            #expect(monoText.contains("+------+------+"))
            #expect(monoText.contains("| Col1 | Col2 |"))
        } else {
            Issue.record("Expected raw monospace table block")
        }
    }

    @Test func horizontalDivider() {
        let text = """
        Above divider

        ---

        Below divider
        """
        let blocks = TranscriptMessageBlocks.parse(text)
        #expect(blocks.count == 3)
        if case .divider = blocks[1] {
            // Success
        } else {
            Issue.record("Expected divider block")
        }
    }

    @Test func inlineMarkdownSafeLinks() {
        let text = "Check [website](https://example.com) and [api](http://localhost:8080)."
        let attr = TranscriptMessageBlocks.parseInlineMarkdown(text)

        var links: [URL] = []
        for run in attr.runs {
            if let link = run.link {
                links.append(link)
            }
        }
        #expect(links.count == 2)
        #expect(links.contains(URL(string: "https://example.com")!))
        #expect(links.contains(URL(string: "http://localhost:8080")!))
    }

    @Test func inlineMarkdownStripsUnsafeLinks() {
        let text = "[evil js](javascript:alert(1)) and [file](file:///etc/passwd) and [data](data:text/html,test)"
        let attr = TranscriptMessageBlocks.parseInlineMarkdown(text)

        for run in attr.runs {
            #expect(run.link == nil, "Unsafe links must be stripped from AttributedString")
        }
    }

    @Test func inlineMarkdownStripsImages() {
        let text = "Text with ![secret](https://tracker.com/pixel.png) image."
        let attr = TranscriptMessageBlocks.parseInlineMarkdown(text)

        for run in attr.runs {
            #expect(run.imageURL == nil, "Images must have imageURL stripped to prevent network fetching")
        }
    }

    @Test func unicodeHandling() {
        let text = """
        # 🚀 Welcome 🤖
        Multi-byte text: 漢字, 한국어, русский, café, naïve.
        """
        let blocks = TranscriptMessageBlocks.parse(text)
        #expect(blocks.count == 2)
        if case let .heading(_, _, title) = blocks[0] {
            #expect(title == "🚀 Welcome 🤖")
        } else {
            Issue.record("Expected unicode heading")
        }
        if case let .paragraph(_, para) = blocks[1] {
            #expect(para.contains("漢字"))
            #expect(para.contains("café"))
        } else {
            Issue.record("Expected unicode paragraph")
        }
    }

    @Test func orderedListDigitLimits() {
        let valid = "999999999. Valid 9-digit marker"
        let validBlocks = TranscriptMessageBlocks.parse(valid)
        #expect(validBlocks.count == 1)
        if case let .orderedList(_, start, items) = validBlocks.first {
            #expect(start == 999_999_999)
            #expect(items == ["Valid 9-digit marker"])
        } else {
            Issue.record("Expected 9-digit ordered list")
        }

        let overflowText = "9223372036854775807. Int.max marker"
        let overflowBlocks = TranscriptMessageBlocks.parse(overflowText)
        #expect(overflowBlocks.count == 1)
        if case let .paragraph(_, text) = overflowBlocks.first {
            #expect(text == overflowText)
        } else {
            Issue.record("Expected >9 digits to fall back to plain text paragraph")
        }
    }

    @Test func fencedCodeBlockLongerClosingFence() {
        let text = """
        ```swift
        let x = 1
        ````
        """
        let blocks = TranscriptMessageBlocks.parse(text)
        #expect(blocks.count == 1)
        if case let .codeBlock(_, lang, code) = blocks.first {
            #expect(lang == "swift")
            #expect(code == "let x = 1")
        } else {
            Issue.record("Expected longer closing fence to close code block")
        }
    }

    @Test func tableWithComplexPipesRetainedAsRawMonospace() {
        let escapedPipeTable = """
        | Option | Syntax |
        | --- | --- |
        | Choice A | a \\| b |
        """
        let escapedBlocks = TranscriptMessageBlocks.parse(escapedPipeTable)
        #expect(escapedBlocks.count == 1)
        if case let .rawMonospace(_, rawText) = escapedBlocks.first {
            #expect(rawText == escapedPipeTable)
        } else {
            Issue.record("Expected table with escaped pipe to be retained intact as rawMonospace")
        }

        let codePipeTable = """
        | Function | Usage |
        | --- | --- |
        | Bitwise | `a | b` |
        """
        let codePipeBlocks = TranscriptMessageBlocks.parse(codePipeTable)
        #expect(codePipeBlocks.count == 1)
        if case let .rawMonospace(_, rawText) = codePipeBlocks.first {
            #expect(rawText == codePipeTable)
        } else {
            Issue.record("Expected table with inline code pipe to be retained intact as rawMonospace")
        }
    }

    @Test func stableBlockIdentifiers() {
        let text = "Line 1\n\nLine 2\n\nLine 3"
        let blocks = TranscriptMessageBlocks.parse(text, idPrefix: "test-msg")
        #expect(blocks.map(\.id) == ["test-msg-0", "test-msg-1", "test-msg-2"])
    }
}
