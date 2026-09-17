import Testing
@testable import SwarmCore

@Suite("Project menu groups")
struct ProjectMenuGroupTests {
    private let zebra = Repo(name: "zebra", path: "/zebra", sortOrder: 0, hidden: true)
    private let vicGames = Repo(name: "VicGames", path: "/VicGames", sortOrder: 1)
    private let alpha = Repo(name: "alpha", path: "/alpha", sortOrder: 2, hidden: true)
    private let swarm = Repo(name: "swarm", path: "/swarm", sortOrder: 3)

    private var repos: [Repo] { [zebra, vicGames, alpha, swarm] }

    @Test("recent comes first in the order given, then every project alphabetically, hidden or not")
    func recentThenAll() {
        let groups = ProjectMenuGroup.grouped(repos, recent: [zebra.id, swarm.id])

        #expect(groups.map(\.title) == ["Recent", "All projects"])
        #expect(groups.map { $0.repos.map(\.name) } == [
            ["zebra", "swarm"],
            ["alpha", "swarm", "VicGames", "zebra"]
        ])
    }

    @Test("no history leaves only the full list, and no projects leaves nothing")
    func emptyGroups() {
        #expect(ProjectMenuGroup.grouped(repos, recent: []).map(\.kind) == [.all])
        #expect(ProjectMenuGroup.grouped([], recent: [swarm.id]).isEmpty)
    }

    @Test("a recent id for a project that has gone is dropped")
    func removedProject() {
        let groups = ProjectMenuGroup.grouped([swarm], recent: [alpha.id, swarm.id])

        #expect(groups.first?.repos.map(\.id) == [swarm.id])
    }

    /// The same project is listed twice, and only one of its rows can be the ticked one.
    @Test("the tick goes on the recent row when there is one, otherwise on the full list")
    func choice() {
        let groups = ProjectMenuGroup.grouped(repos, recent: [swarm.id])

        #expect(ProjectMenuGroup.choice(for: swarm.id, in: groups) == .init(kind: .recent, repoID: swarm.id))
        #expect(ProjectMenuGroup.choice(for: alpha.id, in: groups) == .init(kind: .all, repoID: alpha.id))
        #expect(ProjectMenuGroup.choice(for: nil, in: groups) == .init(kind: .all, repoID: RepoID("")))
    }
}
