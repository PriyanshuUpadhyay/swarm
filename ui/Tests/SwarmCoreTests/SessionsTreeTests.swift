import Foundation
import Testing
@testable import SwarmCore

@Suite("Sessions tree")
struct SessionsTreeTests {
    private let common = "/repo/.bare"
    private let worktrees = [
        WorktreeEntry(path: "/repo/wt/main", branch: "main"),
        WorktreeEntry(path: "/repo/wt/feature", branch: "feature"),
    ]

    @Test("Worktree listings stay cached for ten seconds and refresh additions and removals")
    func worktreeListingsExpire() async throws {
        let calls = WorktreeListingCalls()
        let discovery = SwarmSessionDiscovery(worktreeLister: { await calls.list($0) })
        let start = Date.now
        let first = try await discovery.worktrees(for: "/repo-one", now: start)
        #expect(first.map(\.path) == ["/repo-one/main"])
        await calls.set([WorktreeEntry(path: "/repo-one/new", branch: "new")])
        #expect(try await discovery.worktrees(for: "/repo-one", now: start.addingTimeInterval(9.999)) == first)
        #expect(await calls.paths == ["/repo-one"])
        #expect(try await discovery.worktrees(for: "/repo-one", now: start.addingTimeInterval(10)).map(\.path) == ["/repo-one/new"])
        _ = try await discovery.worktrees(for: "/repo-two", now: start.addingTimeInterval(10))
        #expect(await calls.paths == ["/repo-one", "/repo-one", "/repo-two"])
        await calls.set([])
        #expect(try await discovery.worktrees(for: "/repo-one", now: start.addingTimeInterval(20)).isEmpty)
    }

    @Test("Forgetting worktrees prevents an older in-flight listing from filling the cache")
    func forgetWorktreesDuringListing() async throws {
        let calls = SuspendedWorktreeListing()
        let discovery = SwarmSessionDiscovery(worktreeLister: { await calls.list($0) })
        let start = Date.now
        let oldListing = Task { try await discovery.worktrees(for: "/repo-one", now: start) }
        await calls.waitUntilStarted()
        await discovery.forgetWorktrees(for: "/repo-one")
        let fresh = try await discovery.worktrees(for: "/repo-one", now: start)
        #expect(fresh.map(\.path) == ["/repo-one/new"])
        await calls.resume()
        #expect(try await oldListing.value.map(\.path) == ["/repo-one/old"])
        #expect(try await discovery.worktrees(for: "/repo-one", now: start) == fresh)
        #expect(await calls.count == 2)
    }

    @Test("The tree reads many sessions with one agents call and keeps unknown sessions unknown")
    func treeReadsAgentsOnce() async throws {
        let agent = SwarmAgent(id: .init("orchestrator"), role: "chair", pane: "%1", alive: true, state: "working")
        var sessions = (0..<50).map { session("batch-\($0)", cwd: "/outside") }
        sessions[0].adapter = "herdr"
        var unknown = session("no-adapter", cwd: "/outside")
        unknown.adapter = nil
        var empty = session("empty", cwd: "/outside")
        empty.agents = 0
        let archived = session("archived", cwd: "/outside", archivedAt: 50)
        sessions += [unknown, empty, archived]
        let listings = Dictionary(uniqueKeysWithValues: sessions.map {
            ($0.id.rawValue, SwarmAgentList(agents: [agent]))
        })
        let encoder = JSONEncoder()
        encoder.keyEncodingStrategy = .convertToSnakeCase
        let batch = String(decoding: try encoder.encode(listings), as: UTF8.self)
        let single = String(decoding: try encoder.encode(SwarmAgentList(agents: [agent])), as: UTF8.self)
        let calls = TreeAgentCalls()
        let bus = SwarmCLIBus(environment: [:], cwd: "/tmp") { _, arguments, _, environment, _, _ in
            await calls.record(arguments, environment: environment)
            return ShellResult(status: 0, stdout: arguments.contains("--all") ? batch : single, stderr: "")
        }
        let tree = try await SwarmSessionDiscovery().tree(sessions: sessions, bus: bus)
        #expect(await calls.arguments == [["agents", "--json", "--all"]])
        #expect(await calls.sessionIDs == [nil])
        for item in sessions.prefix(50) {
            #expect(tree.session(item.id)?.status == .working)
            #expect(tree.session(item.id)?.isRunning == true)
            #expect(tree.session(item.id)?.liveAgents == 1)
        }
        #expect(tree.session(unknown.id)?.status == nil)
        #expect(tree.session(unknown.id)?.isRunning == nil)
        #expect(tree.session(empty.id) == nil)
        #expect(tree.session(archived.id) == nil)
    }

