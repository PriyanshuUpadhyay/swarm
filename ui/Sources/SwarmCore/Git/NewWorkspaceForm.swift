import Foundation

public enum NewWorkspaceForm {
    public static func preview(_ request: WorkspaceRequest) -> String? {
        if case .existingBranch(let branch) = request.start {
            guard !branch.isEmpty else { return nil }
            return branch.hasPrefix("origin/")
                ? "\(branch.dropFirst("origin/".count)) (tracks \(branch))" : branch
        }
        return GitTaskWorktree.branchName(request.name, prefix: request.prefix)
    }

    public static func canCreate(
        _ request: WorkspaceRequest, references: WorkspaceReferences, worktreeFolder: String
    ) -> Bool {
        guard !worktreeFolder.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty,
              GitTaskWorktree.branchName(request.name, prefix: request.prefix) != nil else { return false }
        switch request.start {
        case .newBranch(let base):
            return references.defaultBranch == nil ? base.isEmpty : references.bases.contains(base)
        case .existingBranch(let branch):
            return references.defaultBranch != nil && references.availableBranches.contains(branch)
        case .pullRequest(let number):
            return references.defaultBranch != nil && number > 0
        }
    }

    public static func defaults(
        worktreeFolder: String, branchPrefix: String, seedFolder: String, seedPrefix: String
    ) -> ProjectDefaults {
        ProjectDefaults(
            worktreeFolder: worktreeFolder == seedFolder ? nil : worktreeFolder,
            branchPrefix: branchPrefix == seedPrefix ? nil : branchPrefix
        )
    }
}
