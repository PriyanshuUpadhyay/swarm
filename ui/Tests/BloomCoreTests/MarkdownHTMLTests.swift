import Foundation
import Testing
@testable import BloomCore

@Suite("Markdown as HTML")
struct MarkdownHTMLTests {
    @Test("headings carry GitHub's anchors, with repeats numbered")
    func headingAnchors() {
        let html = MarkdownHTML.render("# Costs per month\n\n## Costs per month").html
        #expect(html.contains("<h1 id=\"costs-per-month\">Costs per month</h1>"))
        #expect(html.contains("<h2 id=\"costs-per-month-1\">Costs per month</h2>"))
    }

    @Test("a table keeps its column alignment")
    func table() {
        let html = MarkdownHTML.render("| Plan | Price |\n| :-- | --: |\n| Pro | $9 |").html
        #expect(html.contains("<th>Plan</th><th style=\"text-align: right\">Price</th>"))
        #expect(html.contains("<td>Pro</td><td style=\"text-align: right\">$9</td>"))
    }

    @Test("a task list is drawn as disabled checkboxes")
    func taskList() {
        let html = MarkdownHTML.render("- [x] Parse\n- [ ] Render").html
        #expect(html.contains("<input type=\"checkbox\" disabled checked> Parse"))
        #expect(html.contains("<input type=\"checkbox\" disabled> Render"))
    }

    @Test("an image keeps its relative address and its alternative text")
    func image() {
        let html = MarkdownHTML.render("![The flow](diagrams/flow.png \"Flow\")").html
        #expect(html.contains("<img src=\"diagrams/flow.png\" alt=\"The flow\">"))
    }

    @Test("a Mermaid fence becomes a diagram and says the page needs the script")
    func mermaid() {
        let rendered = MarkdownHTML.render("```mermaid\ngraph TD\n  A --> B\n```")
        #expect(rendered.hasMermaid)
        #expect(rendered.html.contains("<pre class=\"mermaid\">graph TD\n  A --&gt; B</pre>"))
        #expect(!MarkdownHTML.render("```swift\nlet a = 1\n```").hasMermaid)
    }

    @Test("a code block is highlighted and escaped")
    func codeBlock() {
        let html = MarkdownHTML.render("```swift\nlet a = \"<b>\"\n```").html
        #expect(html.contains("class=\"language-swift\""))
        #expect(html.contains("<span class=\"tok-keyword\">let</span>"))
        #expect(html.contains("&lt;b&gt;"))
        #expect(!html.contains("<b>"))
    }

    @Test("markup typed into the document is text, not markup")
    func rawMarkupIsEscaped() {
        let html = MarkdownHTML.render("<script>alert(1)</script> and <img src=x onerror=alert(1)>").html
        #expect(!html.contains("<script>"))
        #expect(!html.contains("<img"))
        #expect(html.contains("&lt;script&gt;"))
    }

    @Test("a javascript address is dropped and its text kept")
    func unsafeLinks() {
        let html = MarkdownHTML.render("[click](javascript:alert(1)) ![x](file:///etc/passwd)").html
        #expect(!html.contains("javascript:"))
        #expect(!html.contains("file://"))
        #expect(html.contains("click"))
    }

    @Test("relative, fragment, web and mail addresses are kept")
    func safeAddresses() {
        #expect(MarkdownHTML.safeAddress("docs/plan.md#costs") == "docs/plan.md#costs")
        #expect(MarkdownHTML.safeAddress("#costs") == "#costs")
        #expect(MarkdownHTML.safeAddress("https://spatie.be") == "https://spatie.be")
        #expect(MarkdownHTML.safeAddress("mailto:freek@spatie.be") == "mailto:freek@spatie.be")
        #expect(MarkdownHTML.safeAddress("notes/a:b.md") == "notes/a:b.md")
        #expect(MarkdownHTML.safeAddress("data:image/png;base64,AAAA", forImage: true) != nil)
        #expect(MarkdownHTML.safeAddress("data:text/html,<script>", forImage: true) == nil)
        #expect(MarkdownHTML.safeAddress("data:image/png;base64,AAAA") == nil)
        #expect(MarkdownHTML.safeAddress("JavaScript:alert(1)") == nil)
    }

    @Test("the page loads Mermaid only when a diagram needs it, pinned by digest")
    func page() {
        let plain = MarkdownPreviewPage.html(for: MarkdownFileDocument(text: "# Plan", isTruncated: false), title: "plan.md")
        #expect(!plain.contains("mermaid.min.js"))
        let diagram = MarkdownPreviewPage.html(
            for: MarkdownFileDocument(text: "```mermaid\ngraph TD\n```", isTruncated: false), title: "plan.md"
        )
        #expect(diagram.contains(MarkdownPreviewPage.mermaidScript))
        #expect(diagram.contains("integrity=\"\(MarkdownPreviewPage.mermaidIntegrity)\""))
    }
}