    @Test("An older swarm with no --all still lists each session's agents")
    func treeFallsBackWithoutBatch() async throws {
        let agent = SwarmAgent(id: .init("orchestrator"), role: "chair", pane: "%1", alive: true, state: "working")
        let sessions = (0..<3).map { session("old-\($0)", cwd: "/outside") }
        let encoder = JSONEncoder()
        encoder.keyEncodingStrategy = .convertToSnakeCase
        let single = String(decoding: try encoder.encode(SwarmAgentList(agents: [agent])), as: UTF8.self)
        let calls = TreeAgentCalls()
        let bus = SwarmCLIBus(environment: [:], cwd: "/tmp") { _, arguments, _, environment, _, _ in
            await calls.record(arguments, environment: environment)
            return arguments.contains("--all")
                ? ShellResult(status: 2, stdout: "", stderr: "unknown flag --all")
                : ShellResult(status: 0, stdout: single, stderr: "")
        }
        let tree = try await SwarmSessionDiscovery().tree(sessions: sessions, bus: bus)
        #expect(await calls.arguments.count == 4)
        for item in sessions { #expect(tree.session(item.id)?.status == .working) }
    }

    @Test("One repository keeps workspaces and exposes their chats")
    func repositoryAndWorktrees() {
        var older = session("11111111-a", cwd: "/repo/wt/main/src")
        older.lastMessageAt = 10
        var newer = session("22222222-b", cwd: "/repo/wt/feature")
        newer.lastMessageAt = 20
        let tree = build([older, newer, session("33333333-c", cwd: "/repo/.bare")])
        #expect(tree.projects.count == 1)
        #expect(tree.projects[0].name == "repo")
        #expect(tree.projects[0].workspaces.map(\.name) == ["feature", "main", "repo"])
        #expect(tree.projects[0].workspaces.map(\.sessions.count) == [1, 1, 1])
        #expect(tree.projects[0].chats.map(\.id) == [newer.id, older.id, SwarmSessionID("33333333-c")])
        #expect(tree.launchDirectory(for: newer.id) == "/repo/wt/feature")
    }

    @Test("Chat tabs contain only chats from the selected workspace")
    func workspaceChats() {
        let first = session("first", cwd: "/repo/wt/feature")
        let second = session("second", cwd: "/repo/wt/feature/src")
        let other = session("other", cwd: "/repo/wt/main")
        let tree = build([first, second, other])
        #expect(Set(tree.workspaceChats(for: first.id).map(\.id)) == [first.id, second.id])
        #expect(tree.workspaceChats(for: other.id).map(\.id) == [other.id])
        #expect(tree.workspaceChats(for: SwarmSessionID("missing")).isEmpty)
    }

    @Test("A folder is one workspace")
    func folder() {
        let tree = build([session("folder-1", cwd: "/outside")])
        #expect(tree.projects[0].path == "/outside")
        #expect(tree.projects[0].workspaces.map(\.name) == ["outside"])
        #expect(tree.projects[0].workspaces[0].sessions.count == 1)
        #expect(tree.launchDirectory(for: SwarmSessionID("folder-1")) == "/outside")
    }

    @Test("Opened projects stay visible with no chats")
    func openedEmptyProjects() {
        let folder = build([], projectPaths: ["/outside"])
        #expect(folder.projects.map(\.path) == ["/outside"])
        #expect(folder.projects[0].chats.isEmpty)

        let repository = build([], projectPaths: ["/repo/wt/feature"])
        #expect(repository.projects.map(\.path) == ["/repo"])
        #expect(repository.projects[0].launchDirectory == "/repo/wt/feature")
        #expect(repository.projects[0].chats.isEmpty)
        #expect(repository.projects[0].workspaces.map(\.name) == ["feature", "main"])

        let reopened = build([], projectPaths: ["/repo/wt/feature", "/repo/wt/main"])
        #expect(reopened.projects.count == 1)
        #expect(reopened.projects[0].launchDirectory == "/repo/wt/main")
    }

    @Test("Resolved titles name their rows and missing titles fall back")
    func resolvedTitles() {
        let named = session("named", cwd: "/outside")
        let fallback = session("fallback", cwd: "/outside")
        let tree = build([named, fallback], titles: [named.id: "Repair the sidebar"])
        let titles = Dictionary(uniqueKeysWithValues: tree.projects[0].workspaces[0].sessions.map {
            ($0.id, $0.title)
        })
        #expect(titles == [named.id: "Repair the sidebar", fallback.id: "Chat fallback"])
        #expect(tree.windowTitle(for: named.id) == "outside · Repair the sidebar")
    }

    @Test("Session rows have clear titles, captions, and states")
    func rowPresentation() {
        let live = projectSession("live-title", title: "Build sidebar", provider: "claude", running: true)
        #expect(SessionRowPresentation.make(chat(live), now: 7_201) == SessionRowPresentation(
            title: "Build sidebar", caption: "outside · claude", age: "2h",
            state: .live, provider: "claude"
        ))

