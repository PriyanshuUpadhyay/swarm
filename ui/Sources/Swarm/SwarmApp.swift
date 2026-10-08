import Foundation
import Observation
import OSLog
import SwiftUI
import SwarmCore

@MainActor @Observable
final class SessionsTreeModel {
    private let logger = Logger(subsystem: "io.github.priyanshuupadhyay.swarm", category: "refresh")
    private let bus = SwarmCLIBus()
    private let discovery = SwarmSessionDiscovery()
    private let drafts = ComposerDraftStore()
    private let ownerChoices = OwnerChoicesStore()
    private let projects: SwarmProjectStore
    private let navigationStore: WorkspaceNavigationStore
    var navigation = WorkspaceNavigation() {
        didSet {
            if navigation != oldValue {
                if let failure = navigationStore.save(navigation) {
                    reportChoicesError(failure)
                }
            }
            if navigation.workspaceOrder != oldValue.workspaceOrder {
                workspaces = WorkspaceEntry.list(in: tree, workspaceOrder: navigation.workspaceOrder)
            }
        }
    }

    var choicesAlerts: OwnerChoicesAlerts {
        get { ownerChoices.alerts }
        set { ownerChoices.alerts = newValue }
    }

    private func reportChoicesError(_ failure: OwnerChoicesFailure) {
        var reported = choicesAlerts
        guard reported.report(failure) else { return }
        choicesAlerts = reported
        logger.error("\(failure.message)")
    }

    init() {
        projects = SwarmProjectStore(choices: ownerChoices)
        navigationStore = WorkspaceNavigationStore(defaults: .standard, choices: ownerChoices)
        navigation = navigationStore.load()
        if let message = choicesAlerts.message { logger.error("\(message)") }
    }

    /// Rebuilt when the tree changes, not on every read.
    private(set) var workspaces: [WorkspaceEntry] = []
    var selectedWorkspace: WorkspaceEntry? {
        workspaces.first { $0.id == navigation.selectedWorkspace }
    }

    func selectWorkspace(_ entry: WorkspaceEntry) {
        navigation.select(entry)
        expandProject(of: entry)
        // A workspace whose only tabs are starts opens on the newest, so its Retry and Close show.
        if navigation.selectedChat(in: entry) == nil, let start = pendingChats.inWorkspace(entry.id).last {
            selectPending(start.id)
        } else {
            select(navigation.selectedChat(in: entry)?.id)
        }
    }

    /// A selected workspace in a collapsed project would be a hidden row; a pinned row never is.
    private func expandProject(of entry: WorkspaceEntry) {
        guard !navigation.pinned.contains(entry.id),
              let project = tree.project(containing: entry.id) else { return }
        navigation.collapsed.remove(WorkspaceNavigation.projectCollapseID(project.path))
    }

    func showHome() {
        navigation.selectedWorkspace = nil
        select(nil)
    }

    func archiveWorkspace(_ entry: WorkspaceEntry) {
        let wasSelected = navigation.selectedWorkspace == entry.id
        navigation.archive(entry.id)
        if wasSelected { select(nil) }
    }

    private var sourceTree = SessionsTree(projects: [])
    private var archives = ChatArchives()
    private var refreshRevision = 0
    private(set) var selectionRevision = 0
    var tree = SessionsTree(projects: []) {
        didSet { workspaces = WorkspaceEntry.list(in: tree, workspaceOrder: navigation.workspaceOrder) }
    }
    /// False until the first tree loads, so the sidebar does not claim "No projects yet" early.
    private(set) var hasLoaded = false
    let detailModels = SessionDetailStore()
    var selectedSessionID: SwarmSessionID? {
        didSet {
            if selectedSessionID != oldValue { detailModels.activate(selectedSessionID) }
        }
    }
    private var pendingID: SwarmSessionID?
    /// Chats being started, each shown as its own tab (ADR 0035).
    private(set) var pendingChats = PendingChats()
    /// A pending chat's tab is selected; then `selectedSessionID` is nil.
    private(set) var selectedPendingID: UUID?
    var agents: [SwarmAgent] = []
    private(set) var runsByWorkspace: [String: [StepRun]] = [:]
    private(set) var runNoticesByWorkspace: [String: String] = [:]
    private let rowFieldCache = RowFieldCache()
    private(set) var workspaceFields: [String: RowWorkspaceFields] = [:]
    /// The model `swarm launch` resolved for each chat this app started, shown until the chair's
    /// log reports one.
    private(set) var launchedModels: [SwarmSessionID: String] = [:]
    /// The folder trust each chair launch wrote, which its chat shows (owner answer I1).
    private(set) var launchedTrust: [SwarmSessionID: [SwarmTrustWrite]] = [:]
    var commandSource: ComposerCommandSource?
    private var commandSourceKey: String?
    var error: String?
    /// A create, import or new workspace that went through in part: the list refresh after it failed,
    /// or a new worktree's save failed. The caller opens a chat right after, which hides `error`, so
    /// the view shows this in its alert.
    var saveNotice: String?
    private(set) var closing: Set<SwarmSessionID> = []

    var selectedSession: SwarmProjectSession? { selectedSessionID.flatMap(tree.session) }

    func select(_ id: SwarmSessionID?) {
        if id != nil { SwarmPerformance.event("ChatSelected") }
        selectionRevision += 1
        pendingID = nil
        selectedPendingID = nil
        selectedSessionID = id
        if let id, let entry = workspaces.first(where: {
            $0.chats.contains { $0.sessions.contains { $0.id == id } }
        }) {
            // Only a move to another workspace opens its project; a tab switch keeps a collapse.
            let moved = navigation.selectedWorkspace != entry.id
            navigation.select(entry, chat: id)
            if moved { expandProject(of: entry) }
        }
        agents = id.flatMap { tree.agentsBySession[$0] } ?? []
        commandSource = nil
        commandSourceKey = nil
    }

    /// Starts the chat profile in `directory` at once, behind a pending tab that is selected now.
    func newChat(in directory: String) {
        guard workspaces.first(where: { $0.id == directory })?.workspace.canStartChat != false else { return }
        guard let plan = SwarmChatLaunchPlan(profileIn: directory) else { return }
        let previous = selectedPendingID.map(PendingChat.Previous.pending)
            ?? selectedSessionID.map(PendingChat.Previous.session)
        // The deepest workspace that holds the directory; a project opened inside a repository
        // launches in a folder that is no workspace id.
        let workspace = workspaces.map(\.id)
            .filter { directory == $0 || directory.hasPrefix($0 + "/") }
            .max { $0.count < $1.count } ?? Self.hubWorkspace(for: directory) ?? directory
        let id = pendingChats.add(directory: directory, workspace: workspace, previous: previous)
        navigation.archived.remove(workspace)
        navigation.selectedWorkspace = workspace
        if let entry = workspaces.first(where: { $0.id == workspace }) { expandProject(of: entry) }
        selectPending(id)
        runStart(id, plan: plan)
    }

    /// The tree with archived rows and the sessions of starts left out.
    private var visibleTree: SessionsTree {
        archives.applying(to: sourceTree, hiding: pendingChats.sessions)
    }

    /// Selects the newest start of the selected workspace, so a failed one stays reachable when
    /// the last chat beside it goes. Returns false when the workspace has none.
    private func selectNewestStart() -> Bool {
        guard let directory = navigation.selectedWorkspace,
              let start = pendingChats.inWorkspace(directory).last else { return false }
        selectPending(start.id)
        return true
    }

    /// A chat in a hub root uses the hub folder before discovery adds its workspace row.
    private static func hubWorkspace(for directory: String) -> String? {
        // Only the hub root holds `.bare`; a worktree of the hub has a `.git` file instead.
        guard FileManager.default.fileExists(atPath: (directory as NSString).appendingPathComponent(".bare")),
              let common = Git.repositoryPaths(in: directory)?.commonDirectory,
              URL(fileURLWithPath: common).lastPathComponent == ".bare" else { return nil }
        return URL(fileURLWithPath: common).deletingLastPathComponent().path
    }

    func selectPending(_ id: UUID) {
        guard let chat = pendingChats[id] else { return }
        selectionRevision += 1
        pendingID = nil
        navigation.selectedWorkspace = chat.workspace
        selectedSessionID = nil
        selectedPendingID = id
        agents = []
        commandSource = nil
        commandSourceKey = nil
    }

    /// Runs `launch` again, in the session the failed start made if it made one.
    func retryChat(_ id: UUID) {
        guard let chat = pendingChats[id], case .failed = chat.state,
              let plan = SwarmChatLaunchPlan(profileIn: chat.directory) else { return }
        pendingChats.update(id) { $0.state = .starting }
        runStart(id, plan: plan)
    }

    /// Archives the session a failed start made, then drops the start and selects the tab that
    /// was selected before it. An archive error keeps the tab, with its Retry and Close.
    func discardChat(_ id: UUID) async throws {
        guard let start = pendingChats[id], case .failed(let failure) = start.state else { return }
        if let session = start.session {
            pendingChats.update(id) { $0.state = .closing(failure) }
            do {
                // A launch can fail after swarm registered the chair (no pane line, or a timeout).
                try? await bus.close(SwarmPanePolicy.chair, in: session, adapter: SwarmSessionInteraction.defaultAdapter)
                try await bus.archive([session])
            } catch {
                pendingChats.update(id) { $0.state = .failed(failure) }
                throw error
            }
            // The tree drops the archived row while the start still hides it, so it never shows.
            try? await refresh()
        }
        guard let chat = pendingChats.remove(id), selectedPendingID == id else { return }
        switch chat.previous {
        case .pending(let start) where pendingChats[start] != nil:
            selectPending(start)
        case .session(let row) where tree.retainedSelection(row) != nil:
            select(row)
        default:
            let saved = selectedWorkspace.flatMap { navigation.selectedChat(in: $0)?.id }
            if saved != nil || !selectNewestStart() { select(saved) }
        }
    }

