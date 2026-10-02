import Foundation
import Testing
@testable import SwarmCore

/// A temp folder that tests fill with the files a CLI would have written.
private struct Tree {
    let root = FileManager.default.temporaryDirectory
        .appendingPathComponent("ComposerPlugins-\(UUID().uuidString)")

    func path(_ relative: String) -> String { root.appendingPathComponent(relative).path }

    func write(_ relative: String, _ text: String) throws {
        let url = root.appendingPathComponent(relative)
        try FileManager.default.createDirectory(
            at: url.deletingLastPathComponent(), withIntermediateDirectories: true
        )
        try text.write(to: url, atomically: true, encoding: .utf8)
    }

    func remove() { try? FileManager.default.removeItem(at: root) }

    /// Writes `installed_plugins.json` for a user-scope install under `cache/<name>`.
    func install(_ id: String, as name: String, scope: String = "user", projectPath: String? = nil) throws -> String {
        let installPath = path("config/plugins/cache/" + name)
        var entry = "\"scope\": \"\(scope)\", \"installPath\": \"\(installPath)\""
        if let projectPath { entry += ", \"projectPath\": \"\(projectPath)\"" }
        try write("config/plugins/installed_plugins.json", """
        {"version": 2, "plugins": {"\(id)": [{\(entry)}]}}
        """)
        return installPath
    }
}

@Suite("Composer plugins")
struct ComposerPluginsTests {
    private static let skill = "---\ndescription: A skill\n---\n"

