import Foundation

public enum WorkspacesRoot {
    public static let preferredName = "swarm/workspaces.noindex"

    public static func resolve(
        home: URL = FileManager.default.homeDirectoryForCurrentUser
    ) -> URL {
        home.appendingPathComponent(preferredName, isDirectory: true)
    }

    public static let note = "The name ends .noindex, which keeps Spotlight out of the dependencies "
        + "and build folders inside every worktree."
}
