import Foundation

public struct ComposerCommand: Identifiable, Hashable, Sendable {
    public enum Kind: Hashable, Sendable {
        case builtIn, command, skill
        /// A skill or command a plugin ships. The value is the plugin name.
        case plugin(String)

        /// The tag a menu row shows beside the command.
        public var sourceLabel: String {
            switch self {
            case .builtIn: "Built-in"
            case .command: "Command"
            case .skill: "Skill"
            case .plugin(let name): name
            }
        }
    }

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
    public var configDirectory: String?
    public var projectDirectory: String?

    public init(
        provider: String?, homeDirectory: String,
        configDirectory: String? = nil, projectDirectory: String? = nil
    ) {
        self.provider = provider
        self.homeDirectory = homeDirectory
        self.configDirectory = configDirectory
        self.projectDirectory = projectDirectory
    }

    public static func resolve(
        provider: String?, session: SwarmSession, accounts: [SwarmAccount],
        homeDirectory: String
    ) -> Self {
        let kind = provider?.lowercased()
        let log = session.chairLog.map { URL(fileURLWithPath: $0).resolvingSymlinksInPath().standardized.path }
        let account = accounts.filter { account in
            guard kind == "claude" || kind == "codex" else { return false }
            guard let log else { return false }
            let home = URL(fileURLWithPath: account.home).resolvingSymlinksInPath().standardized.path
            return log == home || log.hasPrefix(home + "/")
        }.max { $0.home.count < $1.home.count }
        let key = kind == "codex" ? "CODEX_HOME" : "CLAUDE_CONFIG_DIR"
        let config = account.map { $0.env[key] ?? $0.home }
            ?? session.chairLog.flatMap { configDirectory(fromLog: $0, provider: provider) }
        return Self(
            provider: provider, homeDirectory: homeDirectory,
            configDirectory: config, projectDirectory: session.cwd
        )
    }

    /// `<config>/projects/<slug>/<id>.jsonl` gives `<config>` for Claude. For Codex,
    /// `<home>/sessions/Y/M/D/rollout-*.jsonl` and `<home>/archived_sessions/rollout-*.jsonl`
    /// give `<home>`. Any other path gives nil.
    public static func configDirectory(fromLog log: String, provider: String?) -> String? {
        let parts = log.split(separator: "/", omittingEmptySubsequences: true).map(String.init)
        func root(dropping count: Int, marker: String) -> String? {
            guard parts.count > count, parts[parts.count - count] == marker,
                  parts.last?.hasSuffix(".jsonl") == true else { return nil }
            return "/" + parts.dropLast(count).joined(separator: "/")
        }
        switch provider?.lowercased() {
        case "claude": return root(dropping: 3, marker: "projects")
        case "codex":
            return root(dropping: 5, marker: "sessions")
                ?? root(dropping: 2, marker: "archived_sessions")
        default: return nil
        }
    }
}

/// Adapted from Bloom's SlashCommand and SlashCommandIndex.
public enum ComposerCommandCatalog {
    private static let limit = 500

    public static func discover(from source: ComposerCommandSource) -> [ComposerCommand] {
        var values: [String: ComposerCommand] = [:]
        for command in builtIns(provider: source.provider) { values[command.name] = command }

        let provider = source.provider?.lowercased()
        let home = source.homeDirectory
        if provider == "agy" {
            discoverAGY(from: source, into: &values)
            return values.values.sorted { $0.name < $1.name }
        }
        if provider != "codex" {
            let root = source.configDirectory ?? home + "/.claude"
            let plugins = ComposerPluginReader.enabled(
                configDirectory: root, projectDirectory: source.projectDirectory
            )
            for plugin in plugins {
                for path in plugin.skillRoots {
                    add(namespaced(pluginSkills(at: path), plugin: plugin.name), to: &values)
                }
                for path in plugin.commandRoots {
                    add(namespaced(pluginCommands(at: path), plugin: plugin.name), to: &values)
                }
            }
            add(commandFiles(in: root + "/commands"), to: &values)
            add(skillFiles(in: root + "/skills"), to: &values)
        }
        if provider != "claude" {
            let root = source.configDirectory ?? home + "/.codex"
            add(promptFiles(in: root + "/prompts"), to: &values)
            add(skillFiles(in: root + "/skills"), to: &values)
            add(skillFiles(in: root + "/skills/.system"), to: &values)
        }
        add(skillFiles(in: home + "/.agents/skills"), to: &values)
        if let project = source.projectDirectory {
            if provider == "claude" {
                add(commandFiles(in: project + "/.claude/commands"), to: &values)
                add(skillFiles(in: project + "/.claude/skills"), to: &values)
            }
            if provider == "codex" {
                add(skillFiles(in: project + "/.codex/skills"), to: &values)
                for folder in workspaceFolders(from: project).reversed() {
                    add(skillFiles(in: folder + "/.agents/skills"), to: &values)
                }
            }
        }
        return values.values.sorted { $0.name < $1.name }
    }

