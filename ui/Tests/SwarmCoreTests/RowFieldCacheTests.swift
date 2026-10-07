import Foundation
import Testing
@testable import SwarmCore

@Suite("Row field GitHub cache")
struct RowFieldCacheTests {
    @Test("Only requested workspace paths use lookup and each path waits 120 seconds")
    func requestedAndLimited() async throws {
        let recorder = GitHubRowRecorder()
        let cache = RowFieldCache(inspect: { await recorder.inspect($0) }, lookup: { try await recorder.lookup($0) })
        _ = await cache.refresh(paths: [], githubPaths: [], now: Date(timeIntervalSince1970: 100))
        #expect(await recorder.lookups.isEmpty)
        let first = await cache.refresh(paths: [], githubPaths: ["/repo", "/repo"], now: Date(timeIntervalSince1970: 100))
        #expect(first["/repo"]?.pr == "PR #7")
        #expect(first["/repo"]?.ci == "CI passed")
        #expect(first["/repo"]?.dirtyCount == nil)
        _ = await cache.refresh(paths: ["/repo"], githubPaths: ["/repo"], now: Date(timeIntervalSince1970: 110))
        _ = await cache.refresh(paths: [], githubPaths: ["/repo"], now: Date(timeIntervalSince1970: 219))
        #expect(await recorder.lookups == ["/repo"])
        _ = await cache.refresh(paths: [], githubPaths: ["/other"], now: Date(timeIntervalSince1970: 219))
        #expect(await recorder.lookups == ["/repo", "/other"])
        _ = await cache.refresh(paths: [], githubPaths: ["/repo"], now: Date(timeIntervalSince1970: 220))
        #expect(await recorder.lookups == ["/repo", "/other", "/repo"])
    }

    @Test("A lookup in progress reserves the interval before it returns")
    func inFlight() async {
        let recorder = GitHubRowRecorder()
        let gate = RowLookupGate()
        let cache = RowFieldCache(inspect: { await recorder.inspect($0) }, lookup: { try await gate.lookup($0) })
        let first = Task { await cache.refresh(paths: [], githubPaths: ["/repo"], now: Date(timeIntervalSince1970: 100)) }
        await gate.waitForStart()
        _ = await cache.refresh(paths: [], githubPaths: ["/repo"], now: Date(timeIntervalSince1970: 100))
        #expect(await gate.starts == 1)
        await gate.release()
        #expect(await first.value["/repo"]?.pr == "PR #7")
    }

    @Test("No match and failed reads are cached for the same interval and clear old values")
    func absentAndFailure() async {
        let recorder = GitHubRowRecorder()
        let cache = RowFieldCache(inspect: { await recorder.inspect($0) }, lookup: { try await recorder.lookup($0) })
        _ = await cache.refresh(paths: [], githubPaths: ["/repo"], now: Date(timeIntervalSince1970: 100))
        await recorder.setMode("none")
        let absent = await cache.refresh(paths: [], githubPaths: ["/repo"], now: Date(timeIntervalSince1970: 220))
        #expect(absent["/repo"]?.pr == nil && absent["/repo"]?.ci == nil)
        _ = await cache.refresh(paths: [], githubPaths: ["/repo"], now: Date(timeIntervalSince1970: 339))
        #expect(await recorder.lookups.count == 2)
        await recorder.setMode("failure")
        let failed = await cache.refresh(paths: [], githubPaths: ["/repo"], now: Date(timeIntervalSince1970: 340))
        #expect(failed["/repo"]?.pr == nil && failed["/repo"]?.ci == nil)
        _ = await cache.refresh(paths: [], githubPaths: ["/repo"], now: Date(timeIntervalSince1970: 459))
        #expect(await recorder.lookups.count == 3)
    }

