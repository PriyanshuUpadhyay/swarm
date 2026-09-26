import Foundation

public enum TranscriptMessageBlock: Sendable, Hashable, Identifiable {
    case paragraph(id: String, text: String)
    case heading(id: String, level: Int, text: String)
    case codeBlock(id: String, language: String?, code: String)
    case blockquote(id: String, text: String)
    case unorderedList(id: String, items: [String])
    case orderedList(id: String, startIndex: Int, items: [String])
    case table(id: String, headers: [String], rows: [[String]], rawText: String)
    case rawMonospace(id: String, text: String)
    case divider(id: String)

    public var id: String {
        switch self {
        case let .paragraph(id, _): id
        case let .heading(id, _, _): id
        case let .codeBlock(id, _, _): id
        case let .blockquote(id, _): id
        case let .unorderedList(id, _): id
        case let .orderedList(id, _, _): id
        case let .table(id, _, _, _): id
        case let .rawMonospace(id, _): id
        case let .divider(id): id
        }
    }

}

public enum TranscriptMessageBlocks {
    /// Handles common message blocks; complex tables retain their source layout.
    public static func parse(_ text: String, idPrefix: String = "block") -> [TranscriptMessageBlock] {
        let trimmedInput = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmedInput.isEmpty else { return [] }

        let normalized = text
            .replacingOccurrences(of: "\r\n", with: "\n")
            .replacingOccurrences(of: "\r", with: "\n")
        let lines = normalized.split(separator: "\n", omittingEmptySubsequences: false).map(String.init)

        var blocks: [TranscriptMessageBlock] = []
        var blockIndex = 0

        func nextID() -> String {
            defer { blockIndex += 1 }
            return "\(idPrefix)-\(blockIndex)"
        }

        var i = 0
        while i < lines.count {
            let line = lines[i]
            let trimmed = line.trimmingCharacters(in: .whitespaces)

            if trimmed.isEmpty {
                i += 1
                continue
            }

            if trimmed.hasPrefix("```") || trimmed.hasPrefix("~~~") {
                let fenceChar = trimmed.first!
                let fenceCount = trimmed.prefix(while: { $0 == fenceChar }).count
                let info = String(trimmed.dropFirst(fenceCount)).trimmingCharacters(in: .whitespaces)
                let language = info.components(separatedBy: .whitespaces).first.flatMap { $0.isEmpty ? nil : $0 }

                var codeLines: [String] = []
                i += 1
                while i < lines.count {
                    let current = lines[i]
                    let currentTrimmed = current.trimmingCharacters(in: .whitespaces)
                    if currentTrimmed.hasPrefix(String(repeating: fenceChar, count: fenceCount)) {
                        let nonFence = currentTrimmed.drop(while: { $0 == fenceChar })
                        if nonFence.trimmingCharacters(in: .whitespaces).isEmpty {
                            i += 1
                            break
                        }
                    }
                    codeLines.append(current)
                    i += 1
                }
                let code = codeLines.joined(separator: "\n")
                blocks.append(.codeBlock(id: nextID(), language: language, code: code))
                continue
            }

            if trimmed.hasPrefix("#") {
                let hashCount = trimmed.prefix(while: { $0 == "#" }).count
                if hashCount >= 1 && hashCount <= 6 {
                    let remainder = trimmed.dropFirst(hashCount)
                    if remainder.isEmpty {
                        blocks.append(.heading(id: nextID(), level: hashCount, text: ""))
                        i += 1
                        continue
                    } else if remainder.hasPrefix(" ") {
                        let headingText = String(remainder.drop(while: { $0 == " " })).trimmingCharacters(in: .whitespaces)
                        blocks.append(.heading(id: nextID(), level: hashCount, text: headingText))
                        i += 1
                        continue
                    }
                }
            }

            if isDividerLine(trimmed) {
                blocks.append(.divider(id: nextID()))
                i += 1
                continue
            }

            if trimmed.hasPrefix(">") {
                var quoteLines: [String] = []
                while i < lines.count {
                    let lineTrimmed = lines[i].trimmingCharacters(in: .whitespaces)
                    guard lineTrimmed.hasPrefix(">") else { break }
                    var content = String(lineTrimmed.dropFirst())
                    if content.hasPrefix(" ") { content.removeFirst() }
                    quoteLines.append(content)
                    i += 1
                }
                blocks.append(.blockquote(id: nextID(), text: quoteLines.joined(separator: "\n")))
                continue
            }

            if trimmed.hasPrefix("+-") || trimmed.hasPrefix("+=") {
                var monoLines: [String] = []
                while i < lines.count {
                    let current = lines[i]
                    let currentTrimmed = current.trimmingCharacters(in: .whitespaces)
                    if currentTrimmed.isEmpty { break }
                    monoLines.append(current)
                    i += 1
                }
                blocks.append(.rawMonospace(id: nextID(), text: monoLines.joined(separator: "\n")))
                continue
            }

            if trimmed.contains("|") && i + 1 < lines.count && isTableDelimiter(lines[i + 1]) {
                let rawHeader = lines[i]
                let rawDelimiter = lines[i + 1]
                var rawLines = [rawHeader, rawDelimiter]
                var rows: [[String]] = []
                var hasComplexPipes = hasComplexTablePipes(rawHeader) || hasComplexTablePipes(rawDelimiter)
                i += 2
                while i < lines.count {
                    let current = lines[i]
                    let currentTrimmed = current.trimmingCharacters(in: .whitespaces)
                    if currentTrimmed.isEmpty || !currentTrimmed.contains("|") {
                        break
                    }
                    rawLines.append(current)
                    if hasComplexTablePipes(current) {
                        hasComplexPipes = true
                    }
                    rows.append(parseTableCells(current))
                    i += 1
                }

                if hasComplexPipes {
                    blocks.append(.rawMonospace(id: nextID(), text: rawLines.joined(separator: "\n")))
                } else {
                    let headers = parseTableCells(rawHeader)
                    let paddedRows = rows.map { cells in
                        var row = cells
                        if row.count < headers.count {
                            row.append(contentsOf: repeatElement("", count: headers.count - row.count))
                        }
                        return row
                    }
                    blocks.append(.table(id: nextID(), headers: headers, rows: paddedRows, rawText: rawLines.joined(separator: "\n")))
                }
                continue
            }

            if isUnorderedListMarker(line) {
                var items: [String] = []
                while i < lines.count {
                    let current = lines[i]
                    let currentTrimmed = current.trimmingCharacters(in: .whitespaces)
                    if isUnorderedListMarker(current) {
                        items.append(dropUnorderedListMarker(currentTrimmed))
                        i += 1
                    } else if !items.isEmpty && (current.hasPrefix("  ") || current.hasPrefix("\t")) && !currentTrimmed.isEmpty && !isBlockStarter(current, nextLine: i + 1 < lines.count ? lines[i + 1] : nil) {
                        let idx = items.count - 1
                        items[idx] += " " + currentTrimmed
                        i += 1
                    } else {
                        break
                    }
                }
                blocks.append(.unorderedList(id: nextID(), items: items))
                continue
            }

            if let (startNum, _) = parseOrderedListMarker(line) {
                var items: [String] = []
                let firstIndex = startNum
                while i < lines.count {
                    let current = lines[i]
                    let currentTrimmed = current.trimmingCharacters(in: .whitespaces)
                    if let (_, _) = parseOrderedListMarker(current) {
                        items.append(dropOrderedListMarker(currentTrimmed))
                        i += 1
                    } else if !items.isEmpty && (current.hasPrefix("  ") || current.hasPrefix("\t")) && !currentTrimmed.isEmpty && !isBlockStarter(current, nextLine: i + 1 < lines.count ? lines[i + 1] : nil) {
                        let idx = items.count - 1
                        items[idx] += " " + currentTrimmed
                        i += 1
                    } else {
                        break
                    }
                }
                blocks.append(.orderedList(id: nextID(), startIndex: firstIndex, items: items))
                continue
            }

            var paraLines: [String] = []
            while i < lines.count {
                let current = lines[i]
                let currentTrimmed = current.trimmingCharacters(in: .whitespaces)
                if currentTrimmed.isEmpty {
                    break
                }
                if isBlockStarter(current, nextLine: i + 1 < lines.count ? lines[i + 1] : nil) {
                    break
                }
                paraLines.append(current)
                i += 1
            }
            if !paraLines.isEmpty {
                blocks.append(.paragraph(id: nextID(), text: paraLines.joined(separator: "\n")))
            }
        }

        return blocks
    }