    private func runStart(_ id: UUID, plan: SwarmChatLaunchPlan) {
        Task {
            do {
                let session: SwarmSessionID
                if let made = pendingChats[id]?.session {
                    // A Retry: a failed launch may have registered the chair, which would make
                    // this launch fail too.
                    try? await bus.close(SwarmPanePolicy.chair, in: made, adapter: SwarmSessionInteraction.defaultAdapter)
                    session = made
                } else {
                    session = try await SwarmChatLauncher.create(plan, bus: bus)
                    pendingChats.update(id) { $0.session = session }
                }
                let launch = try await SwarmChatLauncher.launch(plan, in: session, bus: bus)
                launchedModels[session] = launch.model
                launchedTrust[session] = launch.trustWrites
                // Said here, as a failure is: the chat may not be the selected tab (UA-11).
                if let said = launch.trustAnnouncement {
                    AccessibilityNotification.Announcement(said).post()
                }
                pendingChats.update(id) { $0.state = .launched }
                try await refresh()
            } catch {
                let failure = LaunchFailure(error)
                guard pendingChats[id]?.state == .starting else { return }
                pendingChats.update(id) { $0.state = .failed(failure) }
                // Said here, not by the view: the failed tab may not be the selected one.
                let first = failure.message.split(separator: "\n").first.map(String.init) ?? ""
                AccessibilityNotification.Announcement("Could not start the chat. \(first)").post()
            }
        }
    }

    func switchChat(
        _ plan: SwarmChatLaunchPlan, from row: SwarmProjectSession,
        onProgress: @escaping @Sendable (ChatSwitchPhase) async -> Void
    ) async throws -> SwarmSessionID {
        let id = try await SwarmChatHandoff.start(plan, after: row, bus: bus, onProgress: onProgress)
        selectionRevision += 1
        pendingID = id
        selectedPendingID = nil
        selectedSessionID = id
        if let path = navigation.selectedWorkspace { navigation.selectedChats[path] = id.rawValue }
        agents = []
        return id
    }

    func refresh() async throws {
        refreshRevision += 1
        let revision = refreshRevision
        let timing = SwarmPerformance.begin("UIRefresh")
        defer { timing.end(count: tree.projects.count) }
        let sessions: [SwarmSession]
        do {
            let listTiming = SwarmPerformance.begin("SessionList")
            defer { listTiming.end() }
            sessions = try await bus.sessions()
        }
        guard revision == refreshRevision else { return }
        var settled: [PendingChat] = []
        drafts.prune(keeping: Set(sessions.map { $0.id.rawValue }))
        do {
            let treeTiming = SwarmPerformance.begin("WorkspaceTree")
            defer { treeTiming.end(count: sessions.count) }
            let choicesRevision = navigationStore.choicesRevision
            let saved = projects.loadChoices(reportError: reportChoicesError) ?? navigationStore.savedChoices
            let choicesLoaded = !projects.choicesLoadFailed
            let loaded = try await discovery.tree(
                sessions: sessions, projectPaths: saved.projectPaths, removed: saved.removedProjects, bus: bus
            )
            guard revision == refreshRevision else { return }
            var refreshed = navigation
            if choicesLoaded {
                refreshed = projects.refreshChoices(
                    shown: loaded.projects, navigation: navigation, saved: saved, loadedAtRevision: choicesRevision,
                    navigationStore: navigationStore,
                    reportError: reportChoicesError
                )
            }
            refreshed.recordFirstSight(loaded.projects.flatMap { $0.chats.map(\.session) })
            navigation = refreshed
            sourceTree = loaded
            archives.reconcile(loaded)
            settled = pendingChats.settle(listed: { loaded.session($0) != nil })
            tree = visibleTree
            hasLoaded = true
        }
        // A start that the owner left selected selects its chat; one they moved away from does not.
        for chat in settled where chat.id == selectedPendingID {
            select(chat.session)
        }
        if pendingID == nil, selectedPendingID == nil, let entry = selectedWorkspace {
            selectedSessionID = navigation.selectedChat(in: entry)?.id
        }
        if let selectedSessionID, let row = tree.session(selectedSessionID) {
            pendingID = nil
            self.selectedSessionID = row.id
            if let entry = workspaces.first(where: { $0.chats.contains { $0.id == row.id } }) {
                navigation.select(entry, chat: row.id, now: row.lastActivity)
            }
            do {
                let agentTiming = SwarmPerformance.begin("SelectedAgents")
                defer { agentTiming.end() }
                let loaded = try await bus.agents(in: row.session)
                guard revision == refreshRevision, self.selectedSessionID == row.id else { return }
                agents = loaded
            }
            let provider = row.provider ?? agents.first {
                $0.id == SwarmPanePolicy.chair
            }?.provider
            let key = row.id.rawValue + (row.session.chairLog ?? "") + (provider ?? "")
            if commandSourceKey != key {
                let commandTiming = SwarmPerformance.begin("ComposerCommands")
                let source = await discovery.composerCommandSource(
                    for: row.session, provider: provider
                )
                commandTiming.end()
                if revision == refreshRevision, self.selectedSessionID == row.id {
                    commandSource = source
                    commandSourceKey = key
                }
            }
        } else if pendingID == nil {
            selectedSessionID = tree.retainedSelection(selectedSessionID)
            agents = []
            commandSource = nil
            commandSourceKey = nil
            // The selected chat closed while its workspace still has starts; the newest one shows.
            if selectedSessionID == nil, selectedPendingID == nil { _ = selectNewestStart() }
        }
        error = nil
    }

    /// `initializeGit` runs `git init` in a plain folder first, after the owner agreed to it.
    func openProject(_ url: URL, initializeGit: Bool) async throws -> String {
        let timing = SwarmPerformance.begin("ProjectOpen")
        defer { timing.end() }
        if initializeGit {
            try await Git.initialize(at: url.path)
            await discovery.forgetIdentities()
        }
        let path = try await projects.add(url)
        await refreshAfterSave("Project saved")
        return path
    }

    func createProject(at url: URL) async throws -> String {
        let timing = SwarmPerformance.begin("ProjectCreate")
        defer { timing.end() }
        let path = try await projects.create(at: url)
        // A path made again after a delete can still be cached as a plain folder.
        await discovery.forgetIdentities()
        await refreshAfterSave("Project saved")
        return path
    }

    func removeProject(_ project: ProjectNode) throws {
        let saved = try projects.remove(project.path, workspacePaths: project.workspaces.map(\.path))
        navigation = navigationStore.adopt(saved, into: navigation)
        let removedWorkspaces = Set(project.workspaces.map(\.path))
        navigation.selectedChats = navigation.selectedChats.filter { !removedWorkspaces.contains($0.key) }
        refreshRevision += 1
        let wasSelected = selectedWorkspace?.project.path == project.path
        sourceTree = SessionsTree(
            projects: sourceTree.projects.filter { $0.path != project.path },
            agentsBySession: sourceTree.agentsBySession
        )
        tree = visibleTree
        if wasSelected { showHome() }
    }

    /// Makes a plain-folder project a git repository and returns it as one. The node comes from
    /// git, not the tree: a later refresh can overtake this one and leave the tree without it.
    func initializeGit(at path: String) async throws -> ProjectNode {
        try await Git.initialize(at: path)
        await discovery.forgetIdentities()
        try await refresh()
        guard let paths = Git.repositoryPaths(in: path) else { throw GitTaskWorktreeError.notRepository }
        return ProjectNode(
            id: .repository(commonDirectory: paths.commonDirectory), path: path,
            launchDirectory: path, workspaces: []
        )
    }

    func createTask(named name: String, in project: ProjectNode) async throws -> String {
        let timing = SwarmPerformance.begin("TaskCreate")
        defer { timing.end() }
        guard case .repository(let common) = project.id else {
            throw GitTaskWorktreeError.notRepository
        }
        let root = URL(fileURLWithPath: project.path)
        let parent = URL(fileURLWithPath: common).lastPathComponent == ".bare"
            ? root.appendingPathComponent("wt", isDirectory: true)
            : root.deletingLastPathComponent()
                .appendingPathComponent(project.name + "-worktrees", isDirectory: true)
        let repositoryDirectory = URL(fileURLWithPath: common).lastPathComponent == ".bare"
            ? common : project.path
        let path = try await GitTaskWorktree.create(
            named: name, in: repositoryDirectory,
            commonDirectory: common, under: parent.path
        )
        await discovery.forgetWorktrees(for: common)
        // Git already made the worktree, so a failed save must not ask the owner to create it again.
        do { try await projects.add(URL(fileURLWithPath: path)) }
        catch {
            // The cause can say "try again", which here would make a second worktree, so it is only the reason.
            addSaveNotice("The workspace at \(path) exists, so do not create it again. Swarm could not record it."
                + "\n\nReason: \(error.localizedDescription)")
        }
        navigation.names[path] = name.trimmingCharacters(in: .whitespacesAndNewlines)
        // The caller starts a chat there, which selects the workspace. Selecting it here, before the
        // tree lists it, showed the Agent Profiles page and its availability check for a moment.
        await refreshAfterSave("Workspace made")
        return path
    }

    private func refreshAfterSave(_ subject: String) async {
        do { try await refresh() }
        catch { addSaveNotice("\(subject), but the list could not refresh. \(error.localizedDescription)") }
    }