    @Test("An enabled plugin with default folders gives its skills and its markdown commands")
    func defaultFolders() throws {
        let tree = Tree()
        defer { tree.remove() }
        let install = try tree.install("ponytail@ponytail", as: "ponytail")
        try tree.write("config/settings.json", #"{"enabledPlugins": {"ponytail@ponytail": true}}"#)
        try tree.write("config/plugins/cache/ponytail/skills/ponytail/SKILL.md",
                       "---\nname: ponytail\ndescription: Lazy mode\n---\n")
        try tree.write("config/plugins/cache/ponytail/commands/gain.md", "---\ndescription: Show gain\n---\n")
        try tree.write("config/plugins/cache/ponytail/commands/gain.toml", "description = \"x\"")

        let plugins = ComposerPluginReader.enabled(configDirectory: tree.path("config"), projectDirectory: nil)
        #expect(plugins == [ComposerPlugin(
            name: "ponytail", skillRoots: [install + "/skills"], commandRoots: [install + "/commands"]
        )])

        let found = ComposerCommandCatalog.discover(from: ComposerCommandSource(
            provider: "claude", homeDirectory: tree.path("home"), configDirectory: tree.path("config")
        ))
        let skill = try #require(found.first { $0.name == "ponytail:ponytail" })
        #expect(skill.kind == .plugin("ponytail") && skill.detail == "Lazy mode")
        #expect(found.contains { $0.name == "ponytail:gain" && $0.kind == .plugin("ponytail") })
        #expect(found.filter { $0.name.hasPrefix("ponytail:") }.count == 2)
    }

    @Test("A plugin that is false or absent in enabledPlugins gives nothing")
    func disabledPlugin() throws {
        let tree = Tree()
        defer { tree.remove() }
        _ = try tree.install("ponytail@ponytail", as: "ponytail")
        try tree.write("config/plugins/cache/ponytail/skills/ponytail/SKILL.md", Self.skill)
        let config = tree.path("config")

        try tree.write("config/settings.json", #"{"enabledPlugins": {"ponytail@ponytail": false}}"#)
        #expect(ComposerPluginReader.enabled(configDirectory: config, projectDirectory: nil).isEmpty)
        try tree.write("config/settings.json", #"{"enabledPlugins": {}}"#)
        #expect(ComposerPluginReader.enabled(configDirectory: config, projectDirectory: nil).isEmpty)
    }

    @Test("A skills array of nested skill folders gives each skill")
    func nestedSkillFolders() throws {
        let tree = Tree()
        defer { tree.remove() }
        _ = try tree.install("matt@market", as: "matt")
        try tree.write("config/settings.json", #"{"enabledPlugins": {"matt@market": true}}"#)
        try tree.write("config/plugins/cache/matt/.claude-plugin/plugin.json", """
        {"skills": ["./skills/engineering/tdd", "./skills/productivity", "../outside"]}
        """)
        try tree.write("config/plugins/cache/matt/skills/engineering/tdd/SKILL.md", Self.skill)
        try tree.write("config/plugins/cache/matt/skills/productivity/grilling/SKILL.md", Self.skill)
        try tree.write("config/plugins/cache/matt/skills/productivity/handoff/SKILL.md", Self.skill)
        try tree.write("config/plugins/cache/outside/SKILL.md", Self.skill)

        let found = ComposerCommandCatalog.discover(from: ComposerCommandSource(
            provider: "claude", homeDirectory: tree.path("home"), configDirectory: tree.path("config")
        ))
        let names = found.filter { $0.kind == .plugin("matt") }.map(\.name)
        #expect(names == ["matt:grilling", "matt:handoff", "matt:tdd"])
    }

    @Test("A project entry counts inside its project, and project local settings win")
    func projectScope() throws {
        let tree = Tree()
        defer { tree.remove() }
        let project = tree.path("work/app")
        _ = try tree.install("lint@market", as: "lint", scope: "project", projectPath: project)
        try tree.write("config/plugins/cache/lint/skills/lint/SKILL.md", Self.skill)
        try tree.write("config/settings.json", #"{"enabledPlugins": {"lint@market": true}}"#)
        let config = tree.path("config")

        #expect(ComposerPluginReader.enabled(configDirectory: config, projectDirectory: project + "/Sources").map(\.name) == ["lint"])
        #expect(ComposerPluginReader.enabled(configDirectory: config, projectDirectory: project).map(\.name) == ["lint"])
        #expect(ComposerPluginReader.enabled(configDirectory: config, projectDirectory: tree.path("work/other")).isEmpty)
        #expect(ComposerPluginReader.enabled(configDirectory: config, projectDirectory: project + "-two").isEmpty)
        #expect(ComposerPluginReader.enabled(configDirectory: config, projectDirectory: nil).isEmpty)

        try tree.write("work/app/.claude/settings.json", #"{"enabledPlugins": {"lint@market": true}}"#)
        try tree.write("work/app/.claude/settings.local.json", #"{"enabledPlugins": {"lint@market": false}}"#)
        #expect(ComposerPluginReader.enabled(configDirectory: config, projectDirectory: project).isEmpty)
    }

    @Test("Missing or malformed JSON gives no plugins")
    func malformedFiles() throws {
        let tree = Tree()
        defer { tree.remove() }
        let config = tree.path("config")
        #expect(ComposerPluginReader.enabled(configDirectory: config, projectDirectory: nil).isEmpty)

        try tree.write("config/plugins/installed_plugins.json", "{ not json")
        #expect(ComposerPluginReader.enabled(configDirectory: config, projectDirectory: nil).isEmpty)
        try tree.write("config/plugins/installed_plugins.json", #"{"version": 1, "plugins": {}}"#)
        #expect(ComposerPluginReader.enabled(configDirectory: config, projectDirectory: nil).isEmpty)

        let install = try tree.install("a@b", as: "a")
        try tree.write("config/settings.json", "[1, 2")
        #expect(ComposerPluginReader.enabled(configDirectory: config, projectDirectory: nil).isEmpty)
        try tree.write("config/settings.json", #"{"enabledPlugins": {"a@b": true}}"#)
        try tree.write("config/plugins/cache/a/.claude-plugin/plugin.json", "oops")
        try tree.write("config/plugins/cache/a/skills/one/SKILL.md", Self.skill)
        #expect(ComposerPluginReader.enabled(configDirectory: config, projectDirectory: nil)
            == [ComposerPlugin(name: "a", skillRoots: [install + "/skills"], commandRoots: [])])
    }

    @Test("A log path gives its config folder, and an unknown path gives nil")
    func configDirectoryFromLog() {
        let claude = "/Users/me/.claude/.profiles/work/projects/-Users-me-app/abc.jsonl"
        #expect(ComposerCommandSource.configDirectory(fromLog: claude, provider: "claude")
            == "/Users/me/.claude/.profiles/work")
        let rollout = "/Users/me/.codex-work/sessions/2026/10/02/rollout-2026-10-02T10-00-00-abc.jsonl"
        #expect(ComposerCommandSource.configDirectory(fromLog: rollout, provider: "codex")
            == "/Users/me/.codex-work")
        let archived = "/Users/me/.codex-work/archived_sessions/rollout-2026-10-02T10-00-00-abc.jsonl"
        #expect(ComposerCommandSource.configDirectory(fromLog: archived, provider: "codex")
            == "/Users/me/.codex-work")
        #expect(ComposerCommandSource.configDirectory(fromLog: "/tmp/chat/log.jsonl", provider: "claude") == nil)
        #expect(ComposerCommandSource.configDirectory(fromLog: "/tmp/chat/log.jsonl", provider: "codex") == nil)
        #expect(ComposerCommandSource.configDirectory(fromLog: claude, provider: "codex") == nil)
        #expect(ComposerCommandSource.configDirectory(fromLog: claude, provider: nil) == nil)
    }

    @Test("A chair log outside every account still finds its config folder")
    func resolveFromLog() {
        let session = SwarmSession(
            id: SwarmSessionID("one"), talkMode: "solo", adapter: nil,
            cwd: "/work", createdAt: 0,
            chairLog: "/Users/me/.claude/.profiles/work/projects/-work/abc.jsonl",
            agents: 0, messages: 0, lastMessageAt: nil
        )
        let source = ComposerCommandSource.resolve(
            provider: "claude", session: session, accounts: [], homeDirectory: "/Users/me"
        )
        #expect(source.configDirectory == "/Users/me/.claude/.profiles/work")
    }

    @Test("AGY finds a workspace skill from a subfolder, skips a disabled one, and has its own built-ins")
    func agyDiscovery() throws {
        let tree = Tree()
        defer { tree.remove() }
        try tree.write("work/app/.git/HEAD", "ref: refs/heads/main")
        try tree.write("work/app/.agents/skills/deploy/SKILL.md", "---\ndescription: Deploy it\n---\n")
        try tree.write("work/app/.agents/skills/hidden/SKILL.md",
                       "---\ndescription: Hidden\ndisable-slash-command: true\n---\n")
        try tree.write("work/app/Sources/deep/.keep", "")
        try tree.write("home/.gemini/config/skills/global/SKILL.md", Self.skill)
        try tree.write("home/.gemini/config/plugins/tools/skills/lint/SKILL.md", Self.skill)
        try tree.write("home/.claude/skills/claude-only/SKILL.md", Self.skill)

        let found = ComposerCommandCatalog.discover(from: ComposerCommandSource(
            provider: "agy", homeDirectory: tree.path("home"),
            projectDirectory: tree.path("work/app/Sources/deep")
        ))
        #expect(found.first { $0.name == "deploy" }?.detail == "Deploy it")
        #expect(!found.contains { $0.name == "hidden" })
        #expect(found.contains { $0.name == "global" && $0.kind == .skill })
        #expect(found.contains { $0.name == "tools:lint" && $0.kind == .plugin("tools") })
        #expect(!found.contains { $0.name == "claude-only" })
        #expect(found.contains { $0.name == "clear" && $0.kind == .builtIn })
        #expect(!found.contains { $0.name == "rewind" || $0.name == "add-dir" })
    }

    @Test("Codex finds project skills from a subfolder up to the repository root")
    func codexProjectSkills() throws {
        let tree = Tree()
        defer { tree.remove() }
        try tree.write("work/app/.git/HEAD", "ref: refs/heads/main")
        try tree.write("work/app/Sources/.codex/skills/native/SKILL.md", Self.skill)
        try tree.write("work/app/.agents/skills/shared/SKILL.md", Self.skill)
        try tree.write("work/outside/.agents/skills/stray/SKILL.md", Self.skill)

        let found = ComposerCommandCatalog.discover(from: ComposerCommandSource(
            provider: "codex", homeDirectory: tree.path("home"),
            projectDirectory: tree.path("work/app/Sources")
        ))
        #expect(found.contains { $0.name == "native" } && found.contains { $0.name == "shared" })
        #expect(!found.contains { $0.name == "stray" })
    }

    @Test("Claude built-ins include the listed commands and rows carry a source label")
    func claudeBuiltInsAndLabels() {
        let claude = Set(ComposerCommandCatalog.builtIns(provider: "claude").map(\.name))
        for name in ["add-dir", "agents", "config", "export", "help", "hooks", "mcp", "plan",
                     "plugin", "resume", "rewind", "skills", "usage"] {
            #expect(claude.contains(name))
        }
        #expect([ComposerCommand.Kind.builtIn, .skill, .command, .plugin("ponytail")]
            .map(\.sourceLabel) == ["Built-in", "Skill", "Command", "ponytail"])
    }

    @Test("An exact name ranks first")
    func exactMatchFirst() {
        let commands = ["clean-up", "clear", "clear-cache"].map {
            ComposerCommand(name: $0, detail: "", kind: .skill)
        }
        #expect(ComposerCommandCatalog.matches(commands, query: "clear").first?.command.name == "clear")
        #expect(ComposerCommandCatalog.matches(commands, query: "CLEAR").first?.command.name == "clear")
    }
}
