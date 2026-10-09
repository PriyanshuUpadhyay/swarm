import Foundation
import Darwin

/// Holds the CLI data folder's app.lock descriptor; the kernel releases it if the app exits.
public final class AppRunLock: Sendable {
    public let file: URL
    private let handle: FileHandle

    public static func file(in folder: URL) -> URL {
        folder.appendingPathComponent("app.lock")
    }

    /// The caller first initializes the home through the CLI. The pid is informational only.
    public init(folder: URL, pid: Int32 = ProcessInfo.processInfo.processIdentifier) throws {
        guard pid > 0 else { throw CocoaError(.fileWriteInvalidFileName) }
        file = Self.file(in: folder)
        let descriptor = open(file.path, O_RDWR | O_CREAT | O_CLOEXEC, S_IRUSR | S_IWUSR)
        guard descriptor >= 0 else { throw POSIXError(POSIXErrorCode(rawValue: errno) ?? .EIO) }
        let opened = FileHandle(fileDescriptor: descriptor, closeOnDealloc: true)
        var retries = 0
        while flock(descriptor, LOCK_EX | LOCK_NB) != 0 {
            let code = errno
            guard code == EWOULDBLOCK, retries < 3 else {
                let error = POSIXError(POSIXErrorCode(rawValue: code) ?? .EIO)
                try? opened.close()
                throw error
            }
            // The CLI briefly holds LOCK_SH while probing; let that probe finish at app launch.
            retries += 1
            usleep(20_000)
        }
        do {
            // Keep the inode while locked; atomic replacement would leave the lock on the old file.
            try opened.truncate(atOffset: 0)
            try opened.write(contentsOf: Data("\(pid)\n".utf8))
        } catch {
            try? opened.close()
            throw error
        }
        handle = opened
    }

    public func release() throws { try handle.close() }

    deinit { try? handle.close() }
}