    @Test("CI shows failed, pending, passed, unknown, or no reported checks")
    func checkStates() throws {
        for (checks, expected) in [
            (#"[{"conclusion":"SUCCESS"},{"state":"SUCCESS"}]"#, "CI passed"),
            (#"[{"conclusion":"SUCCESS"},{"conclusion":"FAILURE"}]"#, "CI failed"),
            (#"[{"conclusion":"TIMED_OUT"},{"status":"IN_PROGRESS"}]"#, "CI failed"),
            (#"[{"status":"QUEUED"},{"conclusion":"SUCCESS"}]"#, "CI pending"),
            (#"[{"state":"PENDING"}]"#, "CI pending"),
            (#"[{"status":"COMPLETED"}]"#, "CI unknown"),
            ("[]", nil), ("null", nil)
        ] as [(String, String?)] {
            let fields = RowFields.githubFields(try rowLookup(checks: checks))
            #expect(fields.pr == "PR #7")
            #expect(fields.ci == expected)
        }
    }

    @Test("Slow values follow the chosen lists on project, workspace, chat, and tab")
    func slowFields() {
        let chat = SwarmProjectSession(sessions: [SwarmSession(
            id: .init("chat"), talkMode: "lane", adapter: "tmux-solo", cwd: "/repo",
            createdAt: 90, chairLog: nil, agents: 1, messages: 0, lastMessageAt: nil
        )], title: "Fix CI")
        let project = ProjectNode(id: .folder("/repo"), path: "/repo", launchDirectory: "/repo", workspaces: [
            WorkspaceNode(path: "/repo", name: "repo", sessions: [chat])
        ])
        let entries = WorkspaceEntry.list(in: SessionsTree(projects: [project]))
        var navigation = WorkspaceNavigation()
        navigation.fields.project = [.ci, .pr]
        navigation.fields.workspace = [.pr, .ci]
        navigation.fields.chat = [.ci, .pr]
        navigation.fields.tab = [.pr, .ci]
        let cache = ["/repo": RowWorkspaceFields(pr: "PR #7", ci: "CI pending")]
        let sections = SidebarRows.sections(projects: [project], workspaces: entries, navigation: navigation,
                                            search: "", showingArchive: false, now: 100, workspaceFields: cache)
        #expect(sections[0].fields.map(\.text) == ["CI pending", "PR #7"])
        #expect(sections[0].rows[0].fields.map(\.text) == ["PR #7", "CI pending"])
        #expect(sections[0].rows[1].fields.map(\.text) == ["CI pending", "PR #7"])
        let tabs = ChatTab.tabs([ChatRow(session: chat, workspace: "repo", workspacePath: "/repo")], closing: [], now: 100,
                                navigation: navigation, workspaceFields: cache)
        #expect(tabs[0].fields.map(\.text) == ["PR #7", "CI pending"])
        navigation.fields = RowFieldLists()
        #expect(RowFields.requestedPaths(for: .pr, entries: entries, fields: navigation.fields).isEmpty)
        #expect(RowFields.requestedPaths(for: .ci, entries: entries, fields: navigation.fields).isEmpty)
        navigation.fields.tab = [.ci]
        #expect(RowFields.requestedPaths(for: .ci, entries: entries, fields: navigation.fields) == ["/repo"])
    }
}

private actor GitHubRowRecorder {
    var lookups: [String] = []
    private var mode = "found"
    func setMode(_ mode: String) { self.mode = mode }
    func inspect(_ path: String) -> GitWorkspaceSnapshot {
        GitWorkspaceSnapshot(root: path, branch: "feature", head: "abc", files: [], refs: [], readAt: Date(timeIntervalSince1970: 100))
    }
    func lookup(_ workspace: GitWorkspaceSnapshot) throws -> PullRequestLookup {
        lookups.append(workspace.root)
        if mode == "failure" { throw WorkspaceReadError("fixture network error") }
        if mode == "none" { return PullRequestLookup(repositories: ["owner/repo"], match: nil) }
        return try rowLookup(checks: #"[{"conclusion":"SUCCESS"}]"#)
    }
}

private func rowLookup(checks: String) throws -> PullRequestLookup {
    let json = """
    {"number":7,"title":"Fix rows","state":"OPEN","isDraft":false,"baseRefName":"main","baseRefOid":"abc",
     "headRefName":"feature","headRefOid":"def","url":"https://github.com/owner/repo/pull/7","statusCheckRollup":\(checks)}
    """
    let request = try JSONDecoder().decode(GitHubPullRequest.self, from: Data(json.utf8))
    return PullRequestLookup(repositories: ["owner/repo"], match: PullRequestSnapshot(
        repository: "owner/repo", pullRequest: request, localHead: "def", readAt: Date(timeIntervalSince1970: 100)
    ))
}

private actor RowLookupGate {
    var starts = 0
    private var waiters: [CheckedContinuation<Void, Never>] = []
    private var pending: CheckedContinuation<Void, Never>?

    func waitForStart() async {
        if starts > 0 { return }
        await withCheckedContinuation { waiters.append($0) }
    }

    func lookup(_ workspace: GitWorkspaceSnapshot) async throws -> PullRequestLookup {
        starts += 1
        for waiter in waiters { waiter.resume() }
        waiters.removeAll()
        await withCheckedContinuation { pending = $0 }
        return try rowLookup(checks: #"[{"conclusion":"SUCCESS"}]"#)
    }

    func release() {
        pending?.resume()
        pending = nil
    }
}
