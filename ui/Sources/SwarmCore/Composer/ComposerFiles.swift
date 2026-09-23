import Foundation

public struct ComposerMentionSource: Equatable, Sendable {
    public var root: String

    public init(root: String) {
        self.root = root
    }
}

public struct ComposerFileMatch: Identifiable, Hashable, Sendable {
    public var path: String
    public var score: Int
    public var id: String { path }

    public init(path: String, score: Int) {
        self.path = path
        self.score = score
    }

    public var name: String { (path as NSString).lastPathComponent }
    public var directory: String { (path as NSString).deletingLastPathComponent }
}

/// The ranking is adapted from Bloom's FileMatch.
public enum ComposerFileCatalog {
    public static func discover(from source: ComposerMentionSource) async -> [String] {
        if let result = try? await Shell.run(
            "git", ["ls-files", "-co", "--exclude-standard"],
            cwd: source.root, timeout: .seconds(10), outputLimit: 4 * 1_024 * 1_024
        ), result.ok {
            return result.stdout.split(whereSeparator: \.isNewline).map(String.init).sorted()
        }
        return walk(root: source.root)
    }

    public static func matches(
        _ paths: [String], query: String, limit: Int = 12
    ) -> [ComposerFileMatch] {
        guard !query.isEmpty else {
            return paths.prefix(limit).map { ComposerFileMatch(path: $0, score: 0) }
        }
        var found: [ComposerFileMatch] = []
        for path in paths {
            guard let score = composerFuzzyScore(path, query: query) else { continue }
            let name = (path as NSString).lastPathComponent
            let nameScore = composerFuzzyScore(name, query: query) ?? 0
            found.append(ComposerFileMatch(path: path, score: score + nameScore))
        }
        found.sort {
            if $0.score != $1.score { return $0.score > $1.score }
            if $0.path.count != $1.path.count { return $0.path.count < $1.path.count }
            return $0.path < $1.path
        }
        return Array(found.prefix(limit))
    }

    private static func walk(root: String) -> [String] {
        let rootURL = URL(fileURLWithPath: root).standardizedFileURL
        let keys: [URLResourceKey] = [.isDirectoryKey, .isRegularFileKey, .isSymbolicLinkKey]
        guard let enumerator = FileManager.default.enumerator(
            at: rootURL, includingPropertiesForKeys: keys,
            options: [.skipsHiddenFiles, .skipsPackageDescendants]
        ) else { return [] }
        let skipped = Set([".git", ".build", "node_modules", "DerivedData"])
        var paths: [String] = []
        for case let url as URL in enumerator {
            let values = try? url.resourceValues(forKeys: Set(keys))
            let normalized = url.standardizedFileURL.path
            guard normalized.hasPrefix(rootURL.path + "/") else { continue }
            let relative = String(normalized.dropFirst(rootURL.path.count + 1))
            if relative.split(separator: "/").count > 8 {
                if values?.isDirectory == true { enumerator.skipDescendants() }
                continue
            }
            if values?.isDirectory == true, skipped.contains(url.lastPathComponent) {
                enumerator.skipDescendants()
                continue
            }
            guard values?.isRegularFile == true, values?.isSymbolicLink != true else { continue }
            paths.append(relative)
            if paths.count == 4_000 { break }
        }
        return paths.sorted()
    }
}
