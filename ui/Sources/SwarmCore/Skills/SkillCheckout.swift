import Darwin
import Foundation
import Synchronization

public enum SkillSaveError: Error, Equatable, Sendable, LocalizedError {
    case conflict(path: String)
    case io(path: String, reason: String)
    case path(path: String, reason: String)

    public var errorDescription: String? {
        switch self {
        case .conflict(let path): "This skill changed on disk. Reload before saving. \(path)"
        case .io(let path, let reason): "Could not read or save \(path). \(reason)"
        case .path(let path, let reason): "Invalid skill checkout path \(path). \(reason)"
        }
    }
}

public struct SkillCheckout: Sendable {
    public let path: String
    private let keys: Set<String>
    private let protectedRoots: [URL]
    private var beforeRecheck: (@Sendable () -> Void)?
    private static let locks = Mutex<[String: NSLock]>([:])

    public static var defaultProtectedRoots: [URL] {
        [Bundle.main.bundleURL] + (SwarmHome.dataFolder.map { [$0.appendingPathComponent("skills")] } ?? [])
    }

    public init(path: String, inventory: SkillInventory, protectedRoots: [URL] = SkillCheckout.defaultProtectedRoots) throws {
        self.path = try Self.validate(path: path, protectedRoots: protectedRoots)
        keys = Set(inventory.entries.map(\.key))
        self.protectedRoots = protectedRoots
    }

    init(path: String, inventory: SkillInventory, protectedRoots: [URL], beforeRecheck: @escaping @Sendable () -> Void) throws {
        try self.init(path: path, inventory: inventory, protectedRoots: protectedRoots)
        self.beforeRecheck = beforeRecheck
    }

    public static func validate(path: String, protectedRoots: [URL] = SkillCheckout.defaultProtectedRoots) throws -> String {
        guard path.hasPrefix("/"), !path.utf8.contains(0) else {
            throw SkillSaveError.path(path: path, reason: "Choose an absolute directory path.")
        }
        let root = URL(fileURLWithPath: path).resolvingSymlinksInPath().standardizedFileURL
        try rejectProtected(root, protectedRoots: protectedRoots)
        let rootValues = try values(root)
        guard rootValues.isDirectory == true else { throw SkillSaveError.path(path: path, reason: "The checkout is not a directory.") }
        let git = try values(root.appendingPathComponent(".git"))
        guard git.isDirectory == true || git.isRegularFile == true else {
            throw SkillSaveError.path(path: path, reason: "The checkout needs a .git file or directory.")
        }
        let skills = root.appendingPathComponent("skills/kit/skills").resolvingSymlinksInPath().standardizedFileURL
        guard isInside(skills, root: root), try values(skills).isDirectory == true else {
            throw SkillSaveError.path(path: skills.path, reason: "The checkout needs skills/kit/skills inside its root.")
        }
        try rejectProtected(skills, protectedRoots: protectedRoots)
        return root.path
    }

    public func targetURL(key: String) throws -> URL {
        let parts = key.split(separator: "/", omittingEmptySubsequences: false)
        guard keys.contains(key), parts.count == 4, parts[0] == "kit", parts[1] == "skills", parts[3] == "SKILL.md",
              !parts[2].isEmpty, parts[2] != ".", parts[2] != "..", !parts[2].contains("\\"), !parts[2].utf8.contains(0) else {
            throw SkillSaveError.path(path: key, reason: "Only kit skills from the bundled inventory can be saved.")
        }
        let root = URL(fileURLWithPath: try Self.validate(path: path, protectedRoots: protectedRoots))
        let subtree = root.appendingPathComponent("skills/kit/skills").resolvingSymlinksInPath().standardizedFileURL
        let target = root.appendingPathComponent("skills/" + key).resolvingSymlinksInPath().standardizedFileURL
        guard Self.isInside(target, root: root), Self.isInside(target, root: subtree) else {
            throw SkillSaveError.path(path: target.path, reason: "The target escapes skills/kit/skills.")
        }
        try Self.rejectProtected(target, protectedRoots: protectedRoots)
        guard try Self.values(target).isRegularFile == true else {
            throw SkillSaveError.path(path: target.path, reason: "The skill source must be an existing regular file.")
        }
        return target
    }