    private func addSaveNotice(_ notice: String) {
        saveNotice = saveNotice.map { "\($0)\n\n\(notice)" } ?? notice
    }

    func pruneWorktree(_ entry: WorkspaceEntry) async throws {
        guard entry.workspace.missing, case .repository(let common) = entry.project.id else { return }
        try await Git.pruneWorktrees(in: common)
        await discovery.forgetWorktrees(for: common)
        try await refresh()
    }

    func archive(_ id: SwarmSessionID) async throws {
        let ids = archives.begin(id, in: tree)
        guard !ids.isEmpty else { return }
        let previous = selectedSessionID
        let workspace = navigation.selectedWorkspace
        let next = ChatArchives.selection(afterArchiving: id, selected: previous, in: tree)
        refreshRevision += 1
        tree = visibleTree
        // With no chat left, a start in the workspace keeps the strip and its Retry and Close.
        if next != previous, next != nil || !selectNewestStart() { select(next) }
        let revision = selectionRevision
        SwarmPerformance.event("ChatArchiveApplied")
        do {
            try await bus.archive(ids)
            archives.finish(id, succeeded: true)
            refreshRevision += 1
        } catch {
            archives.finish(id, succeeded: false)
            refreshRevision += 1
            tree = visibleTree
            if selectionRevision == revision, navigation.selectedWorkspace == workspace,
               let previous, tree.session(previous) != nil {
                select(previous)
            }
            throw error
        }
    }

    func close(_ id: SwarmSessionID) async throws {
        guard let session = tree.session(id)?.session, closing.insert(id).inserted else { return }
        defer { closing.remove(id) }
        try await SwarmSessionCloser.close(session, bus: bus)
        try await refresh()
    }

    func run() async {
        // Account homes load beside the first refresh, not after it: the first chat can open as
        // soon as the tree arrives, and a lookup still running then cost that open about 120 ms.
        let homes = Task.detached { [discovery] in await discovery.prefetchHomes() }
        defer { homes.cancel() }
        var first = true
        while !Task.isCancelled {
            let timing = SwarmPerformance.begin(first ? "InitialRefresh" : "RefreshTick")
            do { try await refresh() }
            catch { self.error = error.localizedDescription }
            timing.end()
            await discovery.prefetchHomes()
            first = false
            try? await Task.sleep(for: .seconds(2))
        }
    }

    func runRowFields() async {
        while !Task.isCancelled {
            let paths = RowFields.requestedPaths(for: .dirty, entries: workspaces, fields: navigation.fields)
            let githubPaths = Set(RowFields.requestedPaths(for: .pr, entries: workspaces, fields: navigation.fields))
                .union(RowFields.requestedPaths(for: .ci, entries: workspaces, fields: navigation.fields))
            let read = await rowFieldCache.refresh(paths: paths, githubPaths: Array(githubPaths))
            guard !Task.isCancelled else { return }
            let updated = read.filter { value in workspaces.contains { $0.id == value.key } }
            if workspaceFields != updated { workspaceFields = updated }
            try? await Task.sleep(for: .seconds(RowFieldCache.refreshInterval))
        }
    }

    func runSidebarRuns() async {
        while !Task.isCancelled {
            let paths = SidebarRows.runWorkspaces(workspaces, navigation: navigation)
            let scan = Task.detached(priority: .utility) {
                await SidebarRows.scanRuns(in: paths)
            }
            let read = await withTaskCancellationHandler {
                await scan.value
            } onCancel: {
                scan.cancel()
            }
            guard !Task.isCancelled else { return }
            let existing = Set(workspaces.map(\.id))
            var updated = runsByWorkspace.filter { existing.contains($0.key) }
            updated.merge(read.mapValues(\.runs), uniquingKeysWith: { _, new in new })
            if runsByWorkspace != updated { runsByWorkspace = updated }
            var notices = runNoticesByWorkspace.filter { existing.contains($0.key) }
            for (path, scan) in read { notices[path] = scan.notice }
            if runNoticesByWorkspace != notices { runNoticesByWorkspace = notices }
            try? await Task.sleep(for: .seconds(RowFieldCache.refreshInterval))
        }
    }


}

private struct SessionsWindow: View {
    @State private var model = SessionsTreeModel()
    @State private var panes = AgentPaneStore()
    @State private var expandedLists: Set<String> = []
    /// Applied after the chat switch releases its old columns.
    @State private var sidebarFocus: SidebarSelection?
    @State private var newTaskProject: ProjectNode?
    @State private var switchTarget: SwitchTarget?
    /// Counts the owner's own moves (a sidebar pick, Home, a tab), so Import Project skips its
    /// chat only when the owner went elsewhere, not when a refresh changed the selection.
    @State private var ownerMoves = 0
    /// The alert on screen, and the ones waiting for it to close (see `WindowAlert`).
    @State private var shownAlert: WindowAlert?
    @State private var pendingAlerts: [WindowAlert] = []
    @State private var projectAction: String?
    @State private var showingPalette = false
    /// When each palette action last ran, in this window only.
    @State private var recentActions: [AppKey: Int] = [:]
    @State private var showingArchive = false
    @State private var createSheet: CreateSheet?
    @State private var showingHooksSetup = false
    /// "Not now" on the hooks question of an older build; it still covers the hooks alone, so
    /// the setup sheet opens once for folder trust (ADR 0029, 0043).
    @AppStorage("hooksSetupDeclined") private var hooksSetupDeclined = false
    /// "Not now" on the setup sheet with every box checked; the app menu can still open it
    /// (ADR 0043).
    @AppStorage("setupDeclined") private var setupDeclined = false
    /// Folder trust left unchecked when the owner applied or said "Not now" to the rest of the
    /// setup sheet.
    @AppStorage("trustSetupDeclined") private var trustSetupDeclined = false
    @State private var createAction: (() -> Void)?
    @State private var renameTarget: RenameTarget?
    @State private var renameName = ""
    @AppStorage("workspaceSidebarVisible") private var sidebarVisible = true
    @AppStorage("workspaceSidebarOnRight") private var sidebarOnRight = false
    @AppStorage("workspaceSidebarMode") private var storedSidebarMode = WorkspaceSidebarMode.workspaces.rawValue
    @AppStorage("workspaceSidebarWidth") private var sidebarWidth = 280.0
    @State private var document: WorkspaceDocument?
    @State private var documentVisible = false
    @State private var reportedUsage: (sessionID: SwarmSessionID, usage: ChatUsage)?
    @State private var runRequest: StepRunRequest?
    @State private var runRequestWorkspace: String?

