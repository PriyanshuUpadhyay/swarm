import Foundation
import Observation
import SwiftUI
import SwarmCore

@MainActor @Observable
final class SessionsTreeModel {
    private let bus = SwarmCLIBus()
    private let discovery = SwarmSessionDiscovery()
    private let drafts = ComposerDraftStore()
    private let projects = SwarmProjectStore()
    private let navigationStore = WorkspaceNavigationStore()
    var navigation = WorkspaceNavigation() {
        didSet { if navigation != oldValue { navigationStore.save(navigation) } }
    }

    init() { navigation = navigationStore.load() }

    /// Rebuilt when the tree changes, not on every read.
    private(set) var workspaces: [WorkspaceEntry] = []
    var selectedWorkspace: WorkspaceEntry? {
        workspaces.first { $0.id == navigation.selectedWorkspace }
    }

    func selectWorkspace(_ entry: WorkspaceEntry) {
        navigation.select(entry)
        select(navigation.selectedChat(in: entry)?.id)
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
        didSet { workspaces = WorkspaceEntry.list(in: tree) }
    }
    let detailModels = SessionDetailStore()
    var selectedSessionID: SwarmSessionID? {
        didSet {
            if selectedSessionID != oldValue { detailModels.activate(selectedSessionID) }
        }
    }
    private var pendingID: SwarmSessionID?
    var agents: [SwarmAgent] = []
    var commandSource: ComposerCommandSource?
    private var commandSourceKey: String?
    var error: String?
    private(set) var closing: Set<SwarmSessionID> = []

    var selectedSession: SwarmProjectSession? { selectedSessionID.flatMap(tree.session) }

    func select(_ id: SwarmSessionID?) {
        if id != nil { SwarmPerformance.event("ChatSelected") }
        selectionRevision += 1
        pendingID = nil
        selectedSessionID = id
        if let id, let entry = workspaces.first(where: {
            $0.chats.contains { $0.sessions.contains { $0.id == id } }
        }) {
            navigation.select(entry, chat: id)
        }
        agents = id.flatMap { tree.agentsBySession[$0] } ?? []
        commandSource = nil
        commandSourceKey = nil
    }