    private static func discoverAGY(
        from source: ComposerCommandSource, into values: inout [String: ComposerCommand]
    ) {
        let config = source.homeDirectory + "/.gemini/config"
        add(skillFiles(in: config + "/skills", hidingDisabled: true), to: &values)
        let plugins = (try? FileManager.default.contentsOfDirectory(atPath: config + "/plugins")) ?? []
        for plugin in plugins.sorted() {
            let skills = skillFiles(in: config + "/plugins/" + plugin + "/skills", hidingDisabled: true)
            add(namespaced(skills, plugin: plugin), to: &values)
        }
        guard let project = source.projectDirectory else { return }
        for folder in workspaceFolders(from: project).reversed() {
            for name in [".agents", ".agent", "_agents", "_agent"] {
                add(skillFiles(in: folder + "/" + name + "/skills", hidingDisabled: true), to: &values)
            }
        }
    }

    /// The folder and its parents up to the repository root, nearest first. Without a
    /// repository it is the folder alone.
    private static func workspaceFolders(from start: String) -> [String] {
        var folder = URL(fileURLWithPath: start).standardizedFileURL
        var folders: [String] = []
        while true {
            folders.append(folder.path)
            if FileManager.default.fileExists(atPath: folder.path + "/.git") { return folders }
            let parent = folder.deletingLastPathComponent()
            if parent.path == folder.path { return Array(folders.prefix(1)) }
            folder = parent
        }
    }

    public static func matches(
        _ commands: [ComposerCommand], query: String, limit: Int = 12
    ) -> [ComposerCommandMatch] {
        guard !query.isEmpty else {
            return commands.prefix(limit).map { ComposerCommandMatch(command: $0, score: 0) }
        }
        var found: [ComposerCommandMatch] = []
        for command in commands {
            let exact = command.name.lowercased() == query.lowercased()
            guard let score = exact ? Int.max : composerFuzzyScore(command.name, query: query) else {
                continue
            }
            found.append(ComposerCommandMatch(command: command, score: score))
        }
        found.sort {
            $0.score == $1.score ? $0.command.name < $1.command.name : $0.score > $1.score
        }
        return Array(found.prefix(limit))
    }

    public static func builtIns(provider: String?) -> [ComposerCommand] {
        let values: [(String, String)]
        if provider?.lowercased() == "agy" {
            // AGY 1.2.14 reference (antigravity.google/docs/cli/reference) and its changelog.
            values = [
                ("add-dir", "Add a working folder"),
                ("agents", "Manage agents"),
                ("btw", "Ask a side question"),
                ("clear", "Start a new conversation"),
                ("config", "Open settings"),
                ("context", "Show context use"),
                ("copy", "Copy the last answer"),
                ("diff", "Show the current changes"),
                ("effort", "Choose the reasoning effort"),
                ("fork", "Fork the conversation"),
                ("goal", "Set a goal"),
                ("help", "Show help"),
                ("hooks", "Show hook settings"),
                ("mcp", "Manage MCP servers"),
                ("model", "Choose the model"),
                ("new", "Start a new conversation"),
                ("permissions", "Change tool permissions"),
                ("plan", "Start plan mode"),
                ("plugin", "Manage plugins"),
                ("rename", "Rename the conversation"),
                ("resume", "Resume an earlier conversation"),
                ("rewind", "Rewind the conversation"),
                ("skills", "List skills"),
                ("tasks", "Show background tasks"),
                ("usage", "Show plan usage"),
            ]
        } else if provider?.lowercased().contains("codex") == true {
            values = [
                ("clear", "Start a new conversation"),
                ("compact", "Compact the current conversation"),
                ("diff", "Show the current changes"),
                ("init", "Create agent instructions for this repository"),
                ("mention", "Add a file to the conversation"),
                ("model", "Choose the model"),
                ("new", "Start a new conversation"),
                ("permissions", "Change approval permissions"),
                ("plan", "Start plan mode"),
                ("agents", "Manage agents"),
                ("export", "Export the conversation"),
                ("fork", "Fork the conversation"),
                ("mcp", "Manage MCP servers"),
                ("rename", "Rename the conversation"),
                ("resume", "Resume an earlier conversation"),
                ("review", "Review the current changes"),
                ("skills", "List skills"),
                ("status", "Show session status"),
                ("usage", "Show plan usage"),
            ]
        } else {
            values = [
                ("add-dir", "Add a working folder"),
                ("agents", "Manage agents"),
                ("clear", "Start a new conversation"),
                ("compact", "Compact the current conversation"),
                ("config", "Open settings"),
                ("context", "Show context use"),
                ("cost", "Show token use and cost"),
                ("diff", "Show the current changes"),
                ("export", "Export the conversation"),
                ("help", "Show help"),
                ("hooks", "Show hook settings"),
                ("init", "Create CLAUDE.md for this repository"),
                ("mcp", "Manage MCP servers"),
                ("memory", "Edit memory files"),
                ("model", "Choose the model"),
                ("permissions", "Change tool permissions"),
                ("plan", "Start plan mode"),
                ("plugin", "Manage plugins"),
                ("resume", "Resume an earlier conversation"),
                ("review", "Review the current changes"),
                ("rewind", "Rewind the conversation"),
                ("skills", "List skills"),
                ("status", "Show session status"),
                ("usage", "Show plan usage"),
            ]
        }
        return values.map { ComposerCommand(name: $0.0, detail: $0.1, kind: .builtIn) }
    }