    var body: some View {
        let sections = sidebarSections(showingArchive: showingArchive)
        return MovableSidebar(visible: sidebarVisible, onRight: sidebarOnRight, minimumContentWidth: minimumContentWidth, width: $sidebarWidth) {
            SidebarView(
                mode: sidebarMode,
                sections: sections,
                collapsed: model.navigation.collapsed,
                loaded: model.hasLoaded,
                selectedID: selectedSidebarID(in: sections),
                showingArchive: showingArchive,
                actions: sidebarActions
            ) {
                if let directory = workspaceDirectory {
                    VStack(spacing: 0) {
                        Button {
                            storedSidebarMode = WorkspaceSidebarMode.workspaces.rawValue
                            documentVisible = false
                        } label: {
                            VStack(alignment: .leading, spacing: DesignTokens.Spacing.xxs) {
                                Text(model.selectedWorkspace.map { model.navigation.title(for: $0) } ?? URL(fileURLWithPath: directory).lastPathComponent)
                                    .font(.subheadline.weight(.semibold))
                                Text(verbatim: directory).font(.caption).foregroundStyle(.secondary)
                            }
                            .lineLimit(1).truncationMode(.middle)
                            .frame(maxWidth: .infinity, alignment: .leading).padding(DesignTokens.Spacing.m)
                        }
                        .buttonStyle(.plain).help("Choose a workspace")
                        Divider()
                        WorkspacePanels(
                            directory: directory, mode: sidebarMode, visible: sidebarVisible,
                            usage: reportedUsage?.sessionID == model.selectedSession?.id ? reportedUsage?.usage : nil,
                            hasChat: model.selectedSession != nil, open: openDocument,
                            runRequest: runRequestWorkspace == directory ? runRequest : nil,
                            openedRun: { runRequest = nil }, chatTitles: runChatTitles(in: directory),
                            selectRunChat: { selectRunChat($0, in: directory) }
                        ).id(directory)
                    }
                } else {
                    VStack {
                        ContentUnavailableView("Select a workspace", systemImage: "folder", description: Text("Choose a workspace to see its files and details."))
                        Button("Show workspaces") { storedSidebarMode = WorkspaceSidebarMode.workspaces.rawValue }
                            .padding(DesignTokens.Spacing.l)
                    }
                }
            }
        } content: {
            VStack(spacing: 0) {
                if let projectAction {
                    DelayedProgress(projectAction).padding(DesignTokens.Spacing.s)
                }
                if !model.closing.isEmpty {
                    DelayedProgress("Closing chat…").padding(DesignTokens.Spacing.s)
                }
                if let document {
                    HStack(spacing: DesignTokens.Spacing.l) {
                        Button("Chat") { documentVisible = false }
                            .foregroundStyle(documentVisible ? Color.secondary : Color.primary)
                        Button(document.title) { documentVisible = true }
                            .lineLimit(1).truncationMode(.middle)
                            .foregroundStyle(documentVisible ? Color.primary : Color.secondary)
                        Button { closeDocument() } label: { Image(systemName: "xmark") }
                            .accessibilityLabel("Close file preview")
                        Spacer()
                    }.buttonStyle(.plain).padding(DesignTokens.Spacing.m)
                    Divider()
                }
                ZStack {
                    // Keep the chat's identity and draft while a file is visible or the sidebar moves.
                    workspaceContent
                        .retainedVisibility(!documentVisible)
                    if let document {
                        WorkspaceDocumentView(document: document).id(document.id)
                            .retainedVisibility(documentVisible)
                    }
                }.frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        .toolbar {
            ToolbarItem(placement: .navigation) {
                Button { sidebarVisible.toggle() } label: { Label("Toggle sidebar", systemImage: sidebarOnRight ? "sidebar.right" : "sidebar.left") }
            }
            ToolbarItem(placement: .primaryAction) {
                Menu {
                    Button(sidebarOnRight ? "Move sidebar left" : "Move sidebar right") { sidebarOnRight.toggle() }
                    Button("Show changes") { storedSidebarMode = WorkspaceSidebarMode.changes.rawValue; sidebarVisible = true }
                } label: { Label("Sidebar options", systemImage: "ellipsis") }
            }
        }
        .focusedSceneValue(\.windowKeyActions, keyActions)
        .overlay(alignment: .top) {
            if showingPalette {
                ZStack(alignment: .top) {
                    // A click outside closes the palette.
                    Color.clear.contentShape(Rectangle()).onTapGesture { showingPalette = false }
                    CommandPalette(items: paletteItems, run: runPaletteItem, close: { showingPalette = false })
                        .padding(.top, DesignTokens.Size.paletteTop)
                }
            }
        }
        .onChange(of: model.choicesAlerts.message, initial: true) { _, _ in scheduleNextAlert() }
        .onChange(of: model.saveNotice) { _, _ in scheduleNextAlert() }
        // Each hop starts from an onChange, after the update that closed the last alert
        // (docs/research/2026-10-08-swiftui-alert-queue.md).
        .onChange(of: shownAlert == nil) { _, closed in if closed { scheduleNextAlert() } }
        .onChange(of: pendingAlerts.count) { _, _ in scheduleNextAlert() }
        .onChange(of: workspaceDirectory) { _, _ in closeDocument() }
        .onChange(of: model.selectedSessionID) { oldID, id in
            guard oldID != id else { return }
            NSApp.keyWindow?.makeFirstResponder(nil)
            panes.stop(keepingSession: id)
            if let target = sidebarFocus, target.chatID == id,
               let session = target.agentSessionID, let agent = target.agentID {
                panes.focus(key: AgentPaneStore.key(session: session, agent: agent.rawValue))
            } else {
                sidebarFocus = nil
            }
            documentVisible = false
        }
        .background(WindowFrameRestorer())
        .task {
            if SwarmOpenScript.opensPalette { showingPalette = true }
            if SwarmOpenScript.isActive { await runOpenScript() }
        }
        .task {
            SwarmPerformance.event("WindowReady")
            LoginShellPath.begin()
            await model.run()
        }
        .task { await model.runSidebarRuns() }
        .task { await model.runRowFields() }
        .task {
            // A Finder launch finds `swarm` only on the login shell's PATH.
            await LoginShellPath.ready()
            // Switch model and the profiles page then open on these reads instead of waiting.
            await SwarmProfileCatalog.shared.prefetch()
            for provider in ModelSwitchChoice.switchable {
                Task { _ = try? await SwarmModelCatalog.shared.models(for: provider) }
            }
            guard !SwarmOpenScript.isActive else { return }
            if let drift = await PathSwarmNotice.shared.ask(check: { await PathSwarmCheck.current(dismissed: $0) }) {
                // The setup sheet waits until this alert closes, so the two never show together.
                // Only the first window of an app run gets one (ADR 0048).
                showAlert(.pathDrift(drift))
                return
            }
            await askForSetup()
        }
        .onReceive(NotificationCenter.default.publisher(for: .showHooksSetup)) { _ in
            showingHooksSetup = true
        }
        .sheet(isPresented: $showingHooksSetup) {
            HooksSetupSheet(
                loadPlan: { try await SwarmCLIBus().setupPlan($0) },
                setUp: { digest, choice in
                    try await SwarmCLIBus().setUp(digest: digest, choice: choice)
                    // A group left unchecked is that group's "Not now".
                    decline(choice.unchecked)
                },
                notNow: { choice in
                    if let groups = choice.notNowDeclines { decline(groups) } else { setupDeclined = true }
                    showingHooksSetup = false
                },
                done: { showingHooksSetup = false },
                copy: .setup
            )
        }
        .onDisappear { panes.stopAll() }
        .onReceive(NotificationCenter.default.publisher(for: NSApplication.willTerminateNotification)) { _ in
            panes.stopAll()
        }
        .sheet(item: $createSheet, onDismiss: {
            let action = createAction
            createAction = nil
            action?()
        }) { sheet in
            createSheetView(sheet)
        }
        .sheet(isPresented: Binding(
            get: { renameTarget != nil }, set: { if !$0 { renameTarget = nil } }
        )) {
            renameWorkspaceSheet
        }
        .sheet(item: $newTaskProject) { project in
            NewTaskSheet(
                project: project,
                create: { try await model.createTask(named: $0, in: project) },
                onCreated: { startChat(in: $0) }
            )
        }
        .sheet(item: $switchTarget) { target in
            let row = target.row
            SwitchModelSheet(
                directory: row.session.cwd, currentProvider: row.provider, currentModel: target.model,
                launch: { plan, progress in try await model.switchChat(plan, from: row, onProgress: progress) }
            ) { _ in
                Task { try? await model.refresh() }
            }
        }
        .alert(
            shownAlert?.title ?? "",
            isPresented: Binding(get: { shownAlert != nil }, set: { if !$0 { closeAlert() } }),
            presenting: shownAlert,
            actions: alertActions,
            message: alertMessage
        )
    }

    private func showAlert(_ alert: WindowAlert) {
        pendingAlerts.append(alert)
    }

    /// The hop keeps the next alert out of the tick the last one closes in (see `WindowAlert`).
    private func scheduleNextAlert() {
        Task { showNextAlert() }
    }

    private func showNextAlert() {
        guard shownAlert == nil else { return }
        if !pendingAlerts.isEmpty {
            shownAlert = pendingAlerts.removeFirst()
        } else if let notice = model.saveNotice {
            model.saveNotice = nil
            shownAlert = .error(notice)
        } else if let failure = model.choicesAlerts.message {
            model.choicesAlerts.dismiss()
            shownAlert = .error(failure)
        }
    }

    private func closeAlert() {
        if case .pathDrift(let drift) = shownAlert {
            PathSwarmNotice.shared.answer(drift)
            Task { await askForSetup() }
        }
        shownAlert = nil
    }

    @ViewBuilder
    private func alertActions(_ alert: WindowAlert) -> some View {
        switch alert {
        case .pathDrift(let drift):
            Button("Copy Command") {
                NSPasteboard.general.clearContents()
                NSPasteboard.general.setString(drift.fixCommand, forType: .string)
            }
            Button("Not Now", role: .cancel) {}
        case .gitInit(let request):
            Button("Run git init") { runGitInit(request) }
            switch request.reason {
            case .importFolder(let url):
                Button("Keep as Folder", role: .cancel) { performProjectAction(url, .open) }
            case .newWorkspace:
                Button("Cancel", role: .cancel) {}
            }
        case .removeProject(let project):
            Button("Remove Project", role: .destructive) {
                do { try model.removeProject(project) }
                catch { showAlert(.error("Could not remove the project. \(error.localizedDescription)")) }
            }
            Button("Cancel", role: .cancel) {}
        case .prune(let entry, _):
            Button("Prune", role: .destructive) {
                Task {
                    do { try await model.pruneWorktree(entry) }
                    catch { showAlert(.error(error.localizedDescription)) }
                }
            }
            Button("Cancel", role: .cancel) {}
        case .error:
            Button("OK") {}
        }
    }

    private func alertMessage(_ alert: WindowAlert) -> Text {
        switch alert {
        case .pathDrift(let drift):
            Text(verbatim: "Terminal runs \(drift.pathLine) from \(drift.path). This app runs \(drift.helperLine). Agents that Swarm starts use the app's copy, but commands in Terminal and agents started elsewhere use the other one.\n\n\(drift.fixCommand)")
        case .gitInit:
            Text("Each workspace in a project is a git worktree, so a project needs git. Swarm can run git init in this folder.")
        case .removeProject(let project):
            Text("Hide \(model.navigation.projectTitle(for: project)) from the sidebar? Its folder and chats stay on disk.")
        case .prune(let entry, let paths):
            Text(verbatim: "Prune removes the worktree records for these missing folders in "
                 + model.navigation.projectTitle(for: entry.project) + ":\n\n"
                 + paths.joined(separator: "\n"))
        case .error(let message):
            Text(verbatim: message)
        }
    }

    private var workspaceContent: some View {
        ZStack {
            VStack(spacing: 0) {
                if let directory = tabsDirectory {
                    workspaceTabs(in: directory)
                    Divider()
                }
                ZStack {
                    ForEach(model.detailModels.entries) { entry in
                        if let cachedRow = model.tree.session(entry.id), cachedRow.id == entry.id {
                            let active = cachedRow.id == model.selectedSessionID
                            chatDetail(cachedRow, model: entry.model, active: active)
                                .retainedVisibility(active)
                        }
                    }
                }
            }
            .retainedVisibility(model.selectedSession != nil)
            if let id = model.selectedPendingID, let chat = model.pendingChats[id] {
                VStack(spacing: 0) {
                    if let directory = tabsDirectory {
                        workspaceTabs(in: directory)
                        Divider()
                    }
                    PendingChatView(
                        chat: chat,
                        retry: { model.retryChat(id) },
                        close: {
                            Task {
                                do { try await model.discardChat(id) }
                                catch { showAlert(.error(LaunchFailure(error).message)) }
                            }
                        }
                    )
                }
            } else if model.selectedSession == nil {
                workspaceLanding
            }
        }
        .navigationTitle(windowTitle)
    }

    @ViewBuilder private var workspaceLanding: some View {
        if let workspace = model.selectedWorkspace {
            VStack(spacing: DesignTokens.Spacing.l) {
                Text(model.navigation.title(for: workspace)).font(.title2)
                Text("This workspace has no open chats.").foregroundStyle(.secondary)
                Button("New chat") { startChat(in: workspace.id) }
                    .disabled(!workspace.workspace.canStartChat)
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        } else {
            AgentProfilesHome(
                sessionsError: model.error,
                onOpenProject: importProject,
                onCreateProject: createProject
            )
        }
    }

    private var minimumContentWidth: CGFloat {
        guard let row = model.selectedSession else { return 400 }
        return SwarmPanePolicy.hasLiveChildAgents(session: row.session, agents: model.agents) ? 680 : 400
    }

    private var workspaceDirectory: String? {
        model.selectedWorkspace?.id ?? model.selectedSession?.session.cwd
    }

    private var sidebarMode: WorkspaceSidebarMode {
        WorkspaceSidebarMode(rawValue: storedSidebarMode) ?? .workspaces
    }

    /// Asked once, on the owner's first run with swarm's hooks not set up.
    private func askForSetup() async {
        guard !setupDeclined,
              let status = try? await SwarmCLIBus().setupStatus(),
              status.needsSheet(
                  hooksDeclined: hooksSetupDeclined, trustDeclined: trustSetupDeclined
              ) else { return }
        showingHooksSetup = true
    }

    /// Sets each setup group's own decline flag. Herdr has none, because this build plans no
    /// Herdr write (ADR 0043).
    private func decline(_ groups: Set<String>) {
        if groups.contains("hooks") { hooksSetupDeclined = true }
        if groups.contains("trust") { trustSetupDeclined = true }
    }

    private func sidebarSections(showingArchive: Bool) -> [SidebarSection] {
        SidebarRows.sections(
            projects: model.tree.projects, workspaces: model.workspaces, navigation: model.navigation, search: "",
            showingArchive: showingArchive, now: Int(Date().timeIntervalSince1970),
            agentsBySession: model.tree.agentsBySession, expandedLists: expandedLists,
            runsByWorkspace: model.runsByWorkspace, workspaceFields: model.workspaceFields,
            runNoticesByWorkspace: model.runNoticesByWorkspace
        )
    }

    private func selectedSidebarID(in sections: [SidebarSection]) -> String? {
        SidebarRows.selectedID(
            in: sections.flatMap(\.rows),
            workspace: model.navigation.selectedWorkspace, chat: model.selectedSession, child: sidebarFocus
        )
    }

    /// Rows in the order the sidebar shows them, without the rows of a collapsed project.
    private var visibleSidebarRows: [SidebarRow] {
        sidebarSections(showingArchive: false).flatMap { section -> [SidebarRow] in
            if model.navigation.collapsed.contains(section.collapseID) { return [] }
            return section.rows
        }
    }

    private var sidebarActions: SidebarActions {
        func entry(_ id: String) -> WorkspaceEntry? { model.workspaces.first { $0.id == id } }
        return SidebarActions(
            selectMode: { mode in
                storedSidebarMode = mode.rawValue
                if mode == .workspaces { documentVisible = false }
            },
            select: { id in
                guard let target = SidebarRows.selection(
                    for: id, in: model.workspaces, agentsBySession: model.tree.agentsBySession
                ), let entry = entry(target.workspaceID) else { return }
                if model.navigation.archived.contains(entry.id) {
                    model.navigation.archived.remove(entry.id)
                    showingArchive = false
                }
                ownerMoves += 1
                sidebarFocus = target.agentID == nil ? nil : target
                if let chat = target.chatID {
                    let changedChat = model.selectedSessionID != chat
                    model.select(chat)
                    if !changedChat, let session = target.agentSessionID, let agent = target.agentID {
                        panes.focus(key: AgentPaneStore.key(session: session, agent: agent.rawValue))
                    }
                } else {
                    model.selectWorkspace(entry)
                }
            },
            home: {
                ownerMoves += 1
                showingArchive = false
                model.showHome()
            },
            newWorkspace: { id in
                // By id, not path: a bare clone kept as a folder shares its path with its repository.
                if let project = model.tree.projects.first(where: { SidebarSection.id(of: $0) == id }) {
                    newWorkspace(in: project)
                }
            },
            openPalette: { showingPalette = true },
            toggleArchive: { showingArchive.toggle() },
            importProject: importProject,
            importFolder: importFolder,
            createProject: createProject,
            toggleCollapsed: { path in
                model.navigation.toggleCollapsed(path)
            },
            expandList: { expandedLists.insert($0) },
            newChat: { startChat(in: $0) },
            unpinWorkspace: { model.navigation.pinned.remove($0) },
            pinWorkspace: { model.navigation.pinWorkspace($0, in: model.workspaces) },
            moveWorkspace: { model.navigation.moveWorkspace($0, onto: $1, in: model.workspaces) },
            workspaceMoveTarget: {
                model.navigation.workspaceMoveTarget($0, by: $1, in: model.workspaces)
            },
            rename: { id in
                guard let entry = entry(id) else { return }
                // The name the row shows, so Save with no edit keeps it.
                renameName = model.navigation.title(for: entry, inProject: !model.navigation.pinned.contains(entry.id))
                renameTarget = .workspace(entry)
            },
            renameChat: { id in
                if let target = SidebarRows.selection(for: id, in: model.workspaces, agentsBySession: model.tree.agentsBySession),
                   let chat = target.chatID { beginRenameChat(chat) }
            },
            renameProject: { id in
                guard let project = model.tree.projects.first(where: { SidebarSection.id(of: $0) == id }) else { return }
                renameName = model.navigation.projectTitle(for: project)
                renameTarget = .project(project)
            },
            archive: { id in entry(id).map(model.archiveWorkspace) },
            archiveChat: { id in
                if let target = SidebarRows.selection(for: id, in: model.workspaces, agentsBySession: model.tree.agentsBySession),
                   let chat = target.chatID { archiveChat(chat) }
            },
            restore: { model.navigation.archived.remove($0) },
            removeProject: { id in
                if let project = model.tree.projects.first(where: { SidebarSection.id(of: $0) == id }) {
                    showAlert(.removeProject(project))
                }
            },
            pruneWorktree: { id in
                guard let workspace = entry(id), workspace.workspace.missing,
                      case .repository(let common) = workspace.project.id else { return }
                Task {
                    do {
                        let paths = try await Git.worktrees(of: common).filter(\.isPrunable).map(\.path)
                        guard !paths.isEmpty else {
                            showAlert(.error("No missing worktrees to prune."))
                            return
                        }
                        showAlert(.prune(entry: workspace, paths: paths))
                    }
                    catch { showAlert(.error(error.localizedDescription)) }
                }
            },
            showRun: { id in
                guard let destination = SidebarRows.runDestination(
                    for: id, in: model.workspaces, runsByWorkspace: model.runsByWorkspace,
                    agentsBySession: model.tree.agentsBySession
                ), let workspace = entry(destination.workspaceID) else { return }
                ownerMoves += 1
                sidebarFocus = nil
                documentVisible = false
                model.selectWorkspace(workspace)
                runRequestWorkspace = destination.workspaceID
                runRequest = StepRunRequest(run: destination.run)
                storedSidebarMode = WorkspaceSidebarMode.runs.rawValue
                sidebarVisible = true
            }
        )
    }

    private func runChatTitles(in directory: String) -> [String: String] {
        guard let entry = model.workspaces.first(where: { $0.id == directory }) else { return [:] }
        var titles: [String: String] = [:]
        for step in (model.runsByWorkspace[directory] ?? []).flatMap(\.steps) {
            if let chat = SidebarRows.chat(for: step, in: entry, agentsBySession: model.tree.agentsBySession) {
                titles[step.path] = model.navigation.title(for: chat)
            }
        }
        return titles
    }

    private func selectRunChat(_ path: String, in directory: String) {
        guard let entry = model.workspaces.first(where: { $0.id == directory }),
              let step = (model.runsByWorkspace[directory] ?? []).flatMap(\.steps).first(where: { $0.path == path }),
              let chat = SidebarRows.chat(for: step, in: entry, agentsBySession: model.tree.agentsBySession) else { return }
        ownerMoves += 1
        sidebarFocus = nil
        documentVisible = false
        model.select(chat.id)
    }

    private var paletteItems: [PaletteItem] {
        let listed = PaletteSource.workspaces(
            model.workspaces, navigation: model.navigation, now: Int(Date().timeIntervalSince1970)
        )
        let session = model.selectedSession?.session
        return PaletteItems.build(
            sidebarViews: WorkspaceSidebarMode.allCases.map(\.rawValue),
            workspaces: listed.workspaces,
            chats: listed.chats,
            agents: session.map { session in
                SwarmPanePolicy.cells(session: session, agents: model.agents).map {
                    PaletteSource.Agent(
                        id: $0.agent.id.rawValue, name: $0.agent.id.rawValue, role: $0.agent.role,
                        status: $0.agent.status
                    )
                }
            } ?? [],
            recentActions: recentActions
        )
    }

    /// Runs a palette item the way its menu command or sidebar row would.
    private func runPaletteItem(_ item: PaletteItem) {
        showingPalette = false
        guard let colon = item.id.firstIndex(of: ":") else { return }
        let id = String(item.id[item.id.index(after: colon)...])
        switch item.id[..<colon] {
        case "action":
            guard let key = PaletteItems.actions.first(where: { "\($0)" == id }) else { return }
            recentActions[key] = Int(Date().timeIntervalSince1970)
            AppKeyTarget.current.perform(key)
        case "workspace":
            sidebarActions.select(id)
        case "chat":
            ownerMoves += 1
            model.select(SwarmSessionID(id))
        case "agent":
            if let session = model.selectedSession?.session {
                panes.focus(key: AgentPaneStore.key(session: session.id, agent: id))
            }
        default:
            break
        }
    }

    /// `SWARM_OPEN_SCRIPT=N`: selects each workspace, then each of its chats, through the model,
    /// N rounds, and prints the milliseconds until each shows content. No chat text is printed.
    private func runOpenScript() async {
        func waitForContent(_ id: SwarmSessionID?) async -> Double? {
            let start = ContinuousClock.now
            while ContinuousClock.now - start < .seconds(5), !Task.isCancelled {
                if let entry = model.detailModels.entries.first, entry.id == id, entry.model.snapshot != .loading {
                    let elapsed = ContinuousClock.now - start
                    return Double(elapsed.components.attoseconds) / 1e15 + Double(elapsed.components.seconds) * 1000
                }
                try? await Task.sleep(for: .milliseconds(2))
            }
            return nil
        }
        while model.workspaces.isEmpty {
            // The window closing cancels this task; stop rather than spin.
            do { try await Task.sleep(for: .milliseconds(100)) } catch { return }
        }
        print("open-script start: \(model.workspaces.count) workspaces")
        fflush(stdout)
        var workspaceTimes: [Double] = []
        var chatTimes: [Double] = []
        var misses = 0
        for round in 1...SwarmOpenScript.rounds where !Task.isCancelled {
            let entries = model.workspaces.filter { !model.navigation.archived.contains($0.id) && !$0.chats.isEmpty }
            for entry in entries where !Task.isCancelled {
                model.selectWorkspace(entry)
                let workspaceMs = await waitForContent(model.selectedSessionID)
                if let workspaceMs { workspaceTimes.append(workspaceMs) } else { misses += 1 }
                print("open-script workspace \(workspaceMs.map { String(format: "%.1f ms", $0) } ?? "miss")")
                for chat in entry.chats where !Task.isCancelled {
                    model.select(chat.id)
                    let chatMs = await waitForContent(chat.id)
                    if let chatMs { chatTimes.append(chatMs) } else { misses += 1 }
                    print("open-script chat \(chatMs.map { String(format: "%.1f ms", $0) } ?? "miss")")
                    fflush(stdout)
                }
            }
            print("open-script round \(round): \(entries.count) workspaces, \(chatTimes.count) chat opens so far")
            fflush(stdout)
        }
        for (name, samples) in [("workspace", workspaceTimes), ("chat", chatTimes)] {
            if let s = SwarmOpenScript.summary(samples) {
                print(String(format: "open-script %@ n=%d p50=%.1f ms p95=%.1f ms max=%.1f ms",
                             name, samples.count, s.p50, s.p95, s.max))
            }
        }
        print("open-script live detail models: \(SessionDetailModel.live.withLock { $0 })")
        print("open-script misses (over 5 s): \(misses)")
        fflush(stdout)
    }

    private var keyActions: WindowKeyActions {
        WindowKeyActions(
            newChat: model.selectedWorkspace?.workspace.canStartChat == false
                ? nil : workspaceDirectory.map { directory in { startChat(in: directory) } },
            newWorkspace: newWorkspaceInCurrentProject,
            newProject: { createSheet = .addProject },
            stepWorkspace: { delta in
                let ids = visibleSidebarRows.map(\.id)
                let listed = ids.compactMap { id in model.workspaces.first { $0.id == id } }
                let current = listed.firstIndex { $0.id == model.selectedWorkspace?.id }
                guard let index = PaneSearch.step(current: current, count: listed.count, delta: delta) else { return }
                ownerMoves += 1
                model.selectWorkspace(listed[index])
            },
            selectTab: { number in
                let tabs = tabsDirectory.map(stripTabs) ?? []
                if tabs.indices.contains(number - 1) { showTab(tabs[number - 1].id) }
            },
            stepTab: { delta in
                let tabs = tabsDirectory.map(stripTabs) ?? []
                let current = tabs.firstIndex { $0.id == selectedTabID }
                if let index = PaneSearch.step(current: current, count: tabs.count, delta: delta) {
                    showTab(tabs[index].id)
                }
            },
            toggleSidebar: { sidebarVisible.toggle() },
            moveSidebar: { sidebarOnRight.toggle() },
            sidebarView: { number in
                let mode = WorkspaceSidebarMode.allCases[number - 1]
                storedSidebarMode = mode.rawValue
                sidebarVisible = true
                if mode == .workspaces { documentVisible = false }
            },
            showChanges: {
                storedSidebarMode = WorkspaceSidebarMode.changes.rawValue
                sidebarVisible = true
            },
            search: { showingPalette = true }
        )
    }

    private func openDocument(_ value: WorkspaceDocument) {
        NSApp.keyWindow?.makeFirstResponder(nil)
        panes.clearFocus()
        document = value
        documentVisible = true
    }

    private func closeDocument() {
        documentVisible = false
        document = nil
    }

    /// ⌘N's fallback lists the projects; ⇧⌘N's sheet has only the two project buttons.
    private func createSheetView(_ sheet: CreateSheet) -> some View {
        VStack(alignment: .leading, spacing: DesignTokens.Spacing.l) {
            Text(sheet == .pickProject ? "New workspace" : "Add project").font(.title2)
            if sheet == .pickProject {
                Text("Choose a project").foregroundStyle(.secondary)
                ScrollView {
                    VStack(alignment: .leading, spacing: DesignTokens.Spacing.m) {
                        ForEach(model.tree.projects) { project in
                            Button(project.name) {
                                createAction = { newWorkspace(in: project) }
                                createSheet = nil
                            }
                        }
                    }
                }
            } else {
                Text("Create a new git repository, or import a folder from disk.").foregroundStyle(.secondary)
                Spacer()
            }
            HStack {
                Button("Create Project…") { createAction = createProject; createSheet = nil }
                    .keyboardShortcut(sheet == .addProject ? .defaultAction : nil)
                Button("Import Project…") { createAction = importProject; createSheet = nil }
                Spacer()
                Button("Cancel") { createSheet = nil }
                    .keyboardShortcut(.cancelAction)
            }
        }
        .padding(DesignTokens.Spacing.xl)
        .frame(width: DesignTokens.Size.sheet, height: DesignTokens.Size.sheetHeight)
    }

    /// The project of the selected workspace or chat, else the only project, else a picker.
    private func newWorkspaceInCurrentProject() {
        let path = model.selectedWorkspace?.id ?? tabsDirectory ?? model.selectedSession?.session.cwd
        let projects = model.tree.projects
        if let project = path.flatMap(model.tree.project(containing:)) ?? (projects.count == 1 ? projects.first : nil) {
            newWorkspace(in: project)
        } else {
            createSheet = projects.isEmpty ? .addProject : .pickProject
        }
    }

    private func newWorkspace(in project: ProjectNode) {
        if case .repository = project.id {
            newTaskProject = project
            return
        }
        Task {
            // `git init` inside a bare clone would hide its worktrees behind a nested repository.
            if await Git.isRepository(at: project.path) {
                showAlert(.error("“\(project.name)” is in a git repository that Swarm does not list as a project, such as a bare clone. Import one of its worktrees instead."))
            } else {
                showAlert(.gitInit(GitInitRequest(path: project.path, reason: .newWorkspace)))
            }
        }
    }

    private func runGitInit(_ request: GitInitRequest) {
        switch request.reason {
        case .importFolder(let url):
            performProjectAction(url, .initializeGitAndOpen)
        case .newWorkspace:
            Task {
                do { newTaskProject = try await model.initializeGit(at: request.path) }
                catch { showAlert(.error(error.localizedDescription)) }
            }
        }
    }

    private func chatDetail(
        _ row: SwarmProjectSession, model detail: SessionDetailModel, active: Bool
    ) -> some View {
        SessionDetailView(
            row: row, model: detail,
            agents: active ? model.agents : model.tree.agentsBySession[row.id] ?? [],
            launchedModel: model.launchedModels[row.id],
            launchedTrust: model.launchedTrust[row.id] ?? [],
            panes: panes, commandSource: active ? model.commandSource : nil,
            onSwitchModel: { currentModel in
                switchTarget = SwitchTarget(row: row, model: currentModel)
            },
            isCurrentSession: { model.selectedSession?.id == row.id },
            isActive: active, isVisible: active && !documentVisible,
            onUsageChanged: { usage in
                // Only a real change: each write re-renders the whole window.
                guard model.selectedSession?.id == row.id,
                      reportedUsage?.sessionID != row.id || reportedUsage?.usage != usage else { return }
                reportedUsage = (row.id, usage)
            },
            onShowUsage: { storedSidebarMode = WorkspaceSidebarMode.usage.rawValue; sidebarVisible = true }
        )
    }

    /// The workspace whose tabs show: the selected chat's, or the selected pending chat's.
    private var tabsDirectory: String? {
        if let id = model.selectedPendingID { return model.pendingChats[id]?.workspace }
        return model.selectedSession.flatMap { model.tree.workspaceChats(for: $0.id).first?.workspacePath }
    }

    /// The tabs of `directory` in strip order: its starts, then its chats. The tab keys use it too.
    private func stripTabs(in directory: String) -> [ChatTab] {
        let first = model.selectedSession?.id
            ?? model.workspaces.first { $0.id == directory }?.chats.first?.id
        let chats = first.map { model.tree.workspaceChats(for: $0) } ?? []
        return ChatTab.tabs(
            chats, pending: model.pendingChats.inWorkspace(directory), closing: model.closing,
            now: Int(Date().timeIntervalSince1970),
            navigation: model.navigation, agentsBySession: model.tree.agentsBySession,
            workspaceFields: model.workspaceFields,
            branches: Dictionary(model.workspaces.compactMap { entry in entry.workspace.branch.map { (entry.id, $0) } },
                                 uniquingKeysWith: { first, _ in first }),
            stepsByChat: tabSteps(in: directory)
        )
    }

    private func tabSteps(in directory: String) -> [SwarmSessionID: String] {
        guard let entry = model.workspaces.first(where: { $0.id == directory }) else { return [:] }
        return RowFields.stepsByChat(in: entry, runs: model.runsByWorkspace[directory] ?? [],
                                    agentsBySession: model.tree.agentsBySession)
    }

    private var selectedTabID: String {
        model.selectedPendingID.flatMap { model.pendingChats[$0]?.tabID }
            ?? model.selectedSession?.id.rawValue ?? ""
    }

    private func showTab(_ id: String) {
        // Going from one start to another leaves `selectedSessionID` nil, so its onChange does
        // not hide a file preview; hide it here.
        documentVisible = false
        ownerMoves += 1
        if let start = model.pendingChats.items.first(where: { $0.tabID == id }) {
            model.selectPending(start.id)
        } else {
            model.select(SwarmSessionID(id))
        }
    }

    private func workspaceTabs(in directory: String) -> some View {
        ChatTabsView(
            workspaceTitle: model.selectedWorkspace.map { model.navigation.title(for: $0) },
            tabs: stripTabs(in: directory),
            selectedID: selectedTabID,
            canStartChat: model.workspaces.first { $0.id == directory }?.workspace.canStartChat ?? true,
            actions: ChatTabActions(
                select: showTab,
                newChat: { startChat(in: directory) },
                close: { id in
                    Task {
                        do { try await model.close(SwarmSessionID(id)) }
                        catch { showAlert(.error(error.localizedDescription)) }
                    }
                },
                archive: { archiveChat(SwarmSessionID($0)) },
                rename: { beginRenameChat(SwarmSessionID($0)) }
            )
        )
    }

    private var renameWorkspaceSheet: some View {
        VStack(alignment: .leading, spacing: DesignTokens.Spacing.l) {
            Text(renameTarget?.heading ?? "Rename").font(.title2)
            TextField("Name", text: $renameName)
                .textFieldStyle(.roundedBorder)
            Text("Leave the name blank to use the default name.")
                .font(.callout).foregroundStyle(.secondary)
            HStack {
                Spacer()
                Button("Cancel") { renameTarget = nil }
                    .keyboardShortcut(.cancelAction)
                Button("Save") {
                    let name = renameName.trimmingCharacters(in: .whitespacesAndNewlines)
                    switch renameTarget {
                    case .workspace(let entry):
                        let shown = model.navigation.title(for: entry, inProject: !model.navigation.pinned.contains(entry.id))
                        // Saving the default title unchanged must not freeze it as a custom name.
                        let unchanged = model.navigation.names[entry.id] == nil && name == shown
                        model.navigation.names[entry.id] = name.isEmpty || unchanged ? nil : name
                    case .chat(let chat):
                        let key = ChatTitle.key(chat)
                        let unchanged = model.navigation.chatNames[key] == nil && name == ChatTitle.title(chat)
                        model.navigation.renameChat(chat, to: unchanged ? "" : name)
                    case .project(let project):
                        let unchanged = model.navigation.projectNames[project.path] == nil && name == project.name
                        model.navigation.renameProject(project, to: unchanged ? "" : name)
                    case nil: break
                    }
                    renameTarget = nil
                }
                .keyboardShortcut(.defaultAction)
            }
        }
        .padding(DesignTokens.Spacing.xl)
        .frame(width: DesignTokens.Size.narrowSheet)
    }

    private func beginRenameChat(_ id: SwarmSessionID) {
        guard let chat = model.tree.session(id) else { return }
        renameName = model.navigation.title(for: chat)
        renameTarget = .chat(chat)
    }

    private var windowTitle: String {
        model.selectedSessionID.flatMap { model.tree.windowTitle(for: $0, navigation: model.navigation) } ?? "Swarm"
    }

    private func archiveChat(_ id: SwarmSessionID) {
        Task {
            do { try await model.archive(id) }
            catch { showAlert(.error(error.localizedDescription)) }
        }
    }

    /// Every New chat entry ends here (ADR 0035).
    private func startChat(in directory: String) {
        ownerMoves += 1
        documentVisible = false
        model.newChat(in: directory)
    }

    /// A folder in a git repository, a linked worktree, or a bare hub joins its repository's
    /// project; a plain folder asks for `git init` first.
    private func importProject() {
        let panel = NSOpenPanel()
        panel.canChooseDirectories = true
        panel.canChooseFiles = false
        panel.allowsMultipleSelection = false
        panel.prompt = "Import Project"
        panel.begin { response in
            guard response == .OK, let url = panel.url else { return }
            importFolder(url)
        }
    }

    private func importFolder(_ url: URL) {
        let identity = SwarmSessionDiscovery.identity(
            for: url.resolvingSymlinksInPath().path, repositoryPathsResolver: Git.repositoryPaths
        )
        Task {
            if case .repository = identity {
                performProjectAction(url, .open)
            } else if await Git.isRepository(at: url.path) {
                // A bare clone: adding it as today is safe, `git init` in it is not.
                performProjectAction(url, .open)
            } else {
                showAlert(.gitInit(GitInitRequest(path: url.path, reason: .importFolder(url))))
            }
        }
    }

    private func createProject() {
        let panel = NSSavePanel()
        panel.title = "Create Project"
        panel.prompt = "Create Project"
        panel.nameFieldStringValue = "New Project"
        panel.canCreateDirectories = true
        panel.begin { response in
            guard response == .OK, let url = panel.url else { return }
            performProjectAction(url, .create)
        }
    }

    private func performProjectAction(_ url: URL, _ action: ProjectAction) {
        guard projectAction == nil else { return }
        projectAction = action == .create ? "Creating project…" : "Opening project…"
        let moves = ownerMoves
        Task {
            defer { projectAction = nil }
            do {
                let path = try await (action == .create
                    ? model.createProject(at: url)
                    : model.openProject(url, initializeGit: action == .initializeGitAndOpen))
                guard ownerMoves == moves else { return }
                // The kept path is the project's launch folder. The tree can still lack the
                // project when a later refresh overtook this one, so it is not read here.
                startChat(in: path)
            } catch {
                let subject = action == .create ? "Could not create the project." : "Could not import the project."
                showAlert(.error("\(subject) \(error.localizedDescription)"))
            }
        }
    }

}

private struct WorkspacePanels: View {
    let directory: String
    let mode: WorkspaceSidebarMode
    let visible: Bool
    let usage: ChatUsage?
    let hasChat: Bool
    let open: (WorkspaceDocument) -> Void
    let runRequest: StepRunRequest?
    let openedRun: () -> Void
    let chatTitles: [String: String]
    let selectRunChat: (String) -> Void
    @State private var visitedFiles = false
    @State private var visitedRuns = false
    @State private var visitedDetails = false
    @State private var detailsMode = WorkspaceSidebarMode.changes

    var body: some View {
        ZStack {
            if visitedFiles || mode == .files {
                WorkspaceFilesView(directory: directory, isActive: visible && mode == .files, open: open)
                    .retainedVisibility(mode == .files)
            }
            if visitedRuns || mode == .runs {
                StepRunsView(directory: directory, isActive: visible && mode == .runs, open: open,
                             request: runRequest, openedRun: openedRun, chatTitles: chatTitles, selectChat: selectRunChat)
                    .retainedVisibility(mode == .runs)
            }
            if visitedDetails || mode.isDetails {
                WorkspaceDetails(
                    directory: directory, usage: usage, hasChat: hasChat,
                    mode: mode.isDetails ? mode : detailsMode, isActive: visible && mode.isDetails, open: open
                )
                .retainedVisibility(mode.isDetails)
            }
        }
        .onChange(of: mode, initial: true) { _, mode in
            if mode == .files { visitedFiles = true }
            if mode == .runs { visitedRuns = true }
            if mode.isDetails { visitedDetails = true; detailsMode = mode }
        }
    }
}

private enum RenameTarget {
    case workspace(WorkspaceEntry), chat(SwarmProjectSession), project(ProjectNode)

    var heading: String {
        switch self {
        case .workspace: "Rename workspace"
        case .chat: "Rename chat"
        case .project: "Rename Project"
        }
    }
}

private enum ProjectAction { case create, open, initializeGitAndOpen }

private enum CreateSheet: Identifiable {
    case pickProject, addProject
    var id: Self { self }
}

/// Every alert of the window. On macOS, SwiftUI drops an alert asked for while a different alert is
/// up, or in the tick one closes, and leaves its state set with nothing on screen; a sheet does not
/// drop one, the alert waits for it (docs/research/2026-10-08-swiftui-alert-queue.md). So the
/// window has one `.alert`, a new alert waits in a list, and the next one shows a `Task` hop after
/// the last one closes.
private enum WindowAlert {
    case pathDrift(PathSwarmDrift)
    case gitInit(GitInitRequest)
    case removeProject(ProjectNode)
    case prune(entry: WorkspaceEntry, paths: [String])
    case error(String)

    var title: String {
        switch self {
        case .pathDrift: "Terminal runs another swarm"
        case .gitInit(let request): "“\(request.name)” is not a git repository"
        case .removeProject: "Remove Project…"
        case .prune: "Prune missing worktrees?"
        case .error: "Could not complete action"
        }
    }
}

/// The owner's yes to `git init` in a plain folder, asked before Import or a project's "+".
private struct GitInitRequest {
    enum Reason { case importFolder(URL), newWorkspace }
    let path: String
    let reason: Reason
    var name: String { URL(fileURLWithPath: path).lastPathComponent }
}

private struct NewTaskSheet: View {
    let project: ProjectNode
    let create: (String) async throws -> String
    let onCreated: (String) -> Void

    @Environment(\.dismiss) private var dismiss
    @State private var name = ""
    @State private var isCreating = false
    @State private var error: String?

    var body: some View {
        VStack(alignment: .leading, spacing: DesignTokens.Spacing.l) {
            Text("New workspace in \(project.name)").font(.title2)
            Text(project.path).foregroundStyle(.secondary)
            TextField("Workspace name", text: $name)
            Text("This workspace has its own branch and files.")
                .foregroundStyle(.secondary)
            Text("To add a chat in the current workspace, press ⌘T.")
                .font(.callout).foregroundStyle(.secondary)
            if let error { Text(verbatim: error).foregroundStyle(.red) }
            HStack {
                Spacer()
                Button("Cancel") { dismiss() }
                    .keyboardShortcut(.cancelAction)
                    .disabled(isCreating)
                Button(isCreating ? "Creating…" : "Create") {
                    guard !isCreating else { return }
                    isCreating = true
                    error = nil
                    Task {
                        defer { isCreating = false }
                        do {
                            onCreated(try await create(name))
                            dismiss()
                        } catch {
                            self.error = "Could not add the workspace. \(error.localizedDescription)"
                        }
                    }
                }
                .keyboardShortcut(.defaultAction)
                .disabled(isCreating || name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
            }
        }
        .padding(DesignTokens.Spacing.xl)
        .frame(width: DesignTokens.Size.sheet)
        .interactiveDismissDisabled(isCreating)
    }
}

private struct WindowFrameRestorer: NSViewRepresentable {
    func makeNSView(context: Context) -> NSView { WindowFrameView() }
    func updateNSView(_ view: NSView, context: Context) {}
}

private final class WindowFrameView: NSView {
    private let frameName = "SwarmSessionsWindow"

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        guard let window, window.frameAutosaveName != frameName else { return }
        window.setFrameAutosaveName(frameName)
        if !window.setFrameUsingName(frameName) {
            window.setFrame(window.screen?.visibleFrame ?? NSScreen.main?.visibleFrame ?? window.frame, display: true)
        }
        let notifications = NotificationCenter.default
        notifications.addObserver(self, selector: #selector(saveFrame), name: NSWindow.didEndLiveResizeNotification, object: window)
        notifications.addObserver(self, selector: #selector(saveFrame), name: NSWindow.didMoveNotification, object: window)
        notifications.addObserver(self, selector: #selector(saveFrame), name: NSApplication.willTerminateNotification, object: nil)
    }

    @objc private func saveFrame() {
        window?.saveFrame(usingName: frameName)
    }

    deinit {
        NotificationCenter.default.removeObserver(self)
    }
}

private struct SwitchTarget: Identifiable {
    let row: SwarmProjectSession
    let model: String?
    var id: SwarmSessionID { row.id }
}

struct SwarmApp: App {
    init() {
        SwarmPerformance.event("AppStarted")
        // Screenshot aid: SWARM_APPEARANCE=dark or light fixes this app's appearance only.
        switch ProcessInfo.processInfo.environment["SWARM_APPEARANCE"] {
        case "dark": NSApplication.shared.appearance = NSAppearance(named: .darkAqua)
        case "light": NSApplication.shared.appearance = NSAppearance(named: .aqua)
        default: break
        }
    }

    var body: some Scene {
        // A fixed id: without one, SwiftUI names the scene by a type address that changes with
        // each build, so a new build restores no window and opens none.
        WindowGroup(id: "sessions") {
            if SwarmPaneStress.count > 0 { PaneStressWindow() } else { SessionsWindow() }
        }
            .commands {
                SetupCommands()
                DebugCommands()
                AppKeyCommands()
            }
        // One window, not a sheet, because the list grows with each trusted folder. It opens only
        // from the menu, never at launch and never restored from the last run, because each open
        // reads every managed file.
        Window("Managed Changes", id: "managed") { ManagedChangesPage() }
            .defaultLaunchBehavior(.suppressed)
            .restorationBehavior(.disabled)
    }
}

private struct SetupCommands: Commands {
    @Environment(\.openWindow) private var openWindow

    var body: some Commands {
        CommandGroup(after: .appSettings) {
            Button("Set Up Swarm…") {
                NotificationCenter.default.post(name: .showHooksSetup, object: nil)
            }
            Button("Managed Changes…") { openWindow(id: "managed") }
        }
    }
}

private struct DebugCommands: Commands {
    @AppStorage("showRawData") private var showRawData = false
    @AppStorage("performanceLogging") private var performanceLogging = false

    var body: some Commands {
        CommandMenu("Debug") {
            Toggle("Show Raw Data", isOn: $showRawData)
                .keyboardShortcut("r", modifiers: [.command, .option])
            Toggle("Performance Logging", isOn: $performanceLogging)
        }
    }
}

@main
@MainActor
enum SwarmExecutable {
    static func main() async {
        let arguments = Array(CommandLine.arguments.dropFirst())
        if arguments == ["--print-tree"] {
            let model = SessionsTreeModel()
            do {
                try await model.refresh()
                let output = model.tree.text()
                if !output.isEmpty { print(output) }
            } catch {
                fputs("\(error)\n", stderr)
                exit(1)
            }
        } else if arguments.count == 2, arguments[0] == "--print-transcript" {
            await printTranscript(prefix: arguments[1], raw: false)
        } else if arguments.count == 3, arguments[0] == "--print-transcript",
                  arguments[2] == "--raw" {
            await printTranscript(prefix: arguments[1], raw: true)
        } else if arguments.count == 3, arguments[0] == "--print-child" {
            await printChild(prefix: arguments[1], agentID: SwarmAgentID(arguments[2]))
        } else if arguments.count == 4, arguments[0] == "--launch-check" {
            await launchCheck(directory: arguments[1], provider: arguments[2], model: arguments[3])
        } else {
            SwarmApp.main()
        }
    }

    private static func launchCheck(directory: String, provider: String, model: String) async {
        do {
            await LoginShellPath.ready()
            guard let plan = SwarmChatLaunchPlan(
                directory: directory, provider: provider, model: model, account: .auto
            ) else {
                throw SwarmProfileError.failed("Provider, model, or directory is invalid")
            }
            let id = try await SwarmChatLauncher.start(plan, bus: SwarmCLIBus())
            let agent = try await SwarmChatLauncher.waitForChairPane(in: id, bus: SwarmCLIBus())
            print("session: \(id.rawValue)")
            print("agent: \(agent.id.rawValue)")
            print("pane: \(agent.pane ?? "")")
        } catch {
            fputs("\((error as? SwarmProfileError)?.message ?? String(describing: error))\n", stderr)
            exit(1)
        }
    }

    private static func printTranscript(prefix: String, raw: Bool) async {
        do {
            let session = try await matchingSession(prefix: prefix)
            let agents = try await SwarmCLIBus().agents(in: session)
            let provider = agents.first { $0.id == SwarmPanePolicy.chair }?.provider
            let snapshot = await SwarmChairTranscript().poll(
                session: session, chairProvider: provider
            )
            if raw, case .rows(_, let entries) = snapshot {
                print(TranscriptDebugData.printText(
                    session: session, agents: agents, entries: entries
                ))
            } else {
                print(snapshot.printText)
            }
            if case .unavailable = snapshot { exit(1) }
        } catch {
            fputs("\(error)\n", stderr)
            exit(1)
        }
    }

    /// A child column's content, headless: its transcript rows, then the question it shows.
    private static func printChild(prefix: String, agentID: SwarmAgentID) async {
        do {
            let session = try await matchingSession(prefix: prefix)
            guard let agent = try await SwarmCLIBus().agents(in: session).first(where: { $0.id == agentID }) else {
                throw SwarmProfileError.failed("agent not found")
            }
            let snapshot = await SwarmChairTranscript().poll(childLog: agent.log, provider: agent.provider)
            print(snapshot.printText)
            if let prompt = agent.prompt {
                print("prompt \(prompt.id): \(prompt.question.replacingOccurrences(of: "\n", with: " / "))")
                for (index, choice) in prompt.choices.enumerated() { print("choice \(index): \(choice)") }
            }
            if case .unavailable = snapshot { exit(1) }
        } catch {
            fputs("\(error)\n", stderr)
            exit(1)
        }
    }

    private static func matchingSession(prefix: String) async throws -> SwarmSession {
        let sessions = try await SwarmCLIBus().sessions().filter { $0.id.rawValue.hasPrefix(prefix) }
        guard sessions.count == 1, let session = sessions.first else {
            throw SwarmProfileError.failed("session prefix does not name one session")
        }
        return session
    }
}