    func startChat(_ plan: SwarmChatLaunchPlan) async throws -> SwarmSessionID {
        try await SwarmChatLauncher.start(plan, bus: bus) { id in
            await MainActor.run {
                self.navigation.selectedWorkspace = plan.directory
                self.navigation.selectedChats[plan.directory] = id.rawValue
                self.navigation.archived.remove(plan.directory)
                self.selectionRevision += 1
                self.pendingID = id
                self.selectedSessionID = id
                self.agents = []
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
        drafts.prune(keeping: Set(sessions.map { $0.id.rawValue }))
        do {
            let treeTiming = SwarmPerformance.begin("WorkspaceTree")
            defer { treeTiming.end(count: sessions.count) }
            let loaded = try await discovery.tree(sessions: sessions, projectPaths: projects.paths(), bus: bus)
            guard revision == refreshRevision else { return }
            sourceTree = loaded
            archives.reconcile(loaded)
            tree = archives.applying(to: loaded)
        }
        if pendingID == nil, let entry = selectedWorkspace {
            selectedSessionID = navigation.selectedChat(in: entry)?.id
        }
        if let selectedSessionID, let row = tree.session(selectedSessionID) {
            pendingID = nil
            self.selectedSessionID = row.id
            if let entry = workspaces.first(where: { $0.chats.contains { $0.id == row.id } }) {
                navigation.select(entry, chat: row.id)
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
        }
        error = nil
    }

    func openProject(_ url: URL) async throws -> SwarmPathIdentity {
        let timing = SwarmPerformance.begin("ProjectOpen")
        defer { timing.end() }
        let path = try await projects.add(url)
        try await refresh()
        return await Task.detached {
            SwarmSessionDiscovery.identity(for: path, repositoryPathsResolver: Git.repositoryPaths)
        }.value
    }

    func createProject(at url: URL) async throws -> SwarmPathIdentity {
        let timing = SwarmPerformance.begin("ProjectCreate")
        defer { timing.end() }
        let path = try await projects.create(at: url)
        try await refresh()
        return await Task.detached {
            SwarmSessionDiscovery.identity(for: path, repositoryPathsResolver: Git.repositoryPaths)
        }.value
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
        try await projects.add(URL(fileURLWithPath: path))
        navigation.names[path] = name.trimmingCharacters(in: .whitespacesAndNewlines)
        navigation.selectedWorkspace = path
        select(nil)
        do { try await refresh() }
        catch { self.error = String(describing: error) }
        return path
    }

    func archive(_ id: SwarmSessionID) async throws {
        let ids = archives.begin(id, in: tree)
        guard !ids.isEmpty else { return }
        let previous = selectedSessionID
        let workspace = navigation.selectedWorkspace
        let next = ChatArchives.selection(afterArchiving: id, selected: previous, in: tree)
        refreshRevision += 1
        tree = archives.applying(to: sourceTree)
        if next != previous { select(next) }
        let revision = selectionRevision
        SwarmPerformance.event("ChatArchiveApplied")
        do {
            try await bus.archive(ids)
            archives.finish(id, succeeded: true)
            refreshRevision += 1
        } catch {
            archives.finish(id, succeeded: false)
            refreshRevision += 1
            tree = archives.applying(to: sourceTree)
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
        let homes = Task.detached { await SwarmChairTranscript().prefetchHomes() }
        defer { homes.cancel() }
        var first = true
        while !Task.isCancelled {
            let timing = SwarmPerformance.begin(first ? "InitialRefresh" : "RefreshTick")
            do { try await refresh() }
            catch { self.error = String(describing: error) }
            timing.end()
            await SwarmChairTranscript().prefetchHomes()
            first = false
            try? await Task.sleep(for: .seconds(2))
        }
    }


}

private struct SessionsWindow: View {
    @State private var model = SessionsTreeModel()
    @State private var panes = AgentPaneStore()
    @State private var newChatDirectory: String?
    @State private var newTaskProject: ProjectNode?
    @State private var pendingTaskChatDirectory: String?
    @State private var switchTarget: SwitchTarget?
    @State private var selectedProjectID: SwarmPathIdentity?
    @State private var actionError: String?
    @State private var projectAction: String?
    @State private var showingPalette = false
    /// When each palette action last ran, in this window only.
    @State private var recentActions: [AppKey: Int] = [:]
    @State private var showingArchive = false
    @State private var showingCreate = false
    @State private var createAction: (() -> Void)?
    @State private var renameTarget: WorkspaceEntry?
    @State private var workspaceName = ""
    @AppStorage("workspaceSidebarVisible") private var sidebarVisible = true
    @AppStorage("workspaceSidebarOnRight") private var sidebarOnRight = false
    @AppStorage("workspaceSidebarMode") private var storedSidebarMode = WorkspaceSidebarMode.workspaces.rawValue
    @AppStorage("workspaceSidebarWidth") private var sidebarWidth = 280.0
    @State private var document: WorkspaceDocument?
    @State private var documentVisible = false
    @State private var reportedUsage: (sessionID: SwarmSessionID, usage: ChatUsage)?

    var body: some View {
        MovableSidebar(visible: sidebarVisible, onRight: sidebarOnRight, minimumContentWidth: minimumContentWidth, width: $sidebarWidth) {
            SidebarView(
                mode: sidebarMode,
                sections: sidebarSections(showingArchive: showingArchive),
                selectedID: model.navigation.selectedWorkspace,
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
                            hasChat: model.selectedSession != nil, open: openDocument
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
        .onChange(of: workspaceDirectory) { _, _ in closeDocument() }
        .onChange(of: model.selectedSessionID) { oldID, id in
            guard oldID != id else { return }
            NSApp.keyWindow?.makeFirstResponder(nil)
            panes.stop(keepingSession: id)
            documentVisible = false
            if id != nil { selectedProjectID = nil }
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
        .onDisappear { panes.stopAll() }
        .onReceive(NotificationCenter.default.publisher(for: NSApplication.willTerminateNotification)) { _ in
            panes.stopAll()
        }
        .sheet(isPresented: $showingCreate, onDismiss: {
            let action = createAction
            createAction = nil
            action?()
        }) {
            createWorkspacePicker
        }
        .sheet(isPresented: Binding(
            get: { renameTarget != nil }, set: { if !$0 { renameTarget = nil } }
        )) {
            renameWorkspaceSheet
        }
        .sheet(item: Binding(
            get: { newChatDirectory.map(LaunchTarget.init) },
            set: { newChatDirectory = $0?.directory }
        )) { target in
            NewChatSheet(directory: target.directory, launch: { plan, _ in try await model.startChat(plan) }) { _ in
                Task { try? await model.refresh() }
            }
        }
        .sheet(item: $newTaskProject, onDismiss: {
            if let path = pendingTaskChatDirectory {
                pendingTaskChatDirectory = nil
                newChatDirectory = path
            }
        }) { project in
            NewTaskSheet(
                project: project,
                create: { try await model.createTask(named: $0, in: project) },
                onCreated: { pendingTaskChatDirectory = $0 }
            )
        }
        .sheet(item: $switchTarget) { target in
            let row = target.row
            NewChatSheet(
                directory: row.session.cwd, isSwitch: true, initialProvider: row.provider,
                initialModel: target.model,
                launch: { plan, progress in try await model.switchChat(plan, from: row, onProgress: progress) }
            ) { _ in
                Task { try? await model.refresh() }
            }
        }
        .alert("Could not complete action", isPresented: Binding(
            get: { actionError != nil },
            set: { if !$0 { actionError = nil } }
        )) {
            Button("OK") { actionError = nil }
        } message: {
            Text(actionError ?? "")
        }
    }

    private var workspaceContent: some View {
        ZStack {
            VStack(spacing: 0) {
                if let row = model.selectedSession {
                    workspaceTabs(for: row)
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
            if model.selectedSession == nil { workspaceLanding }
        }
        .navigationTitle(model.selectedSession.flatMap { model.tree.windowTitle(for: $0.id) } ?? "Swarm")
    }

    @ViewBuilder private var workspaceLanding: some View {
        if let workspace = model.selectedWorkspace {
            VStack(spacing: DesignTokens.Spacing.l) {
                Text(model.navigation.title(for: workspace)).font(.title2)
                Text("This workspace has no open chats.").foregroundStyle(.secondary)
                Button("New chat") { newChatDirectory = workspace.id }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        } else if let project = model.tree.projects.first(where: { $0.id == selectedProjectID }) {
            ProjectHome(
                project: project,
                onNewChat: { newChatDirectory = $0 },
                onNewTask: { newTaskProject = project }
            )
        } else {
            AgentProfilesHome(
                sessionsError: model.error,
                onOpenProject: openExistingProject,
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

    private func sidebarSections(showingArchive: Bool) -> [SidebarSection] {
        SidebarRows.sections(
            workspaces: model.workspaces, navigation: model.navigation, search: "",
            showingArchive: showingArchive, now: Int(Date().timeIntervalSince1970)
        )
    }

    private var sidebarActions: SidebarActions {
        func entry(_ id: String) -> WorkspaceEntry? { model.workspaces.first { $0.id == id } }
        return SidebarActions(
            selectMode: { mode in
                storedSidebarMode = mode.rawValue
                if mode == .workspaces { documentVisible = false }
            },
            select: { id in
                guard let entry = entry(id) else { return }
                if model.navigation.archived.contains(id) {
                    model.navigation.archived.remove(id)
                    showingArchive = false
                }
                selectedProjectID = nil
                model.selectWorkspace(entry)
            },
            home: {
                selectedProjectID = nil
                showingArchive = false
                model.showHome()
            },
            create: { showingCreate = true },
            openPalette: { showingPalette = true },
            toggleArchive: { showingArchive.toggle() },
            openProject: openExistingProject,
            createProject: createProject,
            newChat: { newChatDirectory = $0 },
            togglePin: { id in
                if model.navigation.pinned.contains(id) { model.navigation.pinned.remove(id) }
                else { model.navigation.pinned.insert(id) }
            },
            rename: { id in
                guard let entry = entry(id) else { return }
                workspaceName = model.navigation.title(for: entry)
                renameTarget = entry
            },
            archive: { id in entry(id).map(model.archiveWorkspace) },
            restore: { model.navigation.archived.remove($0) }
        )
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
            newChat: workspaceDirectory.map { directory in { newChatDirectory = directory } },
            newWorkspace: { showingCreate = true },
            stepWorkspace: { delta in
                let ids = sidebarSections(showingArchive: false).flatMap(\.rows).map(\.id)
                let listed = ids.compactMap { id in model.workspaces.first { $0.id == id } }
                let current = listed.firstIndex { $0.id == model.selectedWorkspace?.id }
                guard let index = PaneSearch.step(current: current, count: listed.count, delta: delta) else { return }
                selectedProjectID = nil
                model.selectWorkspace(listed[index])
            },
            selectTab: { number in
                guard let row = model.selectedSession else { return }
                let chats = model.tree.workspaceChats(for: row.id)
                if chats.indices.contains(number - 1) { model.select(chats[number - 1].id) }
            },
            stepTab: { delta in
                guard let row = model.selectedSession else { return }
                let chats = model.tree.workspaceChats(for: row.id)
                let current = chats.firstIndex { $0.id == row.id }
                if let index = PaneSearch.step(current: current, count: chats.count, delta: delta) {
                    model.select(chats[index].id)
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

    private var createWorkspacePicker: some View {
        VStack(alignment: .leading, spacing: DesignTokens.Spacing.l) {
            Text("Create workspace").font(.title2)
            Text("Choose a project").foregroundStyle(.secondary)
            ScrollView {
                VStack(alignment: .leading, spacing: DesignTokens.Spacing.m) {
                    ForEach(model.tree.projects) { project in
                        Button(project.name) {
                            createAction = {
                                if case .repository = project.id {
                                    newTaskProject = project
                                } else {
                                    if let entry = model.workspaces.first(where: { $0.project.id == project.id }) {
                                        model.navigation.archived.remove(entry.id)
                                        model.selectWorkspace(entry)
                                    }
                                    newChatDirectory = project.launchDirectory
                                }
                            }
                            showingCreate = false
                        }
                    }
                }
            }
            HStack {
                Button("Open Project…") { createAction = openExistingProject; showingCreate = false }
                Button("Create Project…") { createAction = createProject; showingCreate = false }
                Spacer()
                Button("Cancel") { showingCreate = false }
            }
        }
        .padding(DesignTokens.Spacing.xl)
        .frame(width: DesignTokens.Size.sheet, height: DesignTokens.Size.sheetHeight)
    }

    private func chatDetail(
        _ row: SwarmProjectSession, model detail: SessionDetailModel, active: Bool
    ) -> some View {
        SessionDetailView(
            row: row, model: detail,
            agents: active ? model.agents : model.tree.agentsBySession[row.id] ?? [],
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

    private func workspaceTabs(for selected: SwarmProjectSession) -> some View {
        let chats = model.tree.workspaceChats(for: selected.id)
        return ChatTabsView(
            workspaceTitle: model.selectedWorkspace.map { model.navigation.title(for: $0) },
            tabs: ChatTab.tabs(chats, closing: model.closing, now: Int(Date().timeIntervalSince1970)),
            selectedID: selected.id.rawValue,
            actions: ChatTabActions(
                select: { model.select(SwarmSessionID($0)) },
                newChat: { if let path = chats.first?.workspacePath { newChatDirectory = path } },
                close: { id in
                    Task {
                        do { try await model.close(SwarmSessionID(id)) }
                        catch { actionError = String(describing: error) }
                    }
                },
                archive: { archiveChat(SwarmSessionID($0)) }
            )
        )
    }

    private var renameWorkspaceSheet: some View {
        VStack(alignment: .leading, spacing: DesignTokens.Spacing.l) {
            Text("Rename workspace").font(.title2)
            TextField("Name", text: $workspaceName)
                .textFieldStyle(.roundedBorder)
            Text("Leave the name blank to use the project and folder name.")
                .font(.callout).foregroundStyle(.secondary)
            HStack {
                Spacer()
                Button("Cancel") { renameTarget = nil }
                    .keyboardShortcut(.cancelAction)
                Button("Save") {
                    if let entry = renameTarget {
                        let name = workspaceName.trimmingCharacters(in: .whitespacesAndNewlines)
                        model.navigation.names[entry.id] = name.isEmpty ? nil : name
                    }
                    renameTarget = nil
                }
                .keyboardShortcut(.defaultAction)
            }
        }
        .padding(DesignTokens.Spacing.xl)
        .frame(width: DesignTokens.Size.narrowSheet)
    }

    private func archiveChat(_ id: SwarmSessionID) {
        Task {
            do { try await model.archive(id) }
            catch { actionError = String(describing: error) }
        }
    }

    private func openExistingProject() {
        let panel = NSOpenPanel()
        panel.canChooseDirectories = true
        panel.canChooseFiles = false
        panel.allowsMultipleSelection = false
        panel.prompt = "Open Project"
        panel.begin { response in
            guard response == .OK, let url = panel.url else { return }
            performProjectAction(url, create: false)
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
            performProjectAction(url, create: true)
        }
    }

    private func performProjectAction(_ url: URL, create: Bool) {
        guard projectAction == nil else { return }
        projectAction = create ? "Creating project…" : "Opening project…"
        let selection = model.selectionRevision
        Task {
            defer { projectAction = nil }
            do {
                let id = try await (create ? model.createProject(at: url) : model.openProject(url))
                guard model.selectionRevision == selection else { return }
                model.showHome()
                selectedProjectID = id
            } catch { actionError = error.localizedDescription }
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
    @State private var visitedFiles = false
    @State private var visitedDetails = false
    @State private var detailsMode = WorkspaceSidebarMode.changes

    var body: some View {
        ZStack {
            if visitedFiles || mode == .files {
                WorkspaceFilesView(directory: directory, isActive: visible && mode == .files, open: open)
                    .retainedVisibility(mode == .files)
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
            if mode.isDetails { visitedDetails = true; detailsMode = mode }
        }
    }
}

private struct ProjectHome: View {
    let project: ProjectNode
    let onNewChat: (String) -> Void
    let onNewTask: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: DesignTokens.Spacing.l) {
            Text(project.name).font(.largeTitle.bold())
            Text(project.launchDirectory).foregroundStyle(.secondary).textSelection(.enabled)
            HStack {
                Button("New chat") { onNewChat(project.launchDirectory) }
                if case .repository = project.id {
                    Button("New workspace…", action: onNewTask)
                }
            }
            if case .repository = project.id {
                Text("Worktrees").font(.headline)
                ScrollView {
                    LazyVStack(spacing: DesignTokens.Spacing.m) {
                        ForEach(project.workspaces) { workspace in
                            HStack {
                                VStack(alignment: .leading) {
                                    Text(workspace.name)
                                    Text(workspace.path).font(.caption).foregroundStyle(.secondary)
                                }
                                Spacer()
                                Button("New chat") { onNewChat(workspace.path) }
                            }
                        }
                    }
                }
            }
        }
        .padding(DesignTokens.Spacing.xl)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
    }
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
            Text("Create workspace").font(.title2)
            Text(project.path).foregroundStyle(.secondary)
            TextField("Workspace name", text: $name)
            Text("This workspace will have its own branch and files.")
                .foregroundStyle(.secondary)
            if let error { Text(verbatim: error).foregroundStyle(.red) }
            HStack {
                Spacer()
                Button("Cancel") { dismiss() }
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
                            self.error = (error as? LocalizedError)?.errorDescription
                                ?? String(describing: error)
                        }
                    }
                }
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

private struct LaunchTarget: Identifiable {
    let directory: String
    var id: String { directory }
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
        WindowGroup {
            if SwarmPaneStress.count > 0 { PaneStressWindow() } else { SessionsWindow() }
        }
            .commands {
                DebugCommands()
                AppKeyCommands()
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
            let id = try await SessionsTreeModel().startChat(plan)
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
