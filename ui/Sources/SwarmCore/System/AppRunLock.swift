import Foundation

/// The pid file lives in the CLI-claimed data folder, just like paths::root_dir()/app.lock.
public struct AppRunLock: Sendable {
    public let file: URL
    private let pid: Int32

    public static func file(in folder: URL) -> URL {
        folder.appendingPathComponent("app.lock")
    }

    /// Writes atomically into an existing folder. The caller first initializes the home through the CLI.
    public init(folder: URL, pid: Int32 = ProcessInfo.processInfo.processIdentifier) throws {
        guard pid > 0 else { throw CocoaError(.fileWriteInvalidFileName) }
        self.file = Self.file(in: folder)
        self.pid = pid
        try Data("\(pid)\n".utf8).write(to: file, options: .atomic)
    }

    public func release() throws {
        guard FileManager.default.fileExists(atPath: file.path) else { return }
        // A second app may have replaced the pid; keep a lock whose pid differs.
        guard try String(contentsOf: file, encoding: .utf8).trimmingCharacters(in: .whitespacesAndNewlines) == "\(pid)" else { return }
        try FileManager.default.removeItem(at: file)
    }
}
