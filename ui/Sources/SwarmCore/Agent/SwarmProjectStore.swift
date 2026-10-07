import Foundation

@MainActor
public final class SwarmProjectStore {
    private let choices: OwnerChoicesStore
    private let initializeRepository: (String) async throws -> Void
    private var lastRefreshError: String?

    public convenience init(choicesFolder: URL? = SwarmHome.dataFolder) {
        self.init(choices: OwnerChoicesStore(folder: choicesFolder))
    }

    init(choices: OwnerChoicesStore, initializeRepository: @escaping (String) async throws -> Void = {
        try await Git.initialize(at: $0)
    }) {
        self.choices = choices
        self.initializeRepository = initializeRepository
    }

    public func loadChoices(reportError: (String) -> Void) -> OwnerChoices? {
        do { return try choices.load() }
        catch {
            report(error, using: reportError)
            return nil
        }
    }

    private func report(_ error: any Error, using reportError: (String) -> Void) {
        let failure = error.localizedDescription
        if failure != lastRefreshError { reportError(failure) }
        lastRefreshError = failure
    }

    public func paths() -> [String] {
        (try? choices.load().projectPaths) ?? []
    }

    public func removedPaths() -> Set<String> {
        (try? choices.load().removedProjects) ?? []
    }

    public func rememberShown(_ projects: [ProjectNode]) throws {
        try choices.update { Self.rememberShown(projects, in: &$0) }
    }

    private static func rememberShown(_ projects: [ProjectNode], in saved: inout OwnerChoices) {
        for project in projects where !saved.removedProjects.contains(project.path) {
            let path: String
            if case .repository(let commonDirectory) = project.id, commonDirectory == project.path {
                path = project.launchDirectory
            } else {
                path = project.path
            }
            if !saved.projectPaths.contains(path) { saved.projectPaths.append(path) }
        }
    }

    public func refreshChoices(
        shown: [ProjectNode], navigation: WorkspaceNavigation, saved: OwnerChoices? = nil,
        navigationStore: WorkspaceNavigationStore, reportError: (String) -> Void
    ) -> WorkspaceNavigation {
        do {
            let snapshot = try saved ?? choices.load()
            var updated = snapshot
            Self.rememberShown(shown, in: &updated)
            if updated != snapshot {
                updated = try choices.update { Self.rememberShown(shown, in: &$0) }
            }
            lastRefreshError = nil
            return navigationStore.applying(updated, to: navigation)
        } catch {
            report(error, using: reportError)
            return navigation
        }
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
        var addedPath = false
        do {
            addedPath = try remember(path, projectPath: path)
            try await initializeRepository(path)
        } catch {
            // Only the empty folder made above is removed; anything else there stays.
            if (try? FileManager.default.contentsOfDirectory(atPath: path))?.isEmpty == true {
                try? FileManager.default.removeItem(atPath: path)
                if addedPath, !FileManager.default.fileExists(atPath: path) {
                    try choices.update { $0.projectPaths.removeAll { $0 == path } }
                }
            }
            throw error
        }
        return path
    }

    nonisolated private static func directoryPath(_ url: URL) throws -> String {
        let path = url.resolvingSymlinksInPath().standardizedFileURL.path
        var isDirectory: ObjCBool = false
        guard FileManager.default.fileExists(atPath: path, isDirectory: &isDirectory),
              isDirectory.boolValue else { throw SwarmProjectError.notDirectory(path) }
        return path
    }

    @discardableResult
    private func remember(_ path: String, projectPath: String? = nil) throws -> Bool {
        let project = projectPath ?? Self.projectPath(for: path)
        var addedPath = false
        try choices.update {
            $0.removedProjects.remove(project)
            $0.removedProjects.remove(path)
            if !$0.projectPaths.contains(path) {
                $0.projectPaths.append(path)
                addedPath = true
            }
        }
        return addedPath
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
