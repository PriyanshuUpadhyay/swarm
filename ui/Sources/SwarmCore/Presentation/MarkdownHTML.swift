import Foundation

/// Markdown written out as HTML, for the file preview that draws it in WebKit.
///
/// It goes through `MarkdownParser`, the transcript's parser, rather than through a second one,
/// so a table or a task list reads the same in an agent's answer and in the file that agent wrote.
/// What that parser does not know stays literal: a `<details>` typed into a README is escaped and
/// shown as text rather than passed through. That is a choice and not an omission. The page this
/// lands in can read the worktree, so Markdown gets no route to running markup of its own; an
/// HTML file that means to run is previewed as HTML, where that is what was asked for.
public enum MarkdownHTML {
    /// The body of the page, and whether any fence in it is a Mermaid diagram, which is what
    /// decides whether the page loads the script that draws one.
    public struct Rendered: Sendable, Equatable {
        public var html: String
        public var hasMermaid: Bool
    }

    public static func render(_ text: String) -> Rendered {
        var writer = Writer()
        writer.blocks(MarkdownParser.parse(text))
        return Rendered(html: writer.output, hasMermaid: writer.hasMermaid)
    }

    /// Escaped for text and for a double quoted attribute alike, so one function serves both.
    public static func escape(_ text: String) -> String {
        var output = ""
        output.reserveCapacity(text.utf8.count)
        for character in text {
            switch character {
            case "&": output += "&amp;"
            case "<": output += "&lt;"
            case ">": output += "&gt;"
            case "\"": output += "&quot;"
            case "'": output += "&#39;"
            default: output.append(character)
            }
        }
        return output
    }

