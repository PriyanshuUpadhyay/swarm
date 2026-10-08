import Foundation

public enum GitTaskWorktree {
    static let pullRequestFetchTimeout: Duration = .seconds(60)

    public static func branchName(_ name: String, prefix: String) -> String? {
        guard !name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { return nil }
        let branch = prefix + slug(name)
        // Match check-ref-format --branch without starting a process for each field edit.
        let forbidden = CharacterSet(charactersIn: " ~^:?*[\\")
        guard !branch.hasPrefix("-"), !branch.hasSuffix("."),
              !branch.contains(".."), !branch.contains("@{"),
              !branch.unicodeScalars.contains(where: { $0.value < 32 || $0.value == 127 || forbidden.contains($0) }),
              branch.split(separator: "/", omittingEmptySubsequences: false).allSatisfy({
                  !$0.isEmpty && !$0.hasPrefix(".") && !$0.hasSuffix(".lock")
              }) else { return nil }
        return branch
    }

    private static func slug(_ name: String) -> String {
        let title = name.trimmingCharacters(in: .whitespacesAndNewlines)
        let slug = title.lowercased().unicodeScalars.reduce(into: "") { result, scalar in
            if scalar.isASCII, CharacterSet.alphanumerics.contains(scalar) {
                result.unicodeScalars.append(scalar)
            } else if !result.isEmpty, !result.hasSuffix("-") {
                result.append("-")
            }
        }.trimmingCharacters(in: CharacterSet(charactersIn: "-"))
        return String((slug.isEmpty ? "task" : slug).prefix(40))
    }

    public static func references(in commonDirectory: String) async throws -> WorkspaceReferences {
        let output = try await Git.checkRaw(
            ["for-each-ref", "--format=%(refname:lstrip=2)%00%(refname)", "refs/heads", "refs/remotes/origin"],
            in: commonDirectory
        )
        let names = String(decoding: output.stdout, as: UTF8.self).split(separator: "\n").map {
            $0.split(separator: "\0").map(String.init)
        }
        let reference = try await defaultReference(in: commonDirectory)
        let defaultBranch = reference.map { ref in
            if ref.hasPrefix("refs/heads/") { return String(ref.dropFirst("refs/heads/".count)) }
            if ref.hasPrefix("refs/remotes/") { return String(ref.dropFirst("refs/remotes/".count)) }
            return ref
        }
        let local = names.filter { $0[1].hasPrefix("refs/heads/") }.map { $0[0] }
        let remote = names.filter { $0[1].hasPrefix("refs/remotes/") && $0[1] != "refs/remotes/origin/HEAD" }.map { $0[0] }
        func ordered(_ branches: [String]) -> [String] {
            branches.filter { $0 == defaultBranch } + branches.filter { $0 != defaultBranch }
        }
        let held = Set(try await Git.worktrees(of: commonDirectory).compactMap(\.branch))
        return WorkspaceReferences(defaultBranch: defaultBranch, local: ordered(local), remote: ordered(remote), held: held)
    }

