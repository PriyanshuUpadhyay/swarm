import Foundation

public struct WorkspaceReadError: Error, Sendable, CustomStringConvertible {
    public let description: String
    public init(_ description: String) { self.description = description }
}

public enum GitChangeLayer: String, CaseIterable, Sendable {
    case conflicted = "Conflicts"
    case staged = "Staged"
    case unstaged = "Unstaged"
    case untracked = "Untracked"
    case branch = "Committed"
}

public struct GitChange: Identifiable, Sendable, Equatable {
    public let path: String
    public let status: String
    public let layer: GitChangeLayer
    public var id: String { layer.rawValue + ":" + path }
}

public struct GitWorkspaceSnapshot: Sendable {
    public let root: String
    public let branch: String?
    public let head: String?
    public let files: [GitChange]
    public let refs: [String]
    public let readAt: Date
    public var branchLabel: String { branch ?? head.map { "Detached at \($0.prefix(8))" } ?? "No commits" }
}

public struct GitBranchComparison: Sendable {
    public let baseRef: String
    public let baseOID: String
    public let mergeBaseOID: String
    public let headOID: String
    public let files: [GitChange]
    public let readAt: Date
}

/// A single user-requested read shares one deadline across all subprocesses.
struct WorkspaceReadCommands: Sendable {
    let directory: String
    let deadline: ContinuousClock.Instant

    init(directory: String, seconds: Int = 20) {
        self.directory = directory
        self.deadline = .now.advanced(by: .seconds(seconds))
    }

    func remaining(maximum: Duration) throws -> Duration {
        try Task.checkCancellation()
        let left = ContinuousClock.now.duration(to: deadline)
        guard left > .zero else { throw WorkspaceReadError("The read timed out. Try Refresh.") }
        return min(left, maximum)
    }

    func git(_ arguments: [String], accepting: Set<Int32> = [0]) async throws -> GitOutput {
        let output = try await Git.runRaw(
            ["--literal-pathspecs"] + arguments, in: directory,
            timeout: remaining(maximum: .seconds(5)), outputLimit: 1024 * 1024
        )
        guard accepting.contains(output.status) else {
            throw Git.error(arguments, output.status, output.stderr, "")
        }
        return output
    }

    func text(_ arguments: [String]) async throws -> String {
        let output = try await git(arguments)
        guard var text = String(data: output.stdout, encoding: .utf8) else {
            throw WorkspaceReadError("Git returned text that is not valid UTF-8.")
        }
        if text.hasSuffix("\n") { text.removeLast() }
        return text
    }

    func identity() async throws -> (branch: String?, head: String?) {
        let branch = try await git(["symbolic-ref", "--quiet", "--short", "HEAD"], accepting: [0, 1])
        let head = try await git(["rev-parse", "--verify", "--quiet", "HEAD"], accepting: [0, 1])
        func value(_ result: GitOutput) -> String? {
            result.status == 0 ? String(decoding: result.stdout, as: UTF8.self).trimmingCharacters(in: .newlines) : nil
        }
        guard branch.status == 0 || head.status == 0 else {
            throw WorkspaceReadError("Git could not read HEAD.")
        }
        return (value(branch), value(head))
    }
}

extension Git {
    public static func inspect(in directory: String) async throws -> GitWorkspaceSnapshot {
        let commands = WorkspaceReadCommands(directory: directory)
        let root = try await commands.text(["rev-parse", "--show-toplevel"])
        let identity = try await commands.identity()
        let status = try await commands.git(["status", "--porcelain=v1", "-z", "--no-renames", "--untracked-files=all"])
        let refs = try await commands.text(["for-each-ref", "--format=%(refname)", "refs/heads", "refs/remotes"])
        guard try await commands.identity() == identity else {
            throw WorkspaceReadError("The branch changed during the read. Try Refresh.")
        }
        return GitWorkspaceSnapshot(
            root: root, branch: identity.branch, head: identity.head,
            files: try parseLocalChanges(status.stdout),
            refs: refs.split(separator: "\n").map(String.init), readAt: Date()
        )
    }