    /// The address a link or an image may carry, or nil for one it may not.
    ///
    /// A relative address and a fragment are the point of a preview, since they are how a plan
    /// reaches its diagram and its own headings. Of the schemes, the web and mail are allowed and
    /// `data:` only for an image. `javascript:` is the reason this exists, and anything else
    /// (`file:`, some other application's private scheme) has no business in a document either.
    public static func safeAddress(_ address: String, forImage: Bool = false) -> String? {
        let trimmed = address.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return nil }
        guard let colon = trimmed.firstIndex(of: ":") else { return trimmed }
        let scheme = trimmed[..<colon]
        // A colon after a slash, a question mark or a hash belongs to the path, the query or the
        // fragment of a relative address, not to a scheme.
        guard scheme.allSatisfy({ $0.isLetter || $0.isNumber || $0 == "+" || $0 == "-" || $0 == "." }) else {
            return trimmed
        }
        switch scheme.lowercased() {
        case "http", "https", "mailto": return trimmed
        case "data": return forImage && trimmed.lowercased().hasPrefix("data:image/") ? trimmed : nil
        default: return nil
        }
    }

    /// GitHub's anchor for a heading, so `[see costs](#costs-per-month)` written for GitHub lands
    /// on the same heading here.
    public static func slug(_ text: String) -> String {
        var slug = ""
        for character in text.lowercased() {
            if character.isLetter || character.isNumber || character == "-" || character == "_" {
                slug.append(character)
            } else if character == " " {
                slug.append("-")
            }
        }
        return slug
    }

    private struct Writer {
        var output = ""
        var hasMermaid = false
        var slugs: [String: Int] = [:]

        mutating func blocks(_ blocks: [MarkdownBlock], tight: Bool = false) {
            for block in blocks { self.block(block, tight: tight) }
        }

        mutating func block(_ block: MarkdownBlock, tight: Bool) {
            switch block {
            case let .paragraph(inline):
                if tight {
                    output += MarkdownHTML.inline(inline)
                } else {
                    output += "<p>\(MarkdownHTML.inline(inline))</p>\n"
                }
            case let .heading(level, inline):
                let level = min(max(level, 1), 6)
                output += "<h\(level) id=\"\(MarkdownHTML.escape(anchor(for: inline)))\">\(MarkdownHTML.inline(inline))</h\(level)>\n"
            case let .codeBlock(code, language, info):
                codeBlock(code, language: language, info: info)
            case let .bulletList(items, isTight):
                output += "<ul>\n"
                listItems(items, tight: isTight)
                output += "</ul>\n"
            case let .numberedList(start, items, isTight):
                output += start == 1 ? "<ol>\n" : "<ol start=\"\(start)\">\n"
                listItems(items, tight: isTight)
                output += "</ol>\n"
            case let .taskList(items):
                output += "<ul class=\"task-list\">\n"
                for item in items {
                    let checked = item.checked ? " checked" : ""
                    output += "<li class=\"task-list-item\"><input type=\"checkbox\" disabled\(checked)> "
                    output += MarkdownHTML.inline(item.inline)
                    output += "</li>\n"
                }
                output += "</ul>\n"
            case let .blockQuote(children):
                output += "<blockquote>\n"
                blocks(children)
                output += "</blockquote>\n"
            case let .table(headers, rows, alignments):
                table(headers: headers, rows: rows, alignments: alignments)
            case .thematicBreak:
                output += "<hr>\n"
            }
        }

        mutating func listItems(_ items: [[MarkdownBlock]], tight: Bool) {
            for item in items {
                output += "<li>"
                // A tight item's first paragraph sits in the `li` itself, and anything after it,
                // a nested list most often, starts on its own line as it would in a browser.
                for (offset, child) in item.enumerated() {
                    if tight, offset > 0, case .paragraph = child { output += "<br>" }
                    block(child, tight: tight)
                }
                output += "</li>\n"
            }
        }

        mutating func codeBlock(_ code: String, language: Language, info: String) {
            let tag = info.split(whereSeparator: \.isWhitespace).first.map { $0.lowercased() } ?? ""
            if tag == "mermaid" {
                hasMermaid = true
                output += "<pre class=\"mermaid\">\(MarkdownHTML.escape(code))</pre>\n"
                return
            }
            let attribute = tag.isEmpty ? "" : " class=\"language-\(MarkdownHTML.escape(tag))\""
            output += "<pre><code\(attribute)>\(MarkdownHTML.highlighted(code, language: language))</code></pre>\n"
        }

        mutating func table(headers: [[MarkdownInline]], rows: [[[MarkdownInline]]], alignments: [TableAlignment]) {
            func style(_ column: Int) -> String {
                guard alignments.indices.contains(column) else { return "" }
                return switch alignments[column] {
                case .leading: ""
                case .center: " style=\"text-align: center\""
                case .trailing: " style=\"text-align: right\""
                }
            }
            output += "<table>\n<thead><tr>"
            for (column, header) in headers.enumerated() {
                output += "<th\(style(column))>\(MarkdownHTML.inline(header))</th>"
            }
            output += "</tr></thead>\n<tbody>\n"
            for row in rows {
                output += "<tr>"
                for column in headers.indices {
                    let cell = row.indices.contains(column) ? row[column] : []
                    output += "<td\(style(column))>\(MarkdownHTML.inline(cell))</td>"
                }
                output += "</tr>\n"
            }
            output += "</tbody>\n</table>\n"
        }

        /// Repeated headings get `-1`, `-2` after the first, which is GitHub's rule too.
        mutating func anchor(for inline: [MarkdownInline]) -> String {
            let base = MarkdownHTML.slug(MarkdownHTML.plainText(inline))
            let count = slugs[base, default: 0]
            slugs[base] = count + 1
            return count == 0 ? base : "\(base)-\(count)"
        }
    }

    static func inline(_ values: [MarkdownInline]) -> String {
        var output = ""
        for value in values {
            switch value {
            case let .text(text):
                output += escape(text)
            case let .emphasis(children):
                output += "<em>\(inline(children))</em>"
            case let .strong(children):
                output += "<strong>\(inline(children))</strong>"
            case let .strikethrough(children):
                output += "<del>\(inline(children))</del>"
            case let .code(text):
                output += "<code>\(escape(text))</code>"
            case let .link(text, url):
                if let address = safeAddress(url) {
                    output += "<a href=\"\(escape(address))\">\(inline(text))</a>"
                } else {
                    output += inline(text)
                }
            case let .image(alt, url):
                if let address = safeAddress(url, forImage: true) {
                    output += "<img src=\"\(escape(address))\" alt=\"\(escape(alt))\">"
                } else {
                    output += escape(alt)
                }
            case .lineBreak:
                output += "<br>\n"
            }
        }
        return output
    }

    static func plainText(_ values: [MarkdownInline]) -> String {
        values.map { value in
            switch value {
            case let .text(text), let .code(text): text
            case let .emphasis(children), let .strong(children), let .strikethrough(children): plainText(children)
            case let .link(text, _): plainText(text)
            case let .image(alt, _): alt
            case .lineBreak: " "
            }
        }.joined()
    }

    /// Classes named after `TokenKind`, which the page's stylesheet colours.
    static func highlighted(_ code: String, language: Language) -> String {
        guard language != .plainText else { return escape(code) }
        let lines = code.components(separatedBy: "\n")
        let tokens = SyntaxHighlighter.tokenize(source: code, language: language)
        var output = ""
        for (index, line) in lines.enumerated() {
            if index > 0 { output += "\n" }
            let utf16 = Array(line.utf16)
            var cursor = 0
            for token in tokens.indices.contains(index) ? tokens[index] : [] {
                let lower = max(token.range.lowerBound, cursor)
                let upper = min(token.range.upperBound, utf16.count)
                guard lower < upper else { continue }
                if lower > cursor { output += escape(String(decoding: utf16[cursor..<lower], as: UTF16.self)) }
                let piece = escape(String(decoding: utf16[lower..<upper], as: UTF16.self))
                output += token.kind == .plain ? piece : "<span class=\"tok-\(token.kind.rawValue)\">\(piece)</span>"
                cursor = upper
            }
            if cursor < utf16.count { output += escape(String(decoding: utf16[cursor...], as: UTF16.self)) }
        }
        return output
    }
}