    /// Converts an inline markdown string to a sanitized Foundation AttributedString.
    /// Removes active links other than HTTP(S) and prevents remote image loading.
    public static func parseInlineMarkdown(_ text: String) -> AttributedString {
        guard !text.isEmpty else { return AttributedString("") }
        let options = AttributedString.MarkdownParsingOptions(
            interpretedSyntax: .inlineOnlyPreservingWhitespace,
            failurePolicy: .returnPartiallyParsedIfPossible
        )
        var attr: AttributedString
        do {
            attr = try AttributedString(markdown: text, options: options)
        } catch {
            attr = AttributedString(text)
        }

        for run in attr.runs {
            if let link = run.link {
                let scheme = link.scheme?.lowercased()
                if scheme != "http" && scheme != "https" {
                    attr[run.range].link = nil
                }
            }
            if run.imageURL != nil {
                attr[run.range].imageURL = nil
            }
        }
        return attr
    }

    // MARK: - Internal Parsing Helpers

    private static func isBlockStarter(_ line: String, nextLine: String? = nil) -> Bool {
        let trimmed = line.trimmingCharacters(in: .whitespaces)
        if trimmed.hasPrefix("```") || trimmed.hasPrefix("~~~") { return true }
        if trimmed.hasPrefix(">") { return true }
        if trimmed.hasPrefix("+-") || trimmed.hasPrefix("+=") { return true }
        if isDividerLine(trimmed) { return true }
        if isUnorderedListMarker(line) { return true }
        if parseOrderedListMarker(line) != nil { return true }
        if trimmed.hasPrefix("#") {
            let hashCount = trimmed.prefix(while: { $0 == "#" }).count
            if hashCount >= 1 && hashCount <= 6 {
                let rem = trimmed.dropFirst(hashCount)
                if rem.isEmpty || rem.hasPrefix(" ") { return true }
            }
        }
        if trimmed.contains("|"), let next = nextLine, isTableDelimiter(next) {
            return true
        }
        return false
    }

