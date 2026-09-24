import Foundation

@MainActor
public final class SwarmProjectStore {
    private let defaults: UserDefaults
    private let key: String

    public init(defaults: UserDefaults = .standard, key: String = "projects.openPaths") {
        self.defaults = defaults
        self.key = key
    }

    public func paths() -> [String] {
        defaults.stringArray(forKey: key) ?? []
    }

    @discardableResult
    public func add(_ url: URL) throws -> String {
        let path = url.resolvingSymlinksInPath().standardizedFileURL.path
        var isDirectory: ObjCBool = false
        guard FileManager.default.fileExists(atPath: path, isDirectory: &isDirectory),
              isDirectory.boolValue else {
            throw SwarmProjectError.notDirectory(path)
        }
        var saved = paths()
        saved.removeAll { $0 == path }
        saved.append(path)
        defaults.set(saved, forKey: key)
        return path
    }

    @discardableResult
    public func create(at url: URL) throws -> String {
        let path = url.standardizedFileURL.path
        guard !FileManager.default.fileExists(atPath: path) else {
            throw SwarmProjectError.alreadyExists(path)
        }
        try FileManager.default.createDirectory(at: url, withIntermediateDirectories: false)
        return try add(url)
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
