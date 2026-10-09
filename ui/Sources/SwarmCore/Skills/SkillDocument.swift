import CryptoKit
import Foundation

public enum SkillDocumentState: Equatable, Sendable {
    case bundledReadOnly(reason: String)
    case checkoutEditable
    case missingCheckoutSource(reason: String)
    case noTable
    case invalidSource(reason: String)

    public var reason: String? {
        switch self {
        case .checkoutEditable: nil
        case .noTable: "no step table"
        case .bundledReadOnly(let reason), .missingCheckoutSource(let reason), .invalidSource(let reason): reason
        }
    }
}

public enum SkillCapability: Equatable, Sendable {
    case generic, scriptOwned
    public var reason: String? {
        self == .scriptOwned ? "this skill's script owns its step ids" : nil
    }
}

public struct SkillRow: Identifiable, Equatable, Sendable {
    public let id: String
    public let stem: String
    public let name: String
    public let needs: [String]
    public let needsText: String
    public let holds: String
    public let range: Range<Int>
}

public struct SkillSection: Equatable, Sendable {
    public let headingRange: Range<Int>
    public let bodyRange: Range<Int>
    public let body: String
    public var range: Range<Int> { headingRange.lowerBound..<bodyRange.upperBound }
}

public struct SkillDocument: Sendable {
    public let key: String
    public let bytes: Data
    public let sourceURL: URL?
    public let revision: String
    public let lineEnding: String
    public let tableRange: Range<Int>?
    public let rows: [SkillRow]
    public let sections: [String: SkillSection]
    public let state: SkillDocumentState
    public let capability: SkillCapability

    public static func parse(data: Data, key: String, sourceURL: URL? = nil,
                             state: SkillDocumentState = .checkoutEditable) -> SkillDocument {
        SkillParser(data: data, key: key, sourceURL: sourceURL, sourceState: state).parse()
    }

    static func revision(of data: Data) -> String {
        SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined()
    }
}

// Byte spans keep all text outside the table and bound sections opaque to the writer.
struct SkillLine {
    let text: String
    let range: Range<Int>
    let contentRange: Range<Int>
    var inFrontMatter = false
    var inFence = false

    static func scan(_ data: Data, frontMatter: Bool = true) -> [SkillLine] {
        let bytes = Array(data)
        var result: [SkillLine] = []
        var start = 0
        for index in bytes.indices where bytes[index] == 10 {
            let end = index > start && bytes[index - 1] == 13 ? index - 1 : index
            result.append(SkillLine(text: String(decoding: bytes[start..<end], as: UTF8.self),
                                    range: start..<(index + 1), contentRange: start..<end))
            start = index + 1
        }
        if start < bytes.count {
            result.append(SkillLine(text: String(decoding: bytes[start...], as: UTF8.self),
                                    range: start..<bytes.count, contentRange: start..<bytes.count))
        }
        var frontMatter = frontMatter && result.first?.text == "---"
        var fence: (Character, Int)?
        for index in result.indices {
            let text = result[index].text
            result[index].inFrontMatter = frontMatter
            if frontMatter {
                if index > 0 && (text == "---" || text == "...") { frontMatter = false }
                continue
            }
            result[index].inFence = fence != nil
            let indentation = text.prefix { $0 == " " }.count
            guard indentation <= 3 else { continue }
            let rest = text.dropFirst(indentation)
            guard let marker = rest.first, marker == "`" || marker == "~" else { continue }
            let count = rest.prefix { $0 == marker }.count
            guard count >= 3 else { continue }
            let tail = rest.dropFirst(count)
            if let open = fence {
                if marker == open.0 && count >= open.1 && tail.trimmingCharacters(in: .whitespaces).isEmpty {
                    fence = nil
                }
            } else if marker != "`" || !tail.contains("`") {
                result[index].inFence = true
                fence = (marker, count)
            }
        }
        return result
    }