    static func parseLocalChanges(_ data: Data) throws -> [GitChange] {
        var changes: [GitChange] = []
        for entry in data.split(separator: 0) {
            guard entry.count >= 4, entry[entry.startIndex + 2] == 32,
                  let path = String(data: entry.dropFirst(3), encoding: .utf8) else {
                throw WorkspaceReadError("Git returned an unreadable file status.")
            }
            let status = String(decoding: entry.prefix(2), as: UTF8.self)
            if status == "??" {
                changes.append(GitChange(path: path, status: "?", layer: .untracked))
            } else if ["DD", "AU", "UD", "UA", "DU", "AA", "UU"].contains(status) {
                changes.append(GitChange(path: path, status: status, layer: .conflicted))
            } else {
                if status.first != " " { changes.append(GitChange(path: path, status: String(status.prefix(1)), layer: .staged)) }
                if status.last != " " { changes.append(GitChange(path: path, status: String(status.suffix(1)), layer: .unstaged)) }
            }
        }
        return changes
    }

    /// Reads the selected local layer now; it is not a saved copy of the status snapshot.
    public static func localPatch(_ file: GitChange, in workspace: GitWorkspaceSnapshot) async throws -> String {
        let commands = WorkspaceReadCommands(directory: workspace.root)
        let identity = try await commands.identity()
        guard identity.head == workspace.head, identity.branch == workspace.branch else {
            throw WorkspaceReadError("The branch changed. Refresh the file list.")
        }
        if file.layer == .untracked { return try UntrackedPreview.read(file.path, in: workspace.root) }
        guard file.layer != .branch else { throw WorkspaceReadError("Select a branch comparison first.") }
        var args = ["diff", "--no-ext-diff", "--no-textconv", "--no-renames", "--color=never"]
        if file.layer == .staged { args.append("--cached") }
        if file.layer == .conflicted { args.append("--cc") }
        let patch = try await commands.text(args + ["--", file.path])
        guard try await commands.identity() == identity else {
            throw WorkspaceReadError("The branch changed during the read. Refresh the file list.")
        }
        return patch.isEmpty ? "No diff for this layer. Refresh the file list." : patch
    }

    public static func compareBranch(
        in workspace: GitWorkspaceSnapshot, baseRef: String
    ) async throws -> GitBranchComparison {
        guard let head = workspace.head else { throw WorkspaceReadError("This repository has no commits yet.") }
        guard workspace.refs.contains(baseRef) else { throw WorkspaceReadError("Select an available base ref.") }
        let commands = WorkspaceReadCommands(directory: workspace.root)
        let base = try await commands.text(["rev-parse", "--verify", "--end-of-options", baseRef + "^{commit}"])
        let mergeBase = try await commands.text(["merge-base", base, head])
        let data = try await commands.git([
            "diff", "--no-ext-diff", "--no-textconv", "--no-renames", "--name-status", "-z", mergeBase, head, "--",
        ]).stdout
        let fields = data.split(separator: 0)
        guard fields.count.isMultiple(of: 2) else { throw WorkspaceReadError("Git returned an unreadable branch diff.") }
        var files: [GitChange] = []
        for i in stride(from: 0, to: fields.count, by: 2) {
            guard let path = String(data: fields[i + 1], encoding: .utf8) else {
                throw WorkspaceReadError("A changed path is not valid UTF-8.")
            }
            files.append(GitChange(path: path, status: String(decoding: fields[i], as: UTF8.self), layer: .branch))
        }
        guard try await commands.identity().head == head,
              try await commands.text(["rev-parse", "--verify", "--end-of-options", baseRef + "^{commit}"]) == base else {
            throw WorkspaceReadError("The comparison changed during the read. Try Refresh.")
        }
        return GitBranchComparison(baseRef: baseRef, baseOID: base, mergeBaseOID: mergeBase, headOID: head, files: files, readAt: Date())
    }

    /// Reads the displayed immutable commit pair without fetching or consulting moved refs.
    public static func branchPatch(
        _ file: GitChange, comparison: GitBranchComparison, in workspace: GitWorkspaceSnapshot
    ) async throws -> String {
        let commands = WorkspaceReadCommands(directory: workspace.root)
        return try await commands.text([
            "diff", "--no-ext-diff", "--no-textconv", "--no-renames", "--color=never",
            comparison.mergeBaseOID, comparison.headOID, "--", file.path,
        ])
    }
}
