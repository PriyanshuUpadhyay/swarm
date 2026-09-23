import Foundation

public struct ComposerCommand: Identifiable, Hashable, Sendable {
    public enum Kind: Hashable, Sendable { case builtIn, command, skill }

    public var name: String
    public var detail: String
    public var kind: Kind
    public var path: String?
    public var id: String { name }

    public init(name: String, detail: String, kind: Kind, path: String? = nil) {
        self.name = name
        self.detail = detail
        self.kind = kind
        self.path = path
    }
}

public struct ComposerCommandMatch: Identifiable, Hashable, Sendable {
    public var command: ComposerCommand
    public var score: Int
    public var id: String { command.id }
}

public struct ComposerCommandSource: Equatable, Sendable {
    public var provider: String?
    public var homeDirectory: String
    public var claudeConfigDirectories: [String]
    public var codexDirectories: [String]

    public init(
        provider: String?, homeDirectory: String,
        claudeConfigDirectories: [String] = [], codexDirectories: [String] = []
    ) {
        self.provider = provider
        self.homeDirectory = homeDirectory
        self.claudeConfigDirectories = claudeConfigDirectories
        self.codexDirectories = codexDirectories
    }
}

/// Adapted from Bloom's SlashCommand and SlashCommandIndex, with only user-level sources.
public enum ComposerCommandCatalog {
    private static let limit = 500

    public static func discover(from source: ComposerCommandSource) -> [ComposerCommand] {
        var values: [String: ComposerCommand] = [:]
        for command in builtIns(provider: source.provider) { values[command.name] = command }

        var claudeRoots = [source.homeDirectory + "/.claude"]
        claudeRoots.append(contentsOf: source.claudeConfigDirectories)
        for root in unique(claudeRoots) {
            add(commandFiles(in: root + "/commands"), to: &values)
            add(skillFiles(in: root + "/skills"), to: &values)
        }

        var codexRoots = [source.homeDirectory + "/.codex"]
        codexRoots.append(contentsOf: source.codexDirectories)
        for root in unique(codexRoots) {
            add(promptFiles(in: root + "/prompts"), to: &values)
            add(skillFiles(in: root + "/skills"), to: &values)
            add(skillFiles(in: root + "/skills/.system"), to: &values)
        }
        add(skillFiles(in: source.homeDirectory + "/.agents/skills"), to: &values)
        return values.values.sorted { $0.name < $1.name }
    }

    public static func matches(
        _ commands: [ComposerCommand], query: String, limit: Int = 12
    ) -> [ComposerCommandMatch] {
        guard !query.isEmpty else {
            return commands.prefix(limit).map { ComposerCommandMatch(command: $0, score: 0) }
        }
        var found: [ComposerCommandMatch] = []
        for command in commands {
            guard let score = composerFuzzyScore(command.name, query: query) else { continue }
            found.append(ComposerCommandMatch(command: command, score: score))
        }
        found.sort {
            $0.score == $1.score ? $0.command.name < $1.command.name : $0.score > $1.score
        }
        return Array(found.prefix(limit))
    }

    public static func builtIns(provider: String?) -> [ComposerCommand] {
        let values: [(String, String)]
        if provider?.lowercased().contains("codex") == true {
            values = [
                ("compact", "Compact the current conversation"),
                ("diff", "Show the current changes"),
                ("init", "Create agent instructions for this repository"),
                ("mention", "Add a file to the conversation"),
                ("model", "Choose the model"),
                ("new", "Start a new conversation"),
                ("permissions", "Change approval permissions"),
                ("review", "Review the current changes"),
                ("status", "Show session status"),
            ]
        } else {
            values = [
                ("clear", "Start a new conversation"),
                ("compact", "Compact the current conversation"),
                ("context", "Show context use"),
                ("cost", "Show token use and cost"),
                ("diff", "Show the current changes"),
                ("init", "Create CLAUDE.md for this repository"),
                ("memory", "Edit memory files"),
                ("model", "Choose the model"),
                ("permissions", "Change tool permissions"),
                ("review", "Review the current changes"),
                ("status", "Show session status"),
            ]
        }
        return values.map { ComposerCommand(name: $0.0, detail: $0.1, kind: .builtIn) }
    }

    private static func add(
        _ commands: [ComposerCommand], to values: inout [String: ComposerCommand]
    ) {
        for command in commands { values[command.name] = command }
    }

    private static func unique(_ values: [String]) -> [String] {
        var seen: Set<String> = []
        return values.filter { !$0.isEmpty && seen.insert($0).inserted }
    }