    var heading: (level: Int, name: String)? {
        guard !inFence && !inFrontMatter else { return nil }
        let indent = text.prefix { $0 == " " }.count
        guard indent <= 3 else { return nil }
        let rest = text.dropFirst(indent)
        let level = rest.prefix { $0 == "#" }.count
        guard (1...6).contains(level), rest.dropFirst(level).isEmpty
            || rest.dropFirst(level).first.map({ $0 == " " || $0 == "\t" }) == true else { return nil }
        return (level, rest.dropFirst(level).trimmingCharacters(in: .whitespaces))
    }
}

enum SkillNeeds {
    struct Word {
        let word: String
        let range: NSRange
    }

    static func words(_ text: String) -> [Word] {
        let separator = try! NSRegularExpression(pattern: ",|\\band\\b")
        let source = text as NSString
        let splits = separator.matches(in: text, range: NSRange(location: 0, length: source.length))
        var start = 0
        var ranges: [NSRange] = []
        for split in splits {
            ranges.append(NSRange(location: start, length: split.range.location - start))
            start = NSMaxRange(split.range)
        }
        ranges.append(NSRange(location: start, length: source.length - start))
        let wordPattern = try! NSRegularExpression(pattern: "\\S+")
        return ranges.compactMap { range in
            let cell = source.substring(with: range).trimmingCharacters(in: .whitespacesAndNewlines)
            guard !cell.isEmpty, cell != "none", let match = wordPattern.firstMatch(in: text, range: range) else { return nil }
            return Word(word: source.substring(with: match.range), range: match.range)
        }
    }

    static func resolve(_ text: String, names: [(id: String, name: String)]) -> (ids: [String], errors: [String]) {
        var ids: [String] = []
        var errors: [String] = []
        for token in words(text) {
            let matches = names.filter { $0.name.hasPrefix(token.word) }
            if matches.isEmpty { errors.append("Unknown Needs word \(token.word).") }
            else if matches.count > 1 { errors.append("Ambiguous Needs word \(token.word).") }
            else if ids.contains(matches[0].id) { errors.append("Duplicate Needs word \(token.word).") }
            else { ids.append(matches[0].id) }
        }
        return (ids, errors)
    }

    static func hasCycle(_ edges: [(id: String, needs: [String])]) -> Bool {
        let needs = Dictionary(edges.map { ($0.id, $0.needs) }, uniquingKeysWith: { first, _ in first })
        var visited = Set<String>()
        var active = Set<String>()
        func visit(_ id: String) -> Bool {
            if active.contains(id) { return true }
            if visited.contains(id) { return false }
            active.insert(id)
            if (needs[id] ?? []).contains(where: visit) { return true }
            active.remove(id)
            visited.insert(id)
            return false
        }
        return edges.contains { visit($0.id) }
    }
}

private struct SkillParser {
    let data: Data
    let key: String
    let sourceURL: URL?
    let sourceState: SkillDocumentState

