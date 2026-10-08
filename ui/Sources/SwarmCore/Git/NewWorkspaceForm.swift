public enum NewWorkspaceForm {
    public static func preview(_ request: WorkspaceRequest) -> String? {
        if case .existingBranch(let branch) = request.start {
            return branch.isEmpty ? nil : branch
        }
        return GitTaskWorktree.branchName(request.name, prefix: request.prefix)
    }

    public static func canCreate(_ request: WorkspaceRequest, references: WorkspaceReferences) -> Bool {
        guard GitTaskWorktree.branchName(request.name, prefix: request.prefix) != nil else { return false }
        switch request.start {
        case .newBranch(let base):
            return references.defaultBranch == nil ? base.isEmpty : references.bases.contains(base)
        case .existingBranch(let branch):
            return references.defaultBranch != nil && references.availableBranches.contains(branch)
        case .pullRequest(let number):
            return references.defaultBranch != nil && number > 0
        }
    }
}