    public func load(key: String) throws -> SkillDocument {
        let target = try targetURL(key: key)
        return SkillDocument.parse(data: try read(target), key: key, sourceURL: target)
    }

    public func save(key: String, candidate: Data, expectedRevision: String) throws -> String {
        let target = try targetURL(key: key)
        let lock = Self.locks.withLock { values in
            if let lock = values[target.path] { return lock }
            let lock = NSLock()
            values[target.path] = lock
            return lock
        }
        lock.lock()
        defer { lock.unlock() }
        try recheckTarget(key: key, target: target)
        let original = try read(target)
        guard SkillDocument.revision(of: original) == expectedRevision else { throw SkillSaveError.conflict(path: target.path) }
        guard original != candidate else { return expectedRevision }
        let mode: mode_t
        do {
            let attributes = try FileManager.default.attributesOfItem(atPath: target.path)
            guard let permissions = attributes[.posixPermissions] as? NSNumber else {
                throw SkillSaveError.io(path: target.path, reason: "The file mode is unavailable.")
            }
            mode = mode_t(permissions.uint16Value)
        } catch let error as SkillSaveError { throw error }
        catch { throw SkillSaveError.io(path: target.path, reason: error.localizedDescription) }
        let stage = target.deletingLastPathComponent().appendingPathComponent(".skill-save-\(UUID().uuidString).tmp")
        let descriptor = open(stage.path, O_WRONLY | O_CREAT | O_EXCL | O_NOFOLLOW | O_CLOEXEC, 0o600)
        guard descriptor >= 0 else { throw posixError(target) }
        // Only this operation's unique stage file is removed, including on partial writes.
        defer { close(descriptor); unlink(stage.path) }
        try candidate.withUnsafeBytes { buffer in
            var offset = 0
            while offset < buffer.count {
                let count = Darwin.write(descriptor, buffer.baseAddress!.advanced(by: offset), buffer.count - offset)
                if count < 0 && errno == EINTR { continue }
                guard count > 0 else { throw posixError(target) }
                offset += count
            }
        }
        guard fchmod(descriptor, mode) == 0 else { throw posixError(target) }
        beforeRecheck?()
        try recheckTarget(key: key, target: target)
        guard SkillDocument.revision(of: try read(target)) == expectedRevision else { throw SkillSaveError.conflict(path: target.path) }
        guard Darwin.rename(stage.path, target.path) == 0 else { throw posixError(target) }
        return SkillDocument.revision(of: candidate)
    }

    private func recheckTarget(key: String, target: URL) throws {
        guard try targetURL(key: key) == target else {
            throw SkillSaveError.path(path: target.path, reason: "The target link changed during Save.")
        }
    }

    private func read(_ target: URL) throws -> Data {
        do { return try Data(contentsOf: target) }
        catch { throw SkillSaveError.io(path: target.path, reason: error.localizedDescription) }
    }

    private func posixError(_ target: URL) -> SkillSaveError {
        .io(path: target.path, reason: NSError(domain: NSPOSIXErrorDomain, code: Int(errno)).localizedDescription)
    }

    private static func values(_ url: URL) throws -> URLResourceValues {
        do { return try url.resourceValues(forKeys: [.isDirectoryKey, .isRegularFileKey]) }
        catch { throw SkillSaveError.path(path: url.path, reason: error.localizedDescription) }
    }

    private static func isInside(_ target: URL, root: URL) -> Bool {
        target.path.hasPrefix(root.path.hasSuffix("/") ? root.path : root.path + "/")
    }

    private static func rejectProtected(_ target: URL, protectedRoots: [URL]) throws {
        for root in protectedRoots.map({ $0.resolvingSymlinksInPath().standardizedFileURL }) where target == root || isInside(target, root: root) {
            throw SkillSaveError.path(path: target.path, reason: "The app bundle and refreshed skills home are read-only.")
        }
    }
}