        let untitled = projectSession("abcdefgh-more", title: "", provider: "codex", running: true)
        #expect(SessionRowPresentation.make(chat(untitled), now: 7_201) == SessionRowPresentation(
            title: "codex abcdefgh", caption: "outside · codex", age: "2h",
            state: .live, provider: "codex"
        ))

        let ended = projectSession("ended-session", title: "", provider: "codex", running: false)
        #expect(SessionRowPresentation.make(chat(ended), now: 7_201) == SessionRowPresentation(
            title: "codex ended-se", caption: "outside · ended", age: "2h",
            state: .ended, provider: "codex"
        ))

        let noChair = projectSession("missing-chair", title: "", provider: nil, running: true)
        #expect(SessionRowPresentation.make(chat(noChair), now: 7_201) == SessionRowPresentation(
            title: "Chat missing-", caption: "outside · no chair", age: "2h",
            state: .noChair, provider: nil
        ))
    }

    @Test("Agent state feeds the sidebar presentation and tree text")
    func agentState() {
        let dead = session("dead-session", cwd: "/outside")
        let agent = SwarmAgent(
            id: .init("orchestrator"), role: "chair", pane: "%1", alive: false
        )
        let tree = build([dead], agentsBySession: [dead.id: [agent]])
        #expect(SessionRowPresentation.make(
            tree.projects[0].chats[0], now: 61
        ).caption == "outside · ended")
        #expect(tree.text(now: 61).contains("Chat dead-ses · outside · ended · 1m"))

        let empty = build([dead], agentsBySession: [dead.id: []])
        #expect(empty.projects.isEmpty)

        let running = build([dead], agentsBySession: [dead.id: [agent, SwarmAgent(
            id: .init("worker"), role: "code", pane: "%2", alive: true
        )]])
        #expect(SessionRowPresentation.make(
            running.projects[0].chats[0], now: 61
        ).caption == "outside · no chair")
    }

    @Test("A chat and its workspace show their agents' most urgent status")
    func aggregateStatus() {
        let chat = session("status-session", cwd: "/outside")
        let chair = SwarmAgent(
            id: .init("orchestrator"), role: "chair", pane: "%1", alive: true, state: "working"
        )
        let reviewer = SwarmAgent(
            id: .init("reviewer"), role: "review", pane: "%2", alive: true, state: "waiting"
        )
        let tree = build([chat], agentsBySession: [chat.id: [chair, reviewer]])
        #expect(tree.projects[0].chats[0].session.status == .waiting)
        #expect(WorkspaceEntry.list(in: tree).first?.status == .waiting)
        #expect(build([chat]).projects.first?.chats.first?.session.status == nil)
    }

    @Test("Sidebar sections keep workspace order and count agents by status")
    func sidebarSections() {
        let api = session("api-session", cwd: "/api")
        let docs = session("docs-session", cwd: "/docs")
        func agent(_ id: String, _ state: String?) -> SwarmAgent {
            SwarmAgent(id: .init(id), role: "code", pane: "%\(id)", alive: true, state: state)
        }
        let tree = build([api, docs], agentsBySession: [
            api.id: [agent("reviewer", "waiting"), agent("coder", "working"), agent("tester", "working")],
            docs.id: [agent("writer", nil)],
        ])
        let workspaces = WorkspaceEntry.list(in: tree)
        var navigation = WorkspaceNavigation()
        navigation.pinned = ["/docs"]
        let sections = SidebarRows.sections(
            projects: tree.projects, workspaces: workspaces, navigation: navigation, search: "",
            showingArchive: false, now: 61
        )
        #expect(sections.map(\.title) == ["Pinned", "api", "docs"])
        #expect(sections.map(\.kind) == [.pinned, .project(path: "/api"), .project(path: "/docs")])
        #expect(sections[0].rows.filter { $0.kind == .workspace }.map(\.id) == ["/docs"])
        // A project whose only workspace is pinned keeps its header for its "+".
        #expect(sections[2].rows.isEmpty)
        let apiRow = sections[1].rows[0]
        #expect(apiRow.status == .waiting)
        #expect(sections[1].status == .waiting)
        #expect(apiRow.counts == [StatusCount(status: .waiting, count: 1), StatusCount(status: .working, count: 2)])
        #expect(sections[0].rows[0].status == .done)

        navigation.archived = ["/api"]
        let archived = SidebarRows.sections(
            projects: tree.projects, workspaces: workspaces, navigation: navigation, search: "",
            showingArchive: true, now: 61
        )
        #expect(archived.map(\.kind) == [.project(path: "/api")])
        #expect(archived[0].rows.filter { $0.kind == .workspace }.map(\.id) == ["/api"])
        #expect(SidebarRows.sections(
            projects: tree.projects, workspaces: workspaces, navigation: navigation, search: "docs",
            showingArchive: false, now: 61
        ).flatMap(\.rows).filter { $0.kind == .workspace }.map(\.id) == ["/docs"])
    }

    @Test("Workspaces sit under their project in activity order, titled without the project name")
    func projectSections() {
        var feature = session("feature-session", cwd: "/repo/wt/feature")
        feature.lastMessageAt = 20
        var main = session("main-session", cwd: "/repo/wt/main")
        main.lastMessageAt = 10
        let notes = session("notes-session", cwd: "/notes")
        let tree = build([main, feature, notes], projectPaths: ["/repo/wt/main", "/empty"])
        var navigation = WorkspaceNavigation()
        let sections = SidebarRows.sections(
            projects: tree.projects, workspaces: WorkspaceEntry.list(in: tree), navigation: navigation,
            search: "", showingArchive: false, now: 61
        )
        #expect(sections.map(\.kind) == [
            .project(path: "/empty"), .project(path: "/notes"), .project(path: "/repo"),
        ])
        // A plain folder is its own one workspace.
        #expect(sections[0].rows.map(\.title) == ["empty"])
        #expect(sections[2].rows.filter { $0.kind == .workspace }.map(\.id) == ["/repo/wt/feature", "/repo/wt/main"])
        #expect(sections[2].rows.filter { $0.kind == .workspace }.map(\.title) == ["feature", "main"])
        #expect(sections[1].rows.filter { $0.kind == .workspace }.map(\.title) == ["notes"])

        // The palette has no headers, so it keeps "project / folder".
        let listed = PaletteSource.workspaces(WorkspaceEntry.list(in: tree), navigation: navigation, now: 61)
        #expect(listed.workspaces.first { $0.id == "/repo/wt/main" }?.title == "repo / main")

        navigation.names["/repo/wt/main"] = "Fix login"
        let renamed = SidebarRows.sections(
            projects: tree.projects, workspaces: WorkspaceEntry.list(in: tree), navigation: navigation,
            search: "", showingArchive: false, now: 61
        )
        let row = renamed[2].rows.filter { $0.kind == .workspace }[1]
        #expect(row.title == "Fix login")
        #expect(row.detail == "main · 1 chat")

        #expect(tree.project(containing: "/repo/wt/main/src")?.path == "/repo")
        #expect(tree.project(containing: "/empty")?.path == "/empty")
        #expect(tree.project(containing: "/elsewhere") == nil)
    }

    @Test("A path takes the deepest project, not a plain folder above it")
    func deepestProject() {
        // The plain folder "/work" holds the repository "/work/app"; a chat in the repo is the repo's.
        let tree = SessionsTree.build(
            sessions: [session("app-chat", cwd: "/work/app")], projectPaths: ["/work"],
            repositoryPathsResolver: { path in
                path == "/work/app" ? GitRepositoryPaths(gitDirectory: "/work/app/.git", commonDirectory: "/work/app/.git") : nil
            },
            worktreeLister: { _ in [WorktreeEntry(path: "/work/app", branch: "main")] }
        )
        #expect(tree.project(containing: "/work/app/src")?.path == "/work/app")
        #expect(tree.project(containing: "/work/notes")?.path == "/work")
    }

    @Test("A bare clone kept as a folder and its repository share a path but not a section")
    func sharedPath() {
        let folderSide = WorkspaceNode(path: "/x/app.git", name: "app.git", sessions: [])
        let worktree = WorkspaceNode(path: "/x/wt/feat", name: "feat", sessions: [], branch: "feat")
        let folder = ProjectNode(id: .folder("/x/app.git"), path: "/x/app.git", launchDirectory: "/x/app.git", workspaces: [folderSide])
        let repository = ProjectNode(
            id: .repository(commonDirectory: "/x/app.git"), path: "/x/app.git", launchDirectory: "/x/wt/feat",
            workspaces: [worktree]
        )
        let tree = SessionsTree(projects: [folder, repository])
        let sections = SidebarRows.sections(
            projects: tree.projects, workspaces: WorkspaceEntry.list(in: tree),
            navigation: WorkspaceNavigation(), search: "", showingArchive: false, now: 61
        )
        #expect(Set(sections.map(\.id)).count == 2)
        #expect(sections.map { $0.rows.map(\.id) } == [["/x/app.git"], ["/x/wt/feat"]])
        #expect(sections.map(\.id) == tree.projects.map(SidebarSection.id(of:)))
    }

    @Test("Two projects with one name show their parent folder")
    func duplicateProjectNames() {
        let tree = build([
            session("work-app", cwd: "/work/app"), session("play-app", cwd: "/play/app"),
        ])
        let sections = SidebarRows.sections(
            projects: tree.projects, workspaces: WorkspaceEntry.list(in: tree),
            navigation: WorkspaceNavigation(), search: "", showingArchive: false, now: 61
        )
        #expect(sections.map(\.title) == ["app — play", "app — work"])
    }

    @Test("The palette lists archived workspaces and their chats, marked Archived")
    func paletteArchived() {
        let api = session("api-session", cwd: "/api")
        let docs = session("docs-session", cwd: "/docs")
        let tree = build([api, docs], agentsBySession: [
            api.id: [SwarmAgent(id: .init("coder"), role: "code", pane: "%1", alive: true)],
            docs.id: [SwarmAgent(id: .init("writer"), role: "write", pane: "%2", alive: true)],
        ])
        var navigation = WorkspaceNavigation()
        navigation.archived = ["/docs"]
        let listed = PaletteSource.workspaces(WorkspaceEntry.list(in: tree), navigation: navigation, now: 61)
        #expect(listed.workspaces.map(\.id) == ["/api", "/docs"])
        #expect(listed.workspaces[1].detail.hasSuffix("Archived"))
        #expect(listed.chats.map(\.id) == ["api-session", "docs-session"])
    }

    @Test("Chat tabs carry status, a provider badge, and whether they can close")
    func chatTabs() {
        let live = session("live-session", cwd: "/api", chair: "chair-1")
        let tree = build([live], agentsBySession: [live.id: [SwarmAgent(
            id: .init("orchestrator"), role: "chair", pane: "%1", alive: true, state: "working"
        )]])
        let chats = tree.workspaceChats(for: live.id)
        let tab = ChatTab.tabs(chats, closing: [], now: 61)[0]
        #expect(tab.id == "live-session")
        #expect(tab.status == .working)
        #expect(tab.badge == "X")
        #expect(tab.canClose)
        #expect(!ChatTab.tabs(chats, closing: [live.id], now: 61)[0].canClose)
        #expect(ChatTab.badge("claude") == "C")
        #expect(ChatTab.badge("agy") == "A")
    }

    @Test("A missing session provider comes from the chair, then the first agent")
    func providerFallback() {
        let item = session("provider-session", cwd: "/outside")
        let worker = SwarmAgent(
            id: .init("worker"), role: "code", pane: "%2", alive: true, provider: "claude"
        )
        let chair = SwarmAgent(
            id: .init("orchestrator"), role: "chair", pane: "%1", alive: true, provider: "codex"
        )
        let withChair = build([item], agentsBySession: [item.id: [worker, chair]])
        #expect(SessionRowPresentation.make(
            withChair.projects[0].chats[0], now: 61
        ).caption == "outside · codex")
        #expect(withChair.windowTitle(for: item.id) == "outside · codex provider")

        let withoutChair = build([item], agentsBySession: [item.id: [worker]])
        #expect(SessionRowPresentation.make(
            withoutChair.projects[0].chats[0], now: 61
        ).caption == "outside · claude")
    }

    @Test("Live, no-chair, and ended rows sort by state then recent activity")
    func rowOrdering() {
        let rows = [
            projectSession("ended-new", provider: "codex", running: false, activity: 50),
            projectSession("no-chair", provider: nil, running: true, activity: 40),
            projectSession("live-old", provider: "codex", running: true, activity: 10),
            projectSession("ended-old", provider: "codex", running: false, activity: 20),
            projectSession("live-new", provider: "codex", running: true, activity: 30),
        ]
        #expect(SessionsTree.ordered(rows).map(\.id.rawValue)
            == ["live-new", "live-old", "no-chair", "ended-new", "ended-old"])
    }

    @Test("Workspaces sort by state and then recent activity")
    func workspaceOrdering() {
        let workspaces = [
            WorkspaceNode(path: "/ended", name: "ended", sessions: [
                projectSession("ended", provider: "codex", running: false, activity: 50),
            ]),
            WorkspaceNode(path: "/no-chair", name: "no-chair", sessions: [
                projectSession("no-chair", provider: nil, running: true, activity: 60),
            ]),
            WorkspaceNode(path: "/live-old", name: "live-old", sessions: [
                projectSession("live-old", provider: "codex", running: true, activity: 10),
            ]),
            WorkspaceNode(path: "/live-new", name: "live-new", sessions: [
                projectSession("live-new", provider: "codex", running: true, activity: 30),
            ]),
        ]
        #expect(SessionsTree.ordered(workspaces).map(\.name)
            == ["live-new", "live-old", "no-chair", "ended"])
    }

    @Test("Archived sessions and empty worktrees are hidden")
    func archived() {
        let tree = build([
            session("active", cwd: "/repo/wt/main"),
            session("archived", cwd: "/repo/wt/feature", archivedAt: 50),
        ])
        #expect(tree.projects[0].workspaces.map(\.path) == ["/repo/wt/main"])
        #expect(tree.session(SwarmSessionID("archived")) == nil)
    }

    @Test("Repeated sessions in one chair make one row")
    func chairGroup() {
        let tree = build([
            session("older", cwd: "/repo/wt/main", chair: "chair"),
            session("newer", cwd: "/repo/wt/main", chair: "chair"),
        ])
        let workspace = tree.projects[0].workspaces[0]
        let rows = workspace.sessions
        #expect(rows.count == 1)
        #expect(rows[0].sessions.count == 2)
        #expect(Set(tree.archiveIDs(for: rows[0].id))
            == [SwarmSessionID("older"), SwarmSessionID("newer")])
        #expect(Set(tree.archiveIDs(for: SwarmSessionID("older")))
            == [SwarmSessionID("older"), SwarmSessionID("newer")])
        #expect(tree.session(rows[0].id)?.id == rows[0].id)
        #expect(tree.retainedSelection(SwarmSessionID("newer")) == rows[0].id)
        #expect(tree.retainedSelection(SwarmSessionID("gone")) == nil)
        #expect(tree.windowTitle(for: rows[0].id) == "repo · codex \(rows[0].id.rawValue.prefix(8))")
    }

    @Test("A provider switch keeps both sessions in one chat")
    func continuedChat() {
        var old = session("old", cwd: "/repo/wt/main", chair: "claude-chat")
        old.chairProvider = "claude"
        var next = session("next", cwd: "/repo/wt/main", chair: "codex-chat")
        next.chairProvider = "codex"
        next.continuationOf = old.id
        next.createdAt = 2
        let tree = build([old, next], titles: [old.id: "Fix the menu", next.id: "Context from prior chat"])
        let row = tree.session(next.id)
        #expect(row?.sessions.map(\.id) == [next.id, old.id])
        #expect(row?.title == "Fix the menu")
        #expect(tree.archiveIDs(for: next.id) == [next.id, old.id])
    }

    @Test("A new session remains the chat row when an older session has later activity")
    func newestSessionOwnsChat() {
        var older = session("older", cwd: "/repo/wt/main", chair: "chair")
        older.lastMessageAt = 100
        var newer = session("newer", cwd: "/repo/wt/main", chair: "chair")
        newer.createdAt = 50
        let tree = build([older, newer])
        #expect(tree.session(older.id)?.id == newer.id)
        #expect(tree.session(newer.id)?.session == newer)
    }

    @Test("The bare repository is one project for its linked worktrees")
    func bareLayout() {
        let tree = build([
            session("first", cwd: "/repo/wt/main/a"),
            session("second", cwd: "/repo/wt/feature/b"),
        ])
        #expect(tree.projects.map(\.id) == [.repository(commonDirectory: common)])
    }

    @Test("A hub session stays in its repository and launches from main")
    func hubSession() {
        let tree = build([
            session("hub", cwd: "/repo/.bare"),
            session("main", cwd: "/repo/wt/main"),
        ])
        #expect(tree.projects.count == 1)
        #expect(tree.projects[0].workspaces.map(\.name) == ["main", "repo"])
        #expect(tree.projects[0].launchDirectory == "/repo/wt/main")
        #expect(tree.launchDirectory(for: SwarmSessionID("hub")) == "/repo")
    }

    @Test("Tree text prints project and chat presentation")
    func treeText() {
        let tree = build([
            session("hub", cwd: "/repo/.bare"),
            session("main", cwd: "/repo/wt/main"),
        ])
        #expect(tree.text(now: 61) == """
            repo
              Chat main · main · no chair · 1m
              Chat hub · repo · no chair · 1m
            """)
    }

    @Test("A .bare directory identifies its repository")
    func bareDirectory() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: root) }
        try FileManager.default.createDirectory(
            at: root.appendingPathComponent(".bare"), withIntermediateDirectories: true
        )
        #expect(Git.repositoryPaths(in: root.path)?.commonDirectory == root.appendingPathComponent(".bare").path)
    }

    @Test("A .git pointer into .bare identifies the same repository")
    func barePointer() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: root) }
        let gitDirectory = root.appendingPathComponent(".bare/worktrees/main")
        let worktree = root.appendingPathComponent("wt/main")
        try FileManager.default.createDirectory(at: gitDirectory, withIntermediateDirectories: true)
        try FileManager.default.createDirectory(at: worktree, withIntermediateDirectories: true)
        try "../..\n".write(to: gitDirectory.appendingPathComponent("commondir"), atomically: true, encoding: .utf8)
        try "gitdir: \(gitDirectory.path)\n".write(
            to: worktree.appendingPathComponent(".git"), atomically: true, encoding: .utf8
        )
        #expect(Git.repositoryPaths(in: worktree.path)?.commonDirectory == root.appendingPathComponent(".bare").path)
    }

    @Test("The agent list keeps provider and pane state")
    func agentList() throws {
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        let list = try decoder.decode(SwarmAgentList.self, from: Data(#"{"agents":[{"id":"coder","role":"code","provider":"codex","pane":"p1","alive":true}]}"#.utf8))
        #expect(list.agents[0].provider == "codex")
        #expect(list.agents[0].alive == true)
    }

    private func build(
        _ sessions: [SwarmSession], projectPaths: [String] = [],
        agentsBySession: [SwarmSessionID: [SwarmAgent]] = [:],
        titles: [SwarmSessionID: String] = [:]
    ) -> SessionsTree {
        SessionsTree.build(
            sessions: sessions, projectPaths: projectPaths,
            agentsBySession: agentsBySession, titles: titles,
            repositoryPathsResolver: { path in
                guard path.hasPrefix("/repo") else { return nil }
                return GitRepositoryPaths(gitDirectory: common + "/worktrees/test", commonDirectory: common)
            },
            worktreeLister: { _ in worktrees }
        )
    }

    private func session(
        _ id: String, cwd: String, chair: String? = nil, archivedAt: Int? = nil
    ) -> SwarmSession {
        SwarmSession(
            id: SwarmSessionID(id), talkMode: "lane", adapter: "tmux-solo", cwd: cwd,
            createdAt: 1, chairProvider: chair == nil ? nil : "codex",
            chairID: chair.map(SwarmChairID.init), chairLog: nil,
            agents: 1, messages: 0, lastMessageAt: nil, archivedAt: archivedAt
        )
    }

    private func projectSession(
        _ id: String, title: String = "Chat", provider: String?, running: Bool?,
        liveAgents: Int? = nil, activity: Int? = nil
    ) -> SwarmProjectSession {
        var item = session(id, cwd: "/outside")
        item.lastMessageAt = activity
        return SwarmProjectSession(
            sessions: [item], title: title, isRunning: running,
            liveAgents: liveAgents, provider: provider
        )
    }

    private func chat(_ session: SwarmProjectSession) -> ChatRow {
        ChatRow(session: session, workspace: "outside", workspacePath: "/outside")
    }
}

private actor TreeAgentCalls {
    var arguments: [[String]] = []
    var sessionIDs: [String?] = []

    func record(_ arguments: [String], environment: [String: String]) {
        self.arguments.append(arguments)
        sessionIDs.append(environment["SWARM_SESSION_ID"])
    }
}

private actor WorktreeListingCalls {
    var paths: [String] = []
    var entries: [WorktreeEntry]?
    func set(_ entries: [WorktreeEntry]) { self.entries = entries }
    func list(_ path: String) -> [WorktreeEntry] {
        paths.append(path)
        return entries ?? [WorktreeEntry(path: path + "/main", branch: "main")]
    }
}

private actor SuspendedWorktreeListing {
    var count = 0
    private var pending: CheckedContinuation<[WorktreeEntry], Never>?

    func list(_ path: String) async -> [WorktreeEntry] {
        count += 1
        if count == 1 {
            return await withCheckedContinuation { pending = $0 }
        }
        return [WorktreeEntry(path: path + "/new", branch: "new")]
    }

    func waitUntilStarted() async {
        while pending == nil { await Task.yield() }
    }

    func resume() {
        pending?.resume(returning: [WorktreeEntry(path: "/repo-one/old", branch: "old")])
        pending = nil
    }
}