    public static func create(
        _ request: WorkspaceRequest, in repositoryDirectory: String,
        commonDirectory: String, under parentDirectory: String
    ) async throws -> String {
        guard !request.name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
            throw GitTaskWorktreeError.emptyName
        }
        guard let branch = branchName(request.name, prefix: request.prefix) else {
            throw GitTaskWorktreeError.invalidBranchName
        }
        let parent = URL(fileURLWithPath: parentDirectory).standardizedFileURL
        let path = parent.appendingPathComponent(slug(request.name), isDirectory: true).path
        guard !FileManager.default.fileExists(atPath: path) else { throw GitTaskWorktreeError.folderExists(path) }
        let references = try await references(in: commonDirectory)
        let add: [String]
        switch request.start {
        case .newBranch(let base):
            add = references.defaultBranch == nil
                ? ["worktree", "add", "--orphan", "-b", branch, "--", path]
                : ["worktree", "add", "-b", branch, "--", path, base]
        case .existingBranch(let existing):
            guard references.defaultBranch != nil else { throw GitTaskWorktreeError.noCommit }
            let local = WorkspaceReferences.localName(of: existing)
            if local != existing {
                add = ["worktree", "add", "--track", "-b", local, "--", path, existing]
            } else {
                add = ["worktree", "add", "--", path, existing]
            }
        case .pullRequest(let number):
            guard references.defaultBranch != nil else { throw GitTaskWorktreeError.noCommit }
            guard number > 0 else { throw GitTaskWorktreeError.invalidPullRequest }
            do {
                _ = try await Git.checkRaw(
                    ["fetch", "origin", "refs/pull/\(number)/head"], in: repositoryDirectory, timeout: pullRequestFetchTimeout
                )
            } catch ShellFailure.timedOut {
                throw GitTaskWorktreeError.pullRequestFetchTimedOut(number)
            }
            add = ["worktree", "add", "-b", branch, "--", path, "FETCH_HEAD"]
        }
        try FileManager.default.createDirectory(at: parent, withIntermediateDirectories: true)
        _ = try await Git.checkRaw(add, in: repositoryDirectory)
        let expectedBranch: String
        if case .existingBranch(let existing) = request.start {
            expectedBranch = WorkspaceReferences.localName(of: existing)
        } else {
            expectedBranch = branch
        }
        guard let created = try await Git.worktrees(of: repositoryDirectory).first(where: { $0.branch == expectedBranch }) else {
            throw GitTaskWorktreeError.notListed(expectedBranch)
        }
        return created.path
    }

    /// Nil when no default branch exists and HEAD names no commit: a new repository, or an
    /// unborn HEAD beside other branches. The workspace then starts an orphan branch.
    private static func defaultReference(in commonDirectory: String) async throws -> String? {
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
        let headRef = String(decoding: head.stdout, as: UTF8.self)
            .trimmingCharacters(in: .whitespacesAndNewlines)
        if head.ok, try await hasReference(headRef, in: commonDirectory) { return headRef }
        let commit = try await Git.runRaw(
            ["rev-parse", "--verify", "--quiet", "HEAD^{commit}"], in: commonDirectory
        )
        // Exit 1 is "HEAD names no commit"; any other failure is a fault, not an empty repository.
        if commit.status == 1 { return nil }
        guard commit.ok else {
            throw Git.error(["rev-parse", "HEAD^{commit}"], commit.status, commit.stderr, "")
        }
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
    case folderExists(String)
    case invalidBranchName
    case invalidPullRequest
    case pullRequestFetchTimedOut(Int)
    case noCommit

    public var errorDescription: String? {
        switch self {
        case .emptyName: "Enter a workspace name"
        case .notRepository: "New Workspace needs a Git project"
        case .folderExists(let path): "A folder already exists at \(path)"
        case .invalidBranchName: "Enter a name and prefix that make a valid Git branch"
        case .invalidPullRequest: "Enter a positive pull request number"
        case .pullRequestFetchTimedOut(let number): "Fetching pull request #\(number) from origin took longer than \(GitTaskWorktree.pullRequestFetchTimeout.components.seconds) s."
        case .noCommit: "The first workspace must start a new branch"
        case .notListed(let branch): "Git created \(branch), but did not list its worktree"
        }
    }
}

public enum WorkspaceStart: Sendable, Equatable {
    case newBranch(base: String)
    case existingBranch(String)
    case pullRequest(Int)
}

public struct WorkspaceRequest: Sendable, Equatable {
    public var name: String
    public var start: WorkspaceStart
    public var prefix: String

    public init(name: String, start: WorkspaceStart, prefix: String) {
        self.name = name
        self.start = start
        self.prefix = prefix
    }
}

public struct WorkspaceReferences: Sendable, Equatable {
    public let defaultBranch: String?
    public let local: [String]
    public let remote: [String]
    public let held: Set<String>

    public init(defaultBranch: String?, local: [String], remote: [String], held: Set<String> = []) {
        self.defaultBranch = defaultBranch
        self.local = local
        self.remote = remote
        self.held = held
    }

    public static func localName(of branch: String) -> String {
        let prefix = "origin/"
        return branch.hasPrefix(prefix) ? String(branch.dropFirst(prefix.count)) : branch
    }

    /// O(n) in the number of local and remote branches.
    public var availableBranches: [String] {
        let localNames = Set(local)
        return local.filter { !held.contains($0) }
            + remote.filter {
                let local = Self.localName(of: $0)
                return !localNames.contains(local) && !held.contains(local)
            }
    }

    public var bases: [String] {
        let branches = local + remote
        guard let defaultBranch else { return branches }
        return [defaultBranch] + branches.filter { $0 != defaultBranch }
    }
}
