import Foundation

public struct SkillInventoryEntry: Identifiable, Equatable, Sendable {
    public let key: String
    public let name: String
    public let qualifier: String?
    public let hasStepTable: Bool
    public var id: String { key }
}

public struct SkillInventory: Sendable {
    public let entries: [SkillInventoryEntry]

    public init(manifest: Data, root: URL) throws {
        struct Manifest: Decodable {
            struct File: Decodable { let path: String }
            let files: [File]
        }
        let files = try JSONDecoder().decode(Manifest.self, from: manifest).files
        let keys = files.map(\.path).filter { $0.hasSuffix("/SKILL.md") || $0 == "SKILL.md" }
        let named = keys.map { key in (key: key, name: URL(fileURLWithPath: key).deletingLastPathComponent().lastPathComponent) }
        let counts = Dictionary(named.map { ($0.name, 1) }, uniquingKeysWith: +)
        let canonicalRoot = root.resolvingSymlinksInPath().standardizedFileURL.path
        entries = try named.map { entry in
            let components = entry.key.split(separator: "/", omittingEmptySubsequences: false)
            guard !entry.key.hasPrefix("/"), components.allSatisfy({ !$0.isEmpty && $0 != "." && $0 != ".." }) else {
                throw CocoaError(.fileReadInvalidFileName)
            }
            let target = root.appendingPathComponent(entry.key).resolvingSymlinksInPath().standardizedFileURL
            guard target.path.hasPrefix(canonicalRoot + "/") else { throw CocoaError(.fileReadInvalidFileName) }
            let data = try Data(contentsOf: target)
            let document = SkillDocument.parse(data: data, key: entry.key)
            return SkillInventoryEntry(key: entry.key, name: entry.name,
                                       qualifier: counts[entry.name, default: 0] > 1 ? String(entry.key.dropLast("/SKILL.md".count)) : nil,
                                       hasStepTable: document.tableRange != nil)
        }.sorted { $0.name == $1.name ? $0.key < $1.key : $0.name < $1.name }
    }
}
