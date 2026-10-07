import Foundation

@MainActor
public final class SwarmProjectStore {
    private let choices: OwnerChoicesStore

    public init(choicesFolder: URL? = SwarmHome.dataFolder) {
        choices = OwnerChoicesStore(folder: choicesFolder)
    }

    public func paths() -> [String] {
        (try? choices.load().projectPaths) ?? []
    }

    @discardableResult
    public func add(_ url: URL) async throws -> String {
        let path = try await Task.detached { try Self.directoryPath(url) }.value
        try remember(path)
        return path
    }

    /// Makes the folder and runs `git init` in it, so a workspace can be made there at once.
    @discardableResult
    public func create(at url: URL) async throws -> String {
        let path = try await Task.detached {
            let path = url.standardizedFileURL.path
            guard !FileManager.default.fileExists(atPath: path) else {
                throw SwarmProjectError.alreadyExists(path)
            }
            try FileManager.default.createDirectory(at: url, withIntermediateDirectories: false)
            return try Self.directoryPath(url)
        }.value
        do {
            try await Git.initialize(at: path)
        } catch {
            // Only the empty folder made above is removed; anything else there stays.
            if (try? FileManager.default.contentsOfDirectory(atPath: path))?.isEmpty == true {
                try? FileManager.default.removeItem(atPath: path)
            }
            throw error
        }
        try remember(path)
        return path
    }

    nonisolated private static func directoryPath(_ url: URL) throws -> String {
        let path = url.resolvingSymlinksInPath().standardizedFileURL.path
        var isDirectory: ObjCBool = false
        guard FileManager.default.fileExists(atPath: path, isDirectory: &isDirectory),
              isDirectory.boolValue else { throw SwarmProjectError.notDirectory(path) }
        return path
    }

    private func remember(_ path: String) throws {
        try choices.update {
            if !$0.projectPaths.contains(path) { $0.projectPaths.append(path) }
        }
    }
}

public enum SwarmProjectError: LocalizedError {
    case notDirectory(String)
    case alreadyExists(String)

    public var errorDescription: String? {
        switch self {
        case .notDirectory(let path): "No folder exists at \(path)"
        case .alreadyExists(let path): "A file or folder already exists at \(path)"
        }
    }
}
