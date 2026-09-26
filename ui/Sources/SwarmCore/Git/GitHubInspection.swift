import Foundation

struct GitHubRepository: Sendable, Hashable {
    let name: String

    init?(name: String) {
        let parts = name.split(separator: "/", omittingEmptySubsequences: false)
        guard parts.count == 2, parts.allSatisfy({ part in
            !part.isEmpty && part != "." && part != ".." && part.utf8.allSatisfy {
                (65...90).contains($0) || (97...122).contains($0) || (48...57).contains($0) || [45, 46, 95].contains($0)
            }
        }) else { return nil }
        self.name = name.lowercased()
    }

    init?(remote: String) {
        let path: String
        if remote.hasPrefix("git@github.com:") {
            path = String(remote.dropFirst("git@github.com:".count))
        } else {
            guard let url = URLComponents(string: remote),
                  ["https", "ssh"].contains(url.scheme?.lowercased() ?? ""),
                  url.host?.lowercased() == "github.com", url.query == nil, url.fragment == nil else { return nil }
            path = String(url.path.drop(while: { $0 == "/" }))
        }
        self.init(name: path.hasSuffix(".git") ? String(path.dropLast(4)) : path)
    }
}

public struct GitHubCheck: Decodable, Sendable, Equatable {
    public let name: String?
    public let context: String?
    public let status: String?
    public let state: String?
    public let conclusion: String?
    public var label: String { name ?? context ?? "Check" }
    public var result: String {
        [conclusion, state, status].compactMap { $0 }.first { !$0.isEmpty } ?? "Unknown"
    }
}

public struct GitHubPullRequest: Decodable, Sendable, Equatable {
    public struct Repository: Decodable, Sendable, Equatable { public let nameWithOwner: String }
    public let number: Int
    public let title: String
    public let state: String
    public let isDraft: Bool
    public let baseRefName: String
    public let baseRefOid: String
    public let headRefName: String
    public let headRefOid: String
    public let headRepository: Repository?
    public let reviewDecision: String?
    public let statusCheckRollup: [GitHubCheck]?
    public let url: String

    func hasSameComparison(as other: Self) -> Bool {
        number == other.number && url == other.url
            && baseRefName == other.baseRefName && baseRefOid == other.baseRefOid
            && headRefName == other.headRefName && headRefOid == other.headRefOid
            && headRepository == other.headRepository
    }
}

public struct PullRequestSnapshot: Sendable {
    public let repository: String
    public let pullRequest: GitHubPullRequest
    public let localHead: String
    public let readAt: Date
    public var differsFromLocalHead: Bool { localHead != pullRequest.headRefOid }
}

public struct PullRequestLookup: Sendable {
    public let repositories: [String]
    public let match: PullRequestSnapshot?
}

public struct GitHubInspection: Sendable {
    private let executable: String
    private static let fields = "number,title,state,isDraft,baseRefName,baseRefOid,headRefName,headRefOid,headRepository,reviewDecision,statusCheckRollup,url"

    public init(executable: String = "gh") { self.executable = executable }

