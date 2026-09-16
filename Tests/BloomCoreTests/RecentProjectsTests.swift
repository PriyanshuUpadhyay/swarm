import Foundation
import Testing
@testable import BloomCore

/// Which projects the create window calls recent, read off the workspaces started in them.
@Suite("Recent projects")
struct RecentProjectsTests {
    private let flare = Repo(name: "flare", path: "/flare", sortOrder: 0)
    private let bloom = Repo(name: "bloom", path: "/bloom", sortOrder: 1, hidden: true)
    private let site = Repo(name: "site", path: "/site", sortOrder: 2)

    private func workspace(
        in repo: Repo,
        at seconds: TimeInterval,
        state: WorkspaceState = .active,
        origin: WorkspaceOrigin = .user
    ) -> Workspace {
        Workspace(
            repoID: repo.id, name: "w", branch: "w", path: "/w", baseBranch: "main",
            state: state, createdAt: Date(timeIntervalSince1970: seconds), origin: origin
        )
    }

    @Test("newest workspace per project first, and a project never started in is left out")
    func newestFirst() {
        let workspaces = [
            workspace(in: flare, at: 100),
            workspace(in: bloom, at: 200),
            workspace(in: flare, at: 300)
        ]

        #expect(RecentProjects.ordered([flare, bloom, site], workspaces: workspaces).map(\.name)
            == ["flare", "bloom"])
    }

    /// Archiving a workspace is the end of the work, not a sign the project was not used.
    @Test("archived workspaces count")
    func archivedCount() {
        let workspaces = [
            workspace(in: flare, at: 100),
            workspace(in: site, at: 200, state: .archived)
        ]

        #expect(RecentProjects.ordered([flare, site], workspaces: workspaces).map(\.name) == ["site", "flare"])
    }

    @Test("a workspace an agent asked for does not make its project recent")
    func agentSpawned() {
        let parent = workspace(in: flare, at: 100)
        let spawned = workspace(
            in: site, at: 200,
            origin: .agent(parentWorkspaceID: parent.id, spawnToolUseID: "call")
        )

        #expect(RecentProjects.ordered([flare, site], workspaces: [parent, spawned]).map(\.name) == ["flare"])
    }

    @Test("capped at the limit, and a removed project's history is ignored")
    func limitAndRemoved() {
        let repos = (0..<12).map { Repo(name: "p\($0)", path: "/p\($0)", sortOrder: $0) }
        let workspaces = repos.map { workspace(in: $0, at: TimeInterval($0.sortOrder)) }
            + [workspace(in: bloom, at: 1_000)]

        let recent = RecentProjects.ordered(repos, workspaces: workspaces)

        #expect(recent.count == RecentProjects.limit)
        #expect(recent.first?.name == "p11")
        #expect(recent.last?.name == "p2")
    }
}
