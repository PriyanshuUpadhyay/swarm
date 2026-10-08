import Foundation

public struct ProjectDefaults: Codable, Sendable, Hashable {
    public var worktreeFolder: String?
    public var branchPrefix: String?

    public init(worktreeFolder: String? = nil, branchPrefix: String? = nil) {
        self.worktreeFolder = worktreeFolder
        self.branchPrefix = branchPrefix
    }

    public func resolved(for project: ProjectNode) -> (folder: String, prefix: String) {
        let root = URL(fileURLWithPath: project.path)
        let folder = folderSetting(for: project)
        let parent = folder.hasPrefix("/") ? URL(fileURLWithPath: folder) : root.appendingPathComponent(folder)
        return (parent.standardizedFileURL.path, branchPrefix ?? "swarm/")
    }

    public func folderSetting(for project: ProjectNode) -> String {
        let isBare: Bool
        if case .repository(let common) = project.id {
            isBare = URL(fileURLWithPath: common).lastPathComponent == ".bare" || common == project.path
        } else {
            isBare = false
        }
        return worktreeFolder ?? (isBare ? "wt/" : "../\(project.name)-worktrees")
    }
}