    public func lookup(in workspace: GitWorkspaceSnapshot) async throws -> PullRequestLookup {
        guard let branch = workspace.branch, let head = workspace.head else {
            throw WorkspaceReadError("PR lookup needs a branch with commits. This workspace is detached or has no commits yet.")
        }
        let commands = WorkspaceReadCommands(directory: workspace.root, seconds: 30)
        let remotes = try await commands.text(["remote"]).split(separator: "\n").map(String.init)
        var pushRemote: String?
        for key in ["branch.\(branch).pushRemote", "remote.pushDefault", "branch.\(branch).remote"] {
            let result = try await commands.git(["config", "--get", key], accepting: [0, 1])
            let value = String(decoding: result.stdout, as: UTF8.self).trimmingCharacters(in: .newlines)
            if result.status == 0, value != ".", !value.isEmpty { pushRemote = value; break }
        }
        if pushRemote == nil {
            pushRemote = remotes.contains("origin") ? "origin" : remotes.count == 1 ? remotes[0] : nil
        }
        guard let pushRemote, remotes.contains(pushRemote) else {
            throw WorkspaceReadError("The push remote is missing or ambiguous. Set the branch's push remote in Git.")
        }
        let pushURLs = try await commands.text(["remote", "get-url", "--push", "--all", "--", pushRemote]).split(separator: "\n")
        let pushRepositories = pushURLs.compactMap { GitHubRepository(remote: String($0)) }
        guard pushRepositories.count == pushURLs.count, Set(pushRepositories).count == 1,
              let headRepository = pushRepositories.first else {
            throw WorkspaceReadError("PR lookup needs one github.com push repository. SSH aliases and other hosts are not supported yet.")
        }
        var targets: Set<GitHubRepository> = [headRepository]
        for remote in remotes {
            let url = try await commands.text(["remote", "get-url", "--", remote])
            if let repository = GitHubRepository(remote: url) { targets.insert(repository) }
        }
        struct RepositoryInfo: Decodable {
            struct Parent: Decodable {
                struct Owner: Decodable { let login: String }
                let name: String
                let owner: Owner
            }
            let nameWithOwner: String
            let parent: Parent?
        }
        let info: RepositoryInfo = try decode(try await gh([
            "repo", "view", "github.com/" + headRepository.name, "--json", "nameWithOwner,parent",
        ], commands: commands))
        guard GitHubRepository(name: info.nameWithOwner) == headRepository else {
            throw WorkspaceReadError("The GitHub repository was renamed or moved. Update the Git remote and refresh.")
        }
        if let parentInfo = info.parent {
            let parentName = parentInfo.owner.login + "/" + parentInfo.name
            guard let parent = GitHubRepository(name: parentName) else { throw WorkspaceReadError("GitHub returned an invalid parent repository.") }
            targets.insert(parent)
        }
        var matches: [PullRequestSnapshot] = []
        for target in targets.sorted(by: { $0.name < $1.name }) {
            let requests: [GitHubPullRequest] = try decode(try await gh([
                "pr", "list", "--repo", "github.com/" + target.name, "--head", branch,
                "--state", "open", "--limit", "100", "--json", Self.fields,
            ], commands: commands))
            guard requests.count < 100 else { throw WorkspaceReadError("PR lookup reached its result limit. Narrow the branch or repository selection in Git.") }
            for request in requests where request.headRefName == branch {
                guard let name = request.headRepository?.nameWithOwner else {
                    throw WorkspaceReadError("A matching PR has no verifiable head repository.")
                }
                guard GitHubRepository(name: name) == headRepository else { continue }
                try validate(request, repository: target.name)
                matches.append(PullRequestSnapshot(repository: target.name, pullRequest: request, localHead: head, readAt: Date()))
            }
        }
        guard matches.count <= 1 else { throw WorkspaceReadError("More than one open PR matches this branch and repository. Open GitHub to select the intended PR.") }
        let latest = try await commands.identity()
        guard latest.branch == branch, latest.head == head else { throw WorkspaceReadError("The branch changed during PR lookup. Try Refresh.") }
        return PullRequestLookup(repositories: targets.map(\.name).sorted(), match: matches.first)
    }

    public func patch(for snapshot: PullRequestSnapshot, in directory: String) async throws -> String {
        let commands = WorkspaceReadCommands(directory: directory, seconds: 30)
        let request = snapshot.pullRequest
        let args = ["pr", "view", String(request.number), "--repo", "github.com/" + snapshot.repository, "--json", Self.fields]
        let before: GitHubPullRequest = try decode(try await gh(args, commands: commands))
        try validate(before, repository: snapshot.repository)
        guard request.hasSameComparison(as: before) else { throw WorkspaceReadError("The PR comparison changed. Refresh before opening its diff.") }
        let patch = try await gh([
            "pr", "diff", String(request.number), "--repo", "github.com/" + snapshot.repository, "--color", "never",
        ], commands: commands)
        let after: GitHubPullRequest = try decode(try await gh(args, commands: commands))
        guard before.hasSameComparison(as: after) else { throw WorkspaceReadError("The PR base or head changed during the read. Refresh and try again.") }
        guard let text = String(data: patch, encoding: .utf8) else { throw WorkspaceReadError("The PR diff is not valid UTF-8.") }
        return text.isEmpty ? "GitHub returned no diff for this comparison." : text
    }

    private func gh(_ arguments: [String], commands: WorkspaceReadCommands) async throws -> Data {
        let result = try await Shell.runBytes(executable, arguments, cwd: commands.directory, env: [
            "GH_PROMPT_DISABLED": "1", "GH_NO_UPDATE_NOTIFIER": "1", "GH_HOST": "github.com", "GH_REPO": "",
        ], timeout: commands.remaining(maximum: .seconds(10)), outputLimit: 1024 * 1024)
        guard result.status == 0 else {
            let error = String(decoding: result.stderr, as: UTF8.self).trimmingCharacters(in: .whitespacesAndNewlines)
            throw WorkspaceReadError("GitHub read failed. Check gh login and network access. " + error)
        }
        return result.stdout
    }

    private func decode<T: Decodable>(_ data: Data) throws -> T {
        do { return try JSONDecoder().decode(T.self, from: data) }
        catch { throw WorkspaceReadError("GitHub returned an unreadable response. Try Refresh.") }
    }

    private func validate(_ request: GitHubPullRequest, repository: String) throws {
        guard request.number > 0, !request.baseRefOid.isEmpty, !request.headRefOid.isEmpty,
              request.url.lowercased() == "https://github.com/\(repository)/pull/\(request.number)" else {
            throw WorkspaceReadError("The PR repository or revision could not be verified.")
        }
    }
}