    func parse() -> SkillDocument {
        let lines = SkillLine.scan(data)
        let ending = lines.first(where: { $0.range.upperBound > $0.contentRange.upperBound })
            .map { $0.range.upperBound - $0.contentRange.upperBound == 2 ? "\r\n" : "\n" } ?? "\n"
        let source = String(decoding: data, as: UTF8.self)
        let ownsScript = source.range(of: #"scripts/[^\s`]+\.py\s+(?:start|take|done|status)\b"#, options: .regularExpression) != nil
        let generic = source.range(of: #"references/step_run\.py\s+start\b"#, options: .regularExpression) != nil
        let capability: SkillCapability = !ownsScript && generic ? .generic : .scriptOwned
        func result(_ state: SkillDocumentState, table: Range<Int>? = nil, rows: [SkillRow] = [], sections: [String: SkillSection] = [:]) -> SkillDocument {
            SkillDocument(key: key, bytes: data, sourceURL: sourceURL, revision: SkillDocument.revision(of: data), lineEnding: ending,
                          tableRange: table, rows: rows, sections: sections, state: state, capability: capability)
        }
        guard String(data: data, encoding: .utf8) != nil else { return result(.invalidSource(reason: "Source is not UTF-8.")) }
        let header = #"^\|\s*File\s*\|\s*Needs\s*\|\s*Holds\s*\|"#
        guard let start = lines.firstIndex(where: { $0.text.range(of: header, options: .regularExpression) != nil }) else {
            return result(.noTable)
        }
        guard !lines[start].inFrontMatter && !lines[start].inFence else {
            return result(.invalidSource(reason: "The first step table is inside front matter or a code fence."))
        }
        var end = start + 1
        while end < lines.count && lines[end].text.hasPrefix("|") { end += 1 }
        let range = lines[start].range.lowerBound..<lines[end - 1].range.upperBound
        guard cells(lines[start].text)?.count == 3, start + 1 < end, lines[start + 1].text == "|---|---|---|", end > start + 2 else {
            return result(.invalidSource(reason: "The step table needs three columns, |---|---|---|, and at least one row."), table: range)
        }
        var parsed: [(stem: String, name: String, needs: String, holds: String, range: Range<Int>)] = []
        var errors: [String] = []
        for index in (start + 2)..<end {
            guard let cells = cells(lines[index].text), cells.count == 3,
                  cells[0].range(of: #"^[0-9]{2}-[a-z][a-z0-9]*(?:-[a-z0-9]+)*\.md$"#, options: .regularExpression) != nil else {
                errors.append("A table row needs three cells and an NN-name.md file.")
                continue
            }
            let stem = String(cells[0].dropLast(3))
            let name = String(stem.dropFirst(3))
            if stem.hasPrefix("00-") || name == "none" || name == "and" { errors.append("Invalid step name \(stem).") }
            parsed.append((stem, name, cells[1], cells[2], lines[index].range))
        }
        if parsed.count > 99 { errors.append("A skill needs 1-99 steps.") }
        if Set(parsed.map(\.name)).count != parsed.count { errors.append("Duplicate step names.") }
        if Set(parsed.map { String($0.stem.prefix(2)) }).count != parsed.count { errors.append("Duplicate step prefixes.") }
        let names = parsed.map { (id: $0.stem, name: $0.name) }
        let rows = parsed.map { row in
            let needs = SkillNeeds.resolve(row.needs, names: names)
            errors += needs.errors
            if needs.ids.contains(row.stem) { errors.append("A step cannot need itself.") }
            return SkillRow(id: row.stem, stem: row.stem, name: row.name, needs: needs.ids,
                            needsText: row.needs, holds: row.holds, range: row.range)
        }
        if SkillNeeds.hasCycle(rows.map { ($0.id, $0.needs) }) { errors.append("Needs contains a cycle.") }
        var sections: [String: SkillSection] = [:]
        for row in rows {
            let matches = lines.indices.filter { lines[$0].heading?.level == 2 && lines[$0].heading?.name == row.stem }
            if matches.count > 1 { errors.append("Duplicate step heading \(row.stem)."); continue }
            guard let index = matches.first else { continue }
            let stop = lines.indices.dropFirst(index + 1).first { (lines[$0].heading?.level ?? 7) <= 2 }
            let bodyRange = lines[index].range.upperBound..<(stop.map { lines[$0].range.lowerBound } ?? data.count)
            sections[row.id] = SkillSection(headingRange: lines[index].range, bodyRange: bodyRange,
                                           body: String(decoding: data.subdata(in: bodyRange), as: UTF8.self))
        }
        return result(errors.isEmpty ? sourceState : .invalidSource(reason: errors.joined(separator: " ")),
                      table: range, rows: rows, sections: sections)
    }

    private func cells(_ line: String) -> [String]? {
        guard line.hasPrefix("|"), line.hasSuffix("|") else { return nil }
        return line.dropFirst().dropLast().split(separator: "|", omittingEmptySubsequences: false)
            .map { $0.trimmingCharacters(in: .whitespaces).trimmingCharacters(in: CharacterSet(charactersIn: "`")) }
    }
}