    private static func add(
        _ commands: [ComposerCommand], to values: inout [String: ComposerCommand]
    ) {
        for command in commands { values[command.name] = command }
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

    private static func namespaced(_ commands: [ComposerCommand], plugin: String) -> [ComposerCommand] {
        commands.compactMap { command in
            valid(plugin + ":" + command.name).map {
                ComposerCommand(
                    name: $0, detail: command.detail, kind: .plugin(plugin), path: command.path
                )
            }
        }
    }

    /// A plugin path is one skill when it holds SKILL.md, else a folder of skills.
    private static func pluginSkills(at path: String) -> [ComposerCommand] {
        guard FileManager.default.fileExists(atPath: path + "/SKILL.md") else {
            return skillFiles(in: path)
        }
        return skill(in: path).map { [$0] } ?? []
    }

    /// A plugin path is a folder of commands, or one command file.
    private static func pluginCommands(at path: String) -> [ComposerCommand] {
        var isDirectory: ObjCBool = false
        guard FileManager.default.fileExists(atPath: path, isDirectory: &isDirectory) else { return [] }
        if isDirectory.boolValue { return commandFiles(in: path) }
        let name = ((path as NSString).lastPathComponent as NSString).deletingPathExtension
        guard path.hasSuffix(".md"), let validName = valid(name) else { return [] }
        return [ComposerCommand(
            name: validName, detail: description(in: path), kind: .command, path: path
        )]
    }

    private static func skillFiles(
        in directory: String, hidingDisabled: Bool = false
    ) -> [ComposerCommand] {
        guard let names = try? FileManager.default.contentsOfDirectory(atPath: directory) else {
            return []
        }
        return names.sorted().prefix(limit).compactMap {
            skill(in: directory + "/" + $0, hidingDisabled: hidingDisabled)
        }
    }

    private static func skill(in folder: String, hidingDisabled: Bool = false) -> ComposerCommand? {
        let path = folder + "/SKILL.md"
        let name = (folder as NSString).lastPathComponent
        guard FileManager.default.fileExists(atPath: path), let validName = valid(name) else {
            return nil
        }
        let fields = frontmatter(in: path)
        guard !(hidingDisabled && fields.disablesSlashCommand) else { return nil }
        return ComposerCommand(
            name: fields.name.flatMap(valid) ?? validName,
            detail: fields.description ?? firstProseLine(in: path),
            kind: .skill,
            path: path
        )
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

    private static func frontmatter(
        in path: String
    ) -> (name: String?, description: String?, disablesSlashCommand: Bool) {
        guard let data = FileManager.default.contents(atPath: path) else {
            return (nil, nil, false)
        }
        let text = String(decoding: data.prefix(8_192), as: UTF8.self)
        let lines = text.components(separatedBy: .newlines)
        guard lines.first?.trimmingCharacters(in: .whitespaces) == "---" else {
            return (nil, nil, false)
        }
        var name: String?
        var detail: String?
        var disabled = false
        var index = 1
        while index < lines.count {
            let line = lines[index]
            let trimmed = line.trimmingCharacters(in: .whitespaces)
            if trimmed == "---" { break }
            if trimmed.hasPrefix("name:") { name = fieldValue(trimmed, key: "name") }
            if trimmed.hasPrefix("disable-slash-command:") {
                disabled = fieldValue(trimmed, key: "disable-slash-command") == "true"
            }
            if trimmed.hasPrefix("description:") {
                let value = fieldValue(trimmed, key: "description")
                if let value, ["|", ">", "|-", ">-", "|+", ">+"].contains(value) {
                    var parts: [String] = []
                    var indentation: Int?
                    while index + 1 < lines.count {
                        let next = lines[index + 1]
                        let spaces = next.prefix(while: { $0 == " " }).count
                        if next.trimmingCharacters(in: .whitespaces).isEmpty {
                            parts.append("")
                            index += 1
                            continue
                        }
                        let width = indentation ?? spaces
                        guard spaces >= width, width > 0 else { break }
                        indentation = width
                        parts.append(String(next.dropFirst(width)))
                        index += 1
                    }
                    let joined = value.hasPrefix("|") ? parts.joined(separator: "\n")
                        : parts.joined(separator: " ")
                    let text = joined.trimmingCharacters(in: .whitespacesAndNewlines)
                    detail = text.isEmpty ? nil : String(text.prefix(120))
                } else {
                    detail = value
                }
            }
            index += 1
        }
        return (name, detail, disabled)
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