    private static func isDividerLine(_ trimmed: String) -> Bool {
        guard trimmed.count >= 3 else { return false }
        if trimmed.allSatisfy({ $0 == "-" }) || trimmed.allSatisfy({ $0 == "*" }) || trimmed.allSatisfy({ $0 == "_" }) {
            return true
        }
        return false
    }

    private static func isUnorderedListMarker(_ line: String) -> Bool {
        let trimmed = line.trimmingCharacters(in: .whitespaces)
        return trimmed.hasPrefix("- ") || trimmed.hasPrefix("* ") || trimmed.hasPrefix("+ ")
    }

    private static func dropUnorderedListMarker(_ trimmedLine: String) -> String {
        String(trimmedLine.dropFirst(2)).trimmingCharacters(in: .whitespaces)
    }

    private static func parseOrderedListMarker(_ line: String) -> (index: Int, markerLength: Int)? {
        let trimmed = line.trimmingCharacters(in: .whitespaces)
        guard let first = trimmed.first, ("0"..."9").contains(first) else { return nil }
        var digits = ""
        var idx = trimmed.startIndex
        while idx < trimmed.endIndex && ("0"..."9").contains(trimmed[idx]) {
            digits.append(trimmed[idx])
            idx = trimmed.index(after: idx)
        }
        // Limit recognized markers to 1-9 ASCII digits per CommonMark spec.
        guard digits.count >= 1 && digits.count <= 9, let number = Int(digits), idx < trimmed.endIndex else { return nil }
        let delim = trimmed[idx]
        guard delim == "." || delim == ")" else { return nil }
        idx = trimmed.index(after: idx)
        guard idx < trimmed.endIndex, trimmed[idx] == " " else { return nil }
        return (number, digits.count + 2)
    }

    private static func dropOrderedListMarker(_ trimmedLine: String) -> String {
        guard let (_, len) = parseOrderedListMarker(trimmedLine) else { return trimmedLine }
        return String(trimmedLine.dropFirst(len)).trimmingCharacters(in: .whitespaces)
    }

    private static func isTableDelimiter(_ line: String) -> Bool {
        let trimmed = line.trimmingCharacters(in: .whitespaces)
        guard trimmed.contains("|") else { return false }
        var stripped = trimmed
        if stripped.hasPrefix("|") { stripped.removeFirst() }
        if stripped.hasSuffix("|") { stripped.removeLast() }
        let parts = stripped.split(separator: "|", omittingEmptySubsequences: false)
        guard !parts.isEmpty else { return false }
        for part in parts {
            let trimmedPart = part.trimmingCharacters(in: .whitespaces)
            guard !trimmedPart.isEmpty else { return false }
            var hasDash = false
            for ch in trimmedPart {
                if ch == "-" {
                    hasDash = true
                } else if ch != ":" {
                    return false
                }
            }
            guard hasDash else { return false }
        }
        return true
    }

    private static func hasComplexTablePipes(_ line: String) -> Bool {
        if line.contains("\\|") { return true }
        guard line.contains("`") else { return false }
        var insideCode = false
        for ch in line {
            if ch == "`" {
                insideCode.toggle()
            } else if ch == "|" && insideCode {
                return true
            }
        }
        return false
    }

    private static func parseTableCells(_ line: String) -> [String] {
        let trimmed = line.trimmingCharacters(in: .whitespaces)
        var stripped = trimmed
        if stripped.hasPrefix("|") { stripped.removeFirst() }
        if stripped.hasSuffix("|") { stripped.removeLast() }
        return stripped.split(separator: "|", omittingEmptySubsequences: false).map {
            $0.trimmingCharacters(in: .whitespaces)
        }
    }
}
