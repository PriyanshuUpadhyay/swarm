import Darwin
import Foundation

public struct OwnerChoices: Codable, Equatable, Sendable {
    public var pinned: Set<String> = []
    public var archived: Set<String> = []
    public var names: [String: String] = [:]
    public var chatNames: [String: String] = [:]
    public var projectNames: [String: String] = [:]
    public var projectPaths: [String] = []
    public var removedProjects: Set<String> = []
    public var workspaceOrder: [String: [String]] = [:]

    public var fields = RowFieldLists()

    public init() {}

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        pinned = try container.decodeIfPresent(Set<String>.self, forKey: .pinned) ?? []
        archived = try container.decodeIfPresent(Set<String>.self, forKey: .archived) ?? []
        names = try container.decodeIfPresent([String: String].self, forKey: .names) ?? [:]
        chatNames = try container.decodeIfPresent([String: String].self, forKey: .chatNames) ?? [:]
        projectNames = try container.decodeIfPresent([String: String].self, forKey: .projectNames) ?? [:]
        projectPaths = try container.decodeIfPresent([String].self, forKey: .projectPaths) ?? []
        removedProjects = try container.decodeIfPresent(Set<String>.self, forKey: .removedProjects) ?? []
        workspaceOrder = try container.decodeIfPresent([String: [String]].self, forKey: .workspaceOrder) ?? [:]
        fields = try container.decodeIfPresent(RowFieldLists.self, forKey: .fields) ?? RowFieldLists()
    }

    /// A missing folder can be offline. Only Remove Project clears its saved choices.
    public mutating func removeProject(_ path: String) {
        let ordered = Set(workspaceOrder[path] ?? [])
        func belongsToProject(_ key: String) -> Bool {
            key == path || key == path + "#removed" || key.hasPrefix(path + "/") || ordered.contains(key)
        }
        pinned = pinned.filter { !belongsToProject($0) }
        archived = archived.filter { !belongsToProject($0) }
        names = names.filter { !belongsToProject($0.key) }
        projectNames.removeValue(forKey: path)
        workspaceOrder.removeValue(forKey: path)
    }

    static func folderExists(_ path: String) -> Bool {
        var isDirectory: ObjCBool = false
        return FileManager.default.fileExists(atPath: path, isDirectory: &isDirectory) && isDirectory.boolValue
    }
}

@MainActor
public final class OwnerChoicesStore {
    private let folder: URL?

    public init(folder: URL? = SwarmHome.dataFolder) { self.folder = folder }

    public func load() throws -> OwnerChoices {
        guard let folder, isClaimed(folder) else { return OwnerChoices() }
        return try withLock(in: folder) { try readUnlocked(in: folder) }
    }

    private func readUnlocked(in folder: URL) throws -> OwnerChoices {
        let file = folder.appendingPathComponent("choices.json")
        let data: Data
        do {
            data = try Data(contentsOf: file)
        } catch CocoaError.fileReadNoSuchFile {
            return OwnerChoices()
        }
        do {
            return try JSONDecoder().decode(OwnerChoices.self, from: data)
        } catch {
            // A failed move stops the write too, so unread choices are never replaced.
            let timestamp = Int(Date().timeIntervalSince1970)
            var backup = folder.appendingPathComponent("choices.json.bad-\(timestamp)")
            var suffix = 0
            while FileManager.default.fileExists(atPath: backup.path) {
                suffix += 1
                backup = folder.appendingPathComponent("choices.json.bad-\(timestamp)-\(suffix)")
            }
            try FileManager.default.moveItem(at: file, to: backup)
            return OwnerChoices()
        }
    }

    @discardableResult
    public func update(_ change: (inout OwnerChoices) -> Void) throws -> OwnerChoices {
        guard let folder else { throw OwnerChoicesError.emptyHome }
        guard isClaimed(folder) else { throw OwnerChoicesError.unclaimedHome(folder.path) }
        return try withLock(in: folder) {
            var choices = try readUnlocked(in: folder)
            let previous = choices
            change(&choices)
            guard choices != previous else { return choices }
            let encoder = JSONEncoder()
            encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
            try encoder.encode(choices).write(to: folder.appendingPathComponent("choices.json"), options: .atomic)
            return choices
        }
    }

    private func isClaimed(_ folder: URL) -> Bool {
        var isDirectory: ObjCBool = false
        return FileManager.default.fileExists(
            atPath: folder.appendingPathComponent("swarm-home").path, isDirectory: &isDirectory
        ) && !isDirectory.boolValue
    }

    private func withLock<T>(in folder: URL, _ body: () throws -> T) throws -> T {
        let descriptor = open(folder.appendingPathComponent("choices.lock").path, O_CREAT | O_RDWR | O_CLOEXEC, 0o600)
        guard descriptor >= 0 else { throw NSError(domain: NSPOSIXErrorDomain, code: Int(errno)) }
        defer { close(descriptor) }
        while flock(descriptor, LOCK_EX) != 0 {
            guard errno == EINTR else { throw NSError(domain: NSPOSIXErrorDomain, code: Int(errno)) }
        }
        defer { _ = flock(descriptor, LOCK_UN) }
        return try body()
    }

}

public enum OwnerChoicesError: LocalizedError {
    case emptyHome
    case unclaimedHome(String)

    public var errorDescription: String? {
        switch self {
        case .emptyHome: "SWARM_HOME is set but empty"
        case .unclaimedHome(let path): "Sidebar choices were not saved. Run swarm init with SWARM_HOME set to \(path), then try again."
        }
    }
}
