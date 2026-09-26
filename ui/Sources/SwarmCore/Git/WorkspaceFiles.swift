import Darwin
import Foundation

public struct WorkspaceFileEntry: Identifiable, Sendable {
    public enum Kind: Sendable { case directory, file, symbolicLink, other }
    public let path: String
    public let name: String
    public let kind: Kind
    public var id: String { path }
}

public struct WorkspaceFileListing: Sendable {
    public let entries: [WorkspaceFileEntry]
    public let truncated: Bool
}

public enum WorkspaceFilePreview: Sendable, Equatable {
    case text(String)
    case notice(String)
}

public enum WorkspaceFiles {
    /// Lists one directory without following symbolic links. Git metadata is omitted.
    /// At most 2,000 entries are returned; truncated listings explicitly report that limit.
    public static func list(in root: String, path: String = "") async throws -> WorkspaceFileListing {
        try withDirectory(root: root, parts: components(path, allowRoot: true)) { descriptor in
            let copy = dup(descriptor)
            guard copy >= 0 else { throw WorkspaceReadError("The directory cannot be read.") }
            guard let directory = fdopendir(copy) else {
                close(copy)
                throw WorkspaceReadError("The directory cannot be read.")
            }
            defer { closedir(directory) }
            var entries: [WorkspaceFileEntry] = []
            while true {
                try Task.checkCancellation()
                errno = 0
                guard let entry = readdir(directory) else {
                    if errno != 0 { throw WorkspaceReadError("The directory changed or cannot be read.") }
                    break
                }
                let name = withUnsafePointer(to: &entry.pointee.d_name) {
                    $0.withMemoryRebound(to: CChar.self, capacity: Int(entry.pointee.d_namlen) + 1) { String(cString: $0) }
                }
                guard name != ".", name != "..", name != ".git" else { continue }
                if entries.count == 2_000 { return listing(entries, truncated: true) }
                var info = stat()
                guard fstatat(descriptor, name, &info, AT_SYMLINK_NOFOLLOW) == 0 else { continue }
                let kind: WorkspaceFileEntry.Kind
                switch info.st_mode & S_IFMT {
                case S_IFDIR: kind = .directory
                case S_IFREG: kind = .file
                case S_IFLNK: kind = .symbolicLink
                default: kind = .other
                }
                entries.append(WorkspaceFileEntry(path: path.isEmpty ? name : path + "/" + name, name: name, kind: kind))
            }
            return listing(entries, truncated: false)
        }
    }

    /// Returns UTF-8 text up to 256 KiB, or a notice. Never follows symbolic links.
    public static func preview(in root: String, path: String) async throws -> WorkspaceFilePreview {
        try read(in: root, path: path)
    }

    static func read(in root: String, path: String) throws -> WorkspaceFilePreview {
        let parts = try components(path)
        return try withDirectory(root: root, parts: Array(parts.dropLast())) { directory in
            let descriptor = openat(directory, parts[parts.count - 1], O_RDONLY | O_CLOEXEC | O_NOFOLLOW | O_NONBLOCK)
            guard descriptor >= 0 else { return .notice("Preview unavailable. The file changed or is a symbolic link.") }
            defer { close(descriptor) }
            var info = stat()
            guard fstat(descriptor, &info) == 0, (info.st_mode & S_IFMT) == S_IFREG else {
                return .notice("Only regular files have a text preview.")
            }
            let limit = 256 * 1024
            guard info.st_size <= limit else { return .notice("This file exceeds the 256 KiB preview limit.") }
            let file = FileHandle(fileDescriptor: descriptor, closeOnDealloc: false)
            var data = Data()
            while data.count <= limit {
                try Task.checkCancellation()
                let chunk = try file.read(upToCount: limit + 1 - data.count) ?? Data()
                if chunk.isEmpty { break }
                data.append(chunk)
            }
            guard data.count <= limit else { return .notice("This file exceeds the 256 KiB preview limit.") }
            guard !data.contains(0), let text = String(data: data, encoding: .utf8) else {
                return .notice("Binary file. No text preview.")
            }
            return .text(text)
        }
    }

    private static func listing(_ entries: [WorkspaceFileEntry], truncated: Bool) -> WorkspaceFileListing {
        WorkspaceFileListing(entries: entries.sorted {
            if ($0.kind == .directory) != ($1.kind == .directory) { return $0.kind == .directory }
            return $0.name.localizedStandardCompare($1.name) == .orderedAscending
        }, truncated: truncated)
    }

    private static func components(_ path: String, allowRoot: Bool = false) throws -> [String] {
        if allowRoot, path.isEmpty { return [] }
        let parts = path.split(separator: "/", omittingEmptySubsequences: false).map(String.init)
        guard !parts.isEmpty, parts.allSatisfy({ !$0.isEmpty && $0 != "." && $0 != ".." && $0 != ".git" && !$0.utf8.contains(0) }) else {
            throw WorkspaceReadError("This path cannot be previewed.")
        }
        return parts
    }

    private static func withDirectory<T>(root: String, parts: [String], read: (Int32) throws -> T) throws -> T {
        guard !root.utf8.contains(0) else { throw WorkspaceReadError("The workspace path is invalid.") }
        var descriptor = open(root, O_RDONLY | O_DIRECTORY | O_CLOEXEC | O_NOFOLLOW)
        guard descriptor >= 0 else { throw WorkspaceReadError("The workspace directory cannot be opened.") }
        defer { close(descriptor) }
        // Hold every parent directory while opening the next component, so path swaps cannot escape the workspace.
        for part in parts {
            let next = openat(descriptor, part, O_RDONLY | O_DIRECTORY | O_CLOEXEC | O_NOFOLLOW)
            guard next >= 0 else { throw WorkspaceReadError("The directory changed or is a symbolic link.") }
            close(descriptor)
            descriptor = next
        }
        return try read(descriptor)
    }
}
