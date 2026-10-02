import Foundation

/// An enabled Claude Code plugin and the folders that hold its skills and commands.
public struct ComposerPlugin: Equatable, Sendable {
    public var name: String
    public var skillRoots: [String]
    public var commandRoots: [String]

    public init(name: String, skillRoots: [String], commandRoots: [String]) {
        self.name = name
        self.skillRoots = skillRoots
        self.commandRoots = commandRoots
    }
}

/// Reads the plugin files Claude Code writes, with no `claude` process. A missing or malformed
/// file gives no plugins for that file, so discovery never throws.
public enum ComposerPluginReader {
    public static func enabled(
        configDirectory: String, projectDirectory: String?
    ) -> [ComposerPlugin] {
        guard let installed = object(at: configDirectory + "/plugins/installed_plugins.json"),
              installed["version"] as? Int == 2,
              let plugins = installed["plugins"] as? [String: Any] else { return [] }

        var switches: [String: Bool] = [:]
        var settings = [configDirectory + "/settings.json"]
        if let projectDirectory {
            settings += [
                projectDirectory + "/.claude/settings.json",
                projectDirectory + "/.claude/settings.local.json",
            ]
        }
        for file in settings {
            let found = object(at: file)?["enabledPlugins"] as? [String: Any] ?? [:]
            for (id, value) in found {
                if let isOn = value as? Bool { switches[id] = isOn }
            }
        }

        return plugins.keys.sorted().compactMap { id -> ComposerPlugin? in
            guard switches[id] == true,
                  let entry = (plugins[id] as? [[String: Any]])?.first(where: {
                      applies($0, to: projectDirectory)
                  }),
                  let installPath = entry["installPath"] as? String else { return nil }
            let name = id.lastIndex(of: "@").map { String(id[..<$0]) } ?? id
            guard !name.isEmpty else { return nil }
            let install = URL(fileURLWithPath: installPath).standardizedFileURL.path
            let manifest = object(at: install + "/.claude-plugin/plugin.json") ?? [:]
            return ComposerPlugin(
                name: name,
                skillRoots: roots(manifest["skills"], default: "./skills", in: install),
                commandRoots: roots(manifest["commands"], default: "./commands", in: install)
            )
        }
    }

    /// A project or local entry belongs to one project folder and its subfolders.
    private static func applies(_ entry: [String: Any], to project: String?) -> Bool {
        guard ["project", "local"].contains(entry["scope"] as? String) else { return true }
        guard let project, let path = entry["projectPath"] as? String else { return false }
        let folder = URL(fileURLWithPath: project).resolvingSymlinksInPath().standardized.path
        let owner = URL(fileURLWithPath: path).resolvingSymlinksInPath().standardized.path
        return folder == owner || folder.hasPrefix(owner + "/")
    }

    /// Paths in plugin.json are relative to the install folder and may not leave it.
    private static func roots(_ value: Any?, default fallback: String, in install: String) -> [String] {
        let listed = (value as? String).map { [$0] } ?? value as? [String] ?? [fallback]
        return listed.compactMap { path in
            let resolved = URL(fileURLWithPath: path, relativeTo: URL(fileURLWithPath: install))
                .standardized.path
            guard resolved.hasPrefix(install + "/"),
                  FileManager.default.fileExists(atPath: resolved) else { return nil }
            return resolved
        }
    }

    private static func object(at path: String) -> [String: Any]? {
        guard let data = FileManager.default.contents(atPath: path) else { return nil }
        return (try? JSONSerialization.jsonObject(with: data)) as? [String: Any]
    }
}
