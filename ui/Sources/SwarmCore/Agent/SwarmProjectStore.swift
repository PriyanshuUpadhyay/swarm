import Foundation

@MainActor
public final class SwarmProjectStore {
    private let choices: OwnerChoicesStore
    private var lastRefreshError: String?

    public init(choicesFolder: URL? = SwarmHome.dataFolder) {
        choices = OwnerChoicesStore(folder: choicesFolder)
    }

    public func paths() -> [String] {
        (try? choices.load().projectPaths) ?? []
    }

    public func removedPaths() -> Set<String> {
        (try? choices.load().removedProjects) ?? []
    }

    public func rememberShown(_ projects: [ProjectNode]) throws {
        try choices.update { saved in
            for project in projects where !saved.removedProjects.contains(project.path) {
                if !saved.projectPaths.contains(project.path) { saved.projectPaths.append(project.path) }
            }
        }
    }

    public func refreshChoices(
        shown: [ProjectNode], navigation: WorkspaceNavigation,
        navigationStore: WorkspaceNavigationStore, reportError: (String) -> Void
    ) -> WorkspaceNavigation {
        var failures: [String] = []
        do { try rememberShown(shown) }
        catch { failures.append(error.localizedDescription) }
        var refreshed = navigation
        do {
            if failures.isEmpty { refreshed = try navigationStore.reloadChoices(navigation) }
        }
        catch {
            let message = error.localizedDescription
            if !failures.contains(message) { failures.append(message) }
        }
        let failure = failures.isEmpty ? nil : failures.joined(separator: "\n")
        if let failure, failure != lastRefreshError { reportError(failure) }
        lastRefreshError = failure
        return refreshed
    }

    public func remove(_ path: String) throws {
        try choices.update {
            $0.projectPaths.removeAll { $0 == path || Self.projectPath(for: $0) == path }
            $0.removedProjects.insert(path)
            $0.removeProject(path)
        }
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
            $0.removedProjects.remove(Self.projectPath(for: path))
            $0.removedProjects.remove(path)
            if !$0.projectPaths.contains(path) { $0.projectPaths.append(path) }
        }
    }

    private static func projectPath(for path: String) -> String {
        ProjectNode.projectPath(for: SwarmSessionDiscovery.identity(
            for: path, repositoryPathsResolver: Git.repositoryPaths
        ))
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
