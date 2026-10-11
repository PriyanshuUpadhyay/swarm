import Foundation

extension Git {
    public struct IgnoredRemovalItems: Sendable, Hashable {
        public let paths: [String]
        public var count: Int { paths.count }
        public var firstPath: String? { paths.first }
        public var message: String? {
            guard let firstPath else { return nil }
            let items = count == 1 ? "1 ignored item" : "\(count) ignored items"
            return "\(items), such as \(firstPath), \(count == 1 ? "is" : "are") deleted too."
        }
    }

    public static func ignoredRemovalItems(worktree: String) async throws -> IgnoredRemovalItems {
        let status = try await checkRaw(["status", "--porcelain", "--ignored=matching"], in: worktree)
        let paths = String(decoding: status.stdout, as: UTF8.self).split(separator: "\n")
            .filter { $0.hasPrefix("!! ") }.map { String($0.dropFirst(3)) }
        return IgnoredRemovalItems(paths: paths)
    }

    public static func removalBlocker(worktree: String) async -> String? {
        do {
            let status = try await checkRaw(["status", "--porcelain"], in: worktree)
            if !status.stdout.isEmpty { return "This workspace has uncommitted changes. Commit or remove them before deleting it." }
            let upstream = try await runRaw(["rev-parse", "--verify", "@{upstream}"], in: worktree)
            if upstream.ok {
                let count = try await checkRaw(["rev-list", "--count", "@{upstream}..HEAD"], in: worktree)
                guard let unpushed = Int(String(decoding: count.stdout, as: UTF8.self).trimmingCharacters(in: .whitespacesAndNewlines)) else {
                    return "Git could not check this workspace for unpushed commits."
                }
                if unpushed > 0 { return "This workspace has unpushed commits. Push them before deleting it." }
            } else {
                let commits = try await checkRaw(["log", "--oneline", "HEAD", "--not", "--remotes"], in: worktree)
                if !commits.stdout.isEmpty { return "This workspace has unpushed commits and no upstream. Push them before deleting it." }
            }
            return nil
        } catch {
            return "Git could not check this workspace. \(error.localizedDescription)"
        }
    }

    public static func removeWorktree(_ path: String, in repository: String) async throws {
        _ = try await checkRaw(["worktree", "remove", path], in: repository)
    }
}