    private static func commandFiles(in directory: String) -> [ComposerCommand] {
        let root = URL(fileURLWithPath: directory).standardizedFileURL.path
        return markdownFiles(in: directory, maximumDepth: 5).compactMap {
            path -> ComposerCommand? in
            let normalized = URL(fileURLWithPath: path).standardizedFileURL.path
            guard normalized.hasPrefix(root + "/") else { return nil }
            let relative = String(normalized.dropFirst(root.count + 1)).dropLast(3)
            let name = relative.replacingOccurrences(of: "/", with: ":")
            return valid(name).map {
                ComposerCommand(
                    name: $0, detail: description(in: path), kind: .command, path: path
                )
            }
        }
    }

    private static func promptFiles(in directory: String) -> [ComposerCommand] {
        markdownFiles(in: directory, maximumDepth: 3).compactMap { path in
            let name = (path as NSString).deletingPathExtension
                .components(separatedBy: "/").last ?? ""
            return valid(name).map {
                ComposerCommand(
                    name: $0, detail: description(in: path), kind: .command, path: path
                )
            }
        }
    }

    private static func skillFiles(in directory: String) -> [ComposerCommand] {
        guard let names = try? FileManager.default.contentsOfDirectory(atPath: directory) else {
            return []
        }
        return names.sorted().prefix(limit).compactMap { name in
            let path = directory + "/" + name + "/SKILL.md"
            guard FileManager.default.fileExists(atPath: path), let validName = valid(name) else {
                return nil
            }
            let fields = frontmatter(in: path)
            return ComposerCommand(
                name: fields.name.flatMap(valid) ?? validName,
                detail: fields.description ?? firstProseLine(in: path),
                kind: .skill,
                path: path
            )
        }
    }

    private static func markdownFiles(in directory: String, maximumDepth: Int) -> [String] {
        let manager = FileManager.default
        guard let enumerator = manager.enumerator(
            at: URL(fileURLWithPath: directory),
            includingPropertiesForKeys: [.isRegularFileKey, .isDirectoryKey],
            options: [.skipsHiddenFiles, .skipsPackageDescendants]
        ) else { return [] }
        var files: [String] = []
        for case let url as URL in enumerator {
            let relative = url.path.dropFirst(directory.count).split(separator: "/")
            if relative.count > maximumDepth { enumerator.skipDescendants(); continue }
            if url.pathExtension.lowercased() == "md" { files.append(url.path) }
            if files.count == limit { break }
        }
        return files.sorted()
    }

    private static func description(in path: String) -> String {
        frontmatter(in: path).description ?? firstProseLine(in: path)
    }

    private static func frontmatter(in path: String) -> (name: String?, description: String?) {
        guard let data = FileManager.default.contents(atPath: path),
              let text = String(data: data.prefix(8_192), encoding: .utf8) else {
            return (nil, nil)
        }
        let lines = text.components(separatedBy: .newlines)
        guard lines.first?.trimmingCharacters(in: .whitespaces) == "---" else {
            return (nil, nil)
        }
        var name: String?
        var detail: String?
        for line in lines.dropFirst() {
            let trimmed = line.trimmingCharacters(in: .whitespaces)
            if trimmed == "---" { break }
            if trimmed.hasPrefix("name:") { name = fieldValue(trimmed, key: "name") }
            if trimmed.hasPrefix("description:") {
                detail = fieldValue(trimmed, key: "description")
            }
        }
        return (name, detail)
    }

    private static func fieldValue(_ line: String, key: String) -> String? {
        let value = line.dropFirst(key.count + 1).trimmingCharacters(in: .whitespaces)
            .trimmingCharacters(in: CharacterSet(charactersIn: "\"'"))
        return value.isEmpty ? nil : String(value.prefix(120))
    }

    private static func firstProseLine(in path: String) -> String {
        guard let text = try? String(contentsOfFile: path, encoding: .utf8) else { return "" }
        return text.components(separatedBy: .newlines).first { line in
            let value = line.trimmingCharacters(in: .whitespaces)
            return !value.isEmpty && value != "---" && !value.hasPrefix("#")
                && !value.contains(":")
        }.map { String($0.prefix(120)) } ?? ""
    }

    private static func valid(_ raw: String) -> String? {
        let value = raw.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        let allowed = CharacterSet(charactersIn: "abcdefghijklmnopqrstuvwxyz0123456789-_.:")
        guard !value.isEmpty, value.count <= 120,
              value.unicodeScalars.allSatisfy(allowed.contains) else { return nil }
        return value
    }
}

func composerFuzzyScore(_ candidate: String, query: String) -> Int? {
    let candidate = Array(candidate.lowercased())
    let query = Array(query.lowercased())
    var positions: [Int] = []
    var start = 0
    for character in query {
        guard let offset = candidate[start...].firstIndex(of: character) else { return nil }
        positions.append(offset)
        start = offset + 1
    }
    var score = max(0, 100 - candidate.count)
    for (left, right) in zip(positions, positions.dropFirst()) where right == left + 1 {
        score += 20
    }
    if positions.first == 0 { score += 30 }
    return score
}
