import Foundation

public enum GitTaskWorktree {
    public static func create(
        named name: String, in repositoryDirectory: String,
        commonDirectory: String, under parentDirectory: String
    ) async throws -> String {
        let title = name.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !title.isEmpty else { throw GitTaskWorktreeError.emptyName }
        let slug = title.lowercased().unicodeScalars.reduce(into: "") { result, scalar in
            if scalar.isASCII, CharacterSet.alphanumerics.contains(scalar) {
                result.unicodeScalars.append(scalar)
            } else if !result.isEmpty, !result.hasSuffix("-") {
                result.append("-")
            }
        }.trimmingCharacters(in: CharacterSet(charactersIn: "-"))
        let prefix = String((slug.isEmpty ? "task" : slug).prefix(40))
        let stem = "\(prefix)-\(UUID().uuidString.lowercased().prefix(8))"
        let branch = "swarm/\(stem)"
        let base = try await defaultReference(in: commonDirectory)
        let parent = URL(fileURLWithPath: parentDirectory).standardizedFileURL
        try FileManager.default.createDirectory(at: parent, withIntermediateDirectories: true)
        let path = parent.appendingPathComponent(stem, isDirectory: true).path
        _ = try await Git.checkRaw(
            ["worktree", "add", "-b", branch, path, base], in: repositoryDirectory
        )
        guard let created = try await Git.worktrees(of: repositoryDirectory).first(where: { $0.branch == branch }) else {
            throw GitTaskWorktreeError.notListed(branch)
        }
        return created.path
    }

    private static func defaultReference(in commonDirectory: String) async throws -> String {
        let remote = try await Git.runRaw(
            ["symbolic-ref", "--quiet", "refs/remotes/origin/HEAD"], in: commonDirectory
        )
        let remoteRef = String(decoding: remote.stdout, as: UTF8.self)
            .trimmingCharacters(in: .whitespacesAndNewlines)
        if remote.ok, remoteRef.hasPrefix("refs/remotes/origin/") {
            let local = "refs/heads/" + remoteRef.dropFirst("refs/remotes/origin/".count)
            if try await hasReference(local, in: commonDirectory) { return local }
            return remoteRef
        }
        for branch in ["main", "master"] {
            let ref = "refs/heads/" + branch
            if try await hasReference(ref, in: commonDirectory) { return ref }
        }
        // Repositories without a known default branch fall back to their primary HEAD.
        let head = try await Git.runRaw(["symbolic-ref", "--quiet", "HEAD"], in: commonDirectory)
        if head.ok {
            return String(decoding: head.stdout, as: UTF8.self)
                .trimmingCharacters(in: .whitespacesAndNewlines)
        }
        let commit = try await Git.checkRaw(["rev-parse", "HEAD"], in: commonDirectory)
        return String(decoding: commit.stdout, as: UTF8.self)
            .trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private static func hasReference(_ ref: String, in directory: String) async throws -> Bool {
        try await Git.runRaw(["show-ref", "--verify", "--quiet", ref], in: directory).ok
    }
}

public enum GitTaskWorktreeError: LocalizedError {
    case emptyName
    case notRepository
    case notListed(String)

    public var errorDescription: String? {
        switch self {
        case .emptyName: "Enter a task name"
        case .notRepository: "New Task needs a Git project"
        case .notListed(let branch): "Git created \(branch), but did not list its worktree"
        }
    }
}
