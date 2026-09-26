import Foundation
import Observation
import SwiftUI
import SwarmCore

struct PaneFindActions {
    var terminalFocused: Bool
    var open: () -> Void
    var next: () -> Void
    var previous: () -> Void
}

private struct PaneFindActionsKey: FocusedValueKey {
    typealias Value = PaneFindActions
}

extension FocusedValues {
    var paneFindActions: PaneFindActions? {
        get { self[PaneFindActionsKey.self] }
        set { self[PaneFindActionsKey.self] = newValue }
    }
}

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

    var workspaces: [WorkspaceEntry] { WorkspaceEntry.list(in: tree) }
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

    var tree = SessionsTree(projects: [])
    var selectedSessionID: SwarmSessionID?
    private var pendingID: SwarmSessionID?
    var agents: [SwarmAgent] = []
    var commandSource: ComposerCommandSource?
    private var commandSourceKey: String?
    var error: String?

    var selectedSession: SwarmProjectSession? { selectedSessionID.flatMap(tree.session) }

    func select(_ id: SwarmSessionID?) {
        if id != nil { SwarmPerformance.event("ChatSelected") }
        pendingID = nil
        selectedSessionID = id
        if let id, let entry = workspaces.first(where: {
            $0.chats.contains { $0.sessions.contains { $0.id == id } }
        }) {
            navigation.select(entry, chat: id)
        }
        agents = []
        commandSource = nil
        commandSourceKey = nil
    }

    func startChat(_ plan: SwarmChatLaunchPlan) async throws -> SwarmSessionID {
        try await SwarmChatLauncher.start(plan, bus: bus) { id in
            await MainActor.run {
                self.navigation.selectedWorkspace = plan.directory
                self.navigation.selectedChats[plan.directory] = id.rawValue
                self.navigation.archived.remove(plan.directory)
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
        pendingID = id
        selectedSessionID = id
        if let path = navigation.selectedWorkspace { navigation.selectedChats[path] = id.rawValue }
        agents = []
        return id
    }

    func refresh() async throws {
        let timing = SwarmPerformance.begin("UIRefresh")
        defer { timing.end(count: tree.projects.count) }
        let sessions: [SwarmSession]
        do {
            let listTiming = SwarmPerformance.begin("SessionList")
            defer { listTiming.end() }
            sessions = try await bus.sessions()
        }
        drafts.prune(keeping: Set(sessions.map { $0.id.rawValue }))
        do {
            let treeTiming = SwarmPerformance.begin("WorkspaceTree")
            defer { treeTiming.end(count: sessions.count) }
            tree = try await discovery.tree(sessions: sessions, projectPaths: projects.paths(), bus: bus)
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
                guard self.selectedSessionID == row.id else { return }
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
                if self.selectedSessionID == row.id {
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
        let path = try projects.add(url)
        try await refresh()
        return SwarmSessionDiscovery.identity(for: path, repositoryPathsResolver: Git.repositoryPaths)
    }

    func createProject(at url: URL) async throws -> SwarmPathIdentity {
        let timing = SwarmPerformance.begin("ProjectCreate")
        defer { timing.end() }
        let path = try projects.create(at: url)
        try await refresh()
        return SwarmSessionDiscovery.identity(for: path, repositoryPathsResolver: Git.repositoryPaths)
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
        try projects.add(URL(fileURLWithPath: path))
        navigation.names[path] = name.trimmingCharacters(in: .whitespacesAndNewlines)
        navigation.selectedWorkspace = path
        selectedSessionID = nil
        do { try await refresh() }
        catch { self.error = String(describing: error) }
        return path
    }

    func archive(_ id: SwarmSessionID) async throws {
        let ids = tree.archiveIDs(for: id)
        guard !ids.isEmpty else { return }
        try await bus.archive(ids)
        clearSelection(if: id)
        try await refresh()
    }

    func close(_ id: SwarmSessionID) async throws {
        guard let session = tree.session(id)?.session else { return }
        try await SwarmSessionCloser.close(session, bus: bus)
        clearSelection(if: id)
        try await refresh()
    }

    func run() async {
        var first = true
        while !Task.isCancelled {
            let timing = SwarmPerformance.begin(first ? "InitialRefresh" : "RefreshTick")
            do { try await refresh() }
            catch { self.error = String(describing: error) }
            timing.end()
            first = false
            try? await Task.sleep(for: .seconds(2))
        }
    }

    private func clearSelection(if id: SwarmSessionID) {
        guard selectedSessionID == id else { return }
        selectedSessionID = nil
        agents = []
    }
}

private struct SessionsWindow: View {
    @State private var model = SessionsTreeModel()
    @State private var panes = AgentPaneStore()
    @State private var newChatDirectory: String?
    @State private var newTaskProject: ProjectNode?
    @State private var pendingTaskChatDirectory: String?
    @State private var switchInitialModel: String?
    @State private var switchChatFrom: SwarmProjectSession?
    @State private var selectedProjectID: SwarmPathIdentity?
    @State private var actionError: String?
    @State private var search = ""
    @State private var searching = false
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
    @FocusState private var searchFocused: Bool

    var body: some View {
        MovableSidebar(visible: sidebarVisible, onRight: sidebarOnRight, minimumContentWidth: minimumContentWidth, width: $sidebarWidth) {
            VStack(spacing: 0) {
                sidebarModes
                Divider()
                if sidebarMode == .workspaces {
                    workspaceSidebar
                } else if let directory = workspaceDirectory {
                    Button {
                        storedSidebarMode = WorkspaceSidebarMode.workspaces.rawValue
                        documentVisible = false
                    } label: {
                        VStack(alignment: .leading, spacing: 3) {
                            Text(model.selectedWorkspace.map { model.navigation.title(for: $0) } ?? URL(fileURLWithPath: directory).lastPathComponent)
                                .font(.subheadline.weight(.semibold))
                            Text(verbatim: directory).font(.caption).foregroundStyle(.secondary)
                        }
                        .lineLimit(1).truncationMode(.middle)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .padding(12)
                    }
                    .buttonStyle(.plain).help("Choose a workspace")
                    Divider()
                    if sidebarMode == .files {
                        WorkspaceFilesView(directory: directory, open: openDocument).id(directory)
                    } else {
                        WorkspaceDetails(
                            directory: directory,
                            usage: reportedUsage?.sessionID == model.selectedSession?.id ? reportedUsage?.usage : nil,
                            hasChat: model.selectedSession != nil, mode: sidebarMode, open: openDocument
                        ).id(directory)
                    }
                } else {
                    ContentUnavailableView("Select a workspace", systemImage: "folder", description: Text("Choose a workspace to see its files and details."))
                    Button("Show workspaces") { storedSidebarMode = WorkspaceSidebarMode.workspaces.rawValue }.padding()
                }
            }
            .background(Color(nsColor: .windowBackgroundColor))
        } content: {
            VStack(spacing: 0) {
                if let document {
                    HStack(spacing: 16) {
                        Button("Chat") { documentVisible = false }
                            .foregroundStyle(documentVisible ? Color.secondary : Color.primary)
                        Button(document.title) { documentVisible = true }
                            .lineLimit(1).truncationMode(.middle)
                            .foregroundStyle(documentVisible ? Color.primary : Color.secondary)
                        Button { closeDocument() } label: { Image(systemName: "xmark") }
                            .accessibilityLabel("Close file preview")
                        Spacer()
                    }.buttonStyle(.plain).padding(12)
                    Divider()
                }
                ZStack {
                    // Keep the chat's identity and draft while a file is visible or the sidebar moves.
                    workspaceContent
                        .opacity(documentVisible ? 0 : 1)
                        .allowsHitTesting(!documentVisible)
                        .disabled(documentVisible)
                        .accessibilityHidden(documentVisible)
                    if let document, documentVisible {
                        WorkspaceDocumentView(document: document).id(document.id)
                    }
                }.frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        .toolbar {
            ToolbarItem(placement: .navigation) {
                Button { sidebarVisible.toggle() } label: { Label("Toggle sidebar", systemImage: sidebarOnRight ? "sidebar.right" : "sidebar.left") }
                    .keyboardShortcut("b", modifiers: .command)
            }
            ToolbarItem(placement: .primaryAction) {
                Menu {
                    Button(sidebarOnRight ? "Move sidebar left" : "Move sidebar right") { sidebarOnRight.toggle() }
                    Button("Show changes") { storedSidebarMode = WorkspaceSidebarMode.changes.rawValue; sidebarVisible = true }
                        .keyboardShortcut("i", modifiers: [.command, .option])
                } label: { Label("Sidebar options", systemImage: "ellipsis") }
            }
        }
        .onChange(of: workspaceDirectory) { _, _ in closeDocument() }
        .onChange(of: model.selectedSessionID) { oldID, id in
            guard oldID != id else { return }
            NSApp.keyWindow?.makeFirstResponder(nil)
            panes.clearFocus()
            documentVisible = false
            if id != nil { selectedProjectID = nil }
        }
        .background(WindowFrameRestorer())
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
        .sheet(item: $switchChatFrom) { row in
            NewChatSheet(
                directory: row.session.cwd, isSwitch: true, initialProvider: row.provider,
                initialModel: switchInitialModel,
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

    @ViewBuilder private var workspaceContent: some View {
        if let row = model.selectedSession {
            VStack(spacing: 0) {
                workspaceTabs(for: row)
                Divider()
                SessionDetailView(
                    row: row,
                    title: model.selectedSessionID.flatMap(model.tree.windowTitle) ?? row.title,
                    agents: model.agents, panes: panes,
                    commandSource: model.commandSource,
                    onSwitchModel: { currentModel in
                        switchInitialModel = currentModel
                        switchChatFrom = row
                    },
                    isCurrentSession: { model.selectedSession?.id == row.id },
                    isVisible: !documentVisible,
                    onUsageChanged: { usage in
                        if model.selectedSession?.id == row.id { reportedUsage = (row.id, usage) }
                    },
                    onShowUsage: { storedSidebarMode = WorkspaceSidebarMode.usage.rawValue; sidebarVisible = true }
                )
                .id(row.id)
            }
        } else if let workspace = model.selectedWorkspace {
            VStack(spacing: 18) {
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

    private var sidebarModes: some View {
        HStack(spacing: 0) {
            ForEach(WorkspaceSidebarMode.allCases, id: \.self) { mode in
                Button {
                    storedSidebarMode = mode.rawValue
                    if mode == .workspaces { documentVisible = false }
                } label: {
                    Image(systemName: mode.symbol)
                        .frame(maxWidth: .infinity).frame(height: 38)
                        .background(sidebarMode == mode ? Color.accentColor.opacity(0.15) : .clear)
                        .overlay(alignment: .bottom) {
                            if sidebarMode == mode { Rectangle().fill(Color.accentColor).frame(height: 2) }
                        }
                }
                .buttonStyle(.plain).help(mode.rawValue).accessibilityLabel(mode.rawValue)
                .accessibilityAddTraits(sidebarMode == mode ? [.isSelected] : [])
            }
        }
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

    private var workspaceSidebar: some View {
        VStack(spacing: 0) {
            VStack(alignment: .leading, spacing: 18) {
                Text("Swarm").font(.title3.weight(.semibold))
                Button {
                    selectedProjectID = nil
                    showingArchive = false
                    model.showHome()
                } label: { Label("Home", systemImage: "house") }
                Button {
                    if KeyRouting.route(focus: .sidebar, key: .commandN) == .openNewWorkspace {
                        showingCreate = true
                    }
                } label: { Label("Create", systemImage: "plus") }
                    .keyboardShortcut("n", modifiers: .command)
                Button {
                    searching.toggle()
                    searchFocused = searching
                    if !searching { search = "" }
                } label: { Label("Search", systemImage: "magnifyingglass") }
                    .keyboardShortcut("k", modifiers: .command)
                if searching {
                    TextField("Search workspaces", text: $search)
                        .textFieldStyle(.roundedBorder)
                        .focused($searchFocused)
                }
            }
            .buttonStyle(.plain)
            .foregroundStyle(.secondary)
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(18)
            Divider()
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 6) {
                    if showingArchive {
                        workspaceSection("Archived", entries: visibleWorkspaces.filter {
                            model.navigation.archived.contains($0.id)
                        })
                    } else {
                        workspaceSection("Pinned", entries: visibleWorkspaces.filter {
                            model.navigation.pinned.contains($0.id) && !model.navigation.archived.contains($0.id)
                        })
                        workspaceSection("My workspaces", entries: visibleWorkspaces.filter {
                            !model.navigation.pinned.contains($0.id) && !model.navigation.archived.contains($0.id)
                        })
                    }
                }
                .padding(.horizontal, 8)
                .padding(.bottom, 12)
            }
            Divider()
            HStack {
                Button {
                    showingArchive.toggle()
                } label: {
                    Label(showingArchive ? "Workspaces" : "Archived", systemImage: "clock.arrow.circlepath")
                }
                Spacer()
                Menu {
                    Button("Open Project…", action: openExistingProject)
                    Button("Create Project…", action: createProject)
                } label: { Image(systemName: "folder.badge.plus") }
                    .menuStyle(.borderlessButton)
                    .fixedSize()
            }
            .buttonStyle(.plain)
            .foregroundStyle(.secondary)
            .padding(16)
        }
        .background(Color(nsColor: .windowBackgroundColor))
    }

    private var createWorkspacePicker: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text("Create workspace").font(.title2)
            Text("Choose a project").foregroundStyle(.secondary)
            ScrollView {
                VStack(alignment: .leading, spacing: 12) {
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
        .padding(24)
        .frame(width: 480, height: 340)
    }

    private func workspaceTabs(for selected: SwarmProjectSession) -> some View {
        let chats = model.tree.workspaceChats(for: selected.id)
        return HStack(spacing: 8) {
            if let workspace = model.selectedWorkspace {
                Text(model.navigation.title(for: workspace))
                    .font(.body.weight(.semibold))
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .frame(maxWidth: 220, alignment: .leading)
                    .padding(.leading, 12)
            }
            ScrollViewReader { proxy in
                ScrollView(.horizontal) {
                    HStack(spacing: 4) {
                        ForEach(chats) { chat in
                            Button {
                                model.select(chat.id)
                            } label: {
                                HStack(spacing: 6) {
                                    Text(chat.session.title)
                                        .lineLimit(1)
                                    if let provider = chat.session.provider {
                                        Text(providerBadge(provider))
                                            .font(.caption2)
                                            .foregroundStyle(.secondary)
                                    }
                                }
                                .frame(width: 180, alignment: .leading)
                                .padding(.horizontal, 10)
                                .padding(.vertical, 10)
                                .overlay(alignment: .bottom) {
                                    Rectangle()
                                        .fill(chat.id == selected.id ? Color.primary.opacity(0.75) : Color.clear)
                                        .frame(height: 2)
                                }
                            }
                            .buttonStyle(.plain)
                            .accessibilityAddTraits(chat.id == selected.id ? .isSelected : [])
                            .id(chat.id)
                            .contextMenu { chatMenu(chat) }
                        }
                    }
                }
                .scrollIndicators(.hidden)
                .onAppear { proxy.scrollTo(selected.id) }
                .onChange(of: selected.id) { _, id in proxy.scrollTo(id) }
            }
            Button {
                if let path = chats.first?.workspacePath { newChatDirectory = path }
            } label: {
                Image(systemName: "plus")
            }
            .buttonStyle(.plain)
            .help("New chat in this workspace")
            .accessibilityLabel("New chat in this workspace")
            .padding(.trailing, 12)
        }
        .padding(.vertical, 5)
    }

    private var visibleWorkspaces: [WorkspaceEntry] {
        model.workspaces.filter { model.navigation.matches(search, entry: $0) }
    }

    private var renameWorkspaceSheet: some View {
        VStack(alignment: .leading, spacing: 16) {
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
        .padding(24)
        .frame(width: 380)
    }

    private func workspaceSection(_ title: String, entries: [WorkspaceEntry]) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(title)
                .font(.subheadline.weight(.medium))
                .foregroundStyle(Color.primary.opacity(0.7))
                .padding(.horizontal, 10)
                .padding(.top, 20)
                .padding(.bottom, 8)
            if entries.isEmpty {
                Text(search.isEmpty ? "No workspaces" : "No matches")
                    .font(.caption).foregroundStyle(.secondary)
                    .padding(.horizontal, 10)
            }
            ForEach(entries) { entry in workspaceRow(entry) }
        }
    }

    private func workspaceRow(_ entry: WorkspaceEntry) -> some View {
        let label = model.navigation.title(for: entry) + ", "
            + model.navigation.detail(for: entry, among: model.workspaces)
        let help = "\(entry.project.name) · \(entry.workspace.name)\n\(entry.id)"
            + (entry.isRunning ? "\nAn agent process is alive" : "")
        return Button {
            if model.navigation.archived.contains(entry.id) {
                model.navigation.archived.remove(entry.id)
                showingArchive = false
            }
            selectedProjectID = nil
            model.selectWorkspace(entry)
        } label: {
            workspaceRowLabel(entry)
                .padding(.vertical, 8)
                .padding(.horizontal, 6)
                .frame(maxWidth: .infinity, alignment: .leading)
                .contentShape(Rectangle())
                .background(
                    model.navigation.selectedWorkspace == entry.id ? Color.primary.opacity(0.08) : Color.clear,
                    in: RoundedRectangle(cornerRadius: 6)
                )
        }
        .buttonStyle(.plain)
        .help(help)
        .accessibilityLabel(label)
        .accessibilityValue(entry.isRunning ? "Agent process alive" : "")
        .accessibilityAddTraits(model.navigation.selectedWorkspace == entry.id ? .isSelected : [])
        .contextMenu {
            if model.navigation.archived.contains(entry.id) {
                Button("Restore workspace") { model.navigation.archived.remove(entry.id) }
            } else {
                Button("New chat") { newChatDirectory = entry.id }
                Button(model.navigation.pinned.contains(entry.id) ? "Unpin workspace" : "Pin workspace") {
                    if model.navigation.pinned.contains(entry.id) { model.navigation.pinned.remove(entry.id) }
                    else { model.navigation.pinned.insert(entry.id) }
                }
                Button("Rename workspace…") {
                    workspaceName = model.navigation.title(for: entry)
                    renameTarget = entry
                }
                Button("Archive workspace") { model.archiveWorkspace(entry) }
            }
        }
    }

    private func workspaceRowLabel(_ entry: WorkspaceEntry) -> some View {
        let title = model.navigation.title(for: entry)
        let detail = model.navigation.detail(for: entry, among: model.workspaces)
        let age: String?
        if let chat = entry.chats.max(by: { $0.lastActivity < $1.lastActivity }) {
            age = SessionRowPresentation.make(
                ChatRow(session: chat, workspace: entry.workspace.name, workspacePath: entry.id),
                now: Int(Date().timeIntervalSince1970)
            ).age
        } else {
            age = nil
        }
        return HStack(spacing: 8) {
            RoundedRectangle(cornerRadius: 1)
                .fill(entry.isRunning ? Color.green : Color.clear)
                .frame(width: 2, height: 32)
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 4) {
                Text(title)
                    .font(.system(size: 14, weight: .medium))
                    .foregroundStyle(.primary)
                    .lineLimit(1)
                    .truncationMode(.tail)
                Text(detail)
                    .font(.system(size: 12))
                    .foregroundStyle(Color.primary.opacity(0.7))
                    .lineLimit(1)
                    .truncationMode(.middle)
            }
            Spacer(minLength: 4)
            if let age {
                Text(age)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .help("Last chat activity " + age + " ago")
            }
        }
    }

    @ViewBuilder
    private func chatMenu(_ row: ChatRow) -> some View {
        let presentation = SessionRowPresentation.make(
            row, now: Int(Date().timeIntervalSince1970)
        )
        Button("New chat here") { newChatDirectory = row.workspacePath }
        Button("Close chat") {
            Task {
                do { try await model.close(row.id) }
                catch { actionError = String(describing: error) }
            }
        }
        .disabled(presentation.state != .live)
        Button("Archive chat") {
            Task {
                do { try await model.archive(row.id) }
                catch { actionError = String(describing: error) }
            }
        }
    }

    private func providerBadge(_ provider: String) -> String {
        switch provider.lowercased() {
        case "codex": "X"
        case "agy": "A"
        default: provider.prefix(1).uppercased()
        }
    }

    private func openExistingProject() {
        let panel = NSOpenPanel()
        panel.canChooseDirectories = true
        panel.canChooseFiles = false
        panel.allowsMultipleSelection = false
        panel.prompt = "Open Project"
        guard panel.runModal() == .OK, let url = panel.url else { return }
        Task {
            do {
                let id = try await model.openProject(url)
                model.showHome()
                selectedProjectID = id
            } catch {
                actionError = error.localizedDescription
            }
        }
    }

    private func createProject() {
        let panel = NSSavePanel()
        panel.title = "Create Project"
        panel.prompt = "Create Project"
        panel.nameFieldStringValue = "New Project"
        panel.canCreateDirectories = true
        guard panel.runModal() == .OK, let url = panel.url else { return }
        Task {
            do {
                let id = try await model.createProject(at: url)
                model.showHome()
                selectedProjectID = id
            } catch {
                actionError = error.localizedDescription
            }
        }
    }

}

private struct ProjectHome: View {
    let project: ProjectNode
    let onNewChat: (String) -> Void
    let onNewTask: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
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
                    LazyVStack(spacing: 12) {
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
        .padding(24)
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
        VStack(alignment: .leading, spacing: 16) {
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
                    Task {
                        isCreating = true
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
        .padding(24)
        .frame(width: 480)
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

private struct LaunchTarget: Identifiable {
    let directory: String
    var id: String { directory }
}

struct SwarmApp: App {
    init() { SwarmPerformance.event("AppStarted") }

    var body: some Scene {
        WindowGroup { SessionsWindow() }
            .commands {
                DebugCommands()
                PaneFindCommands()
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

private struct PaneFindCommands: Commands {
    @FocusedValue(\.paneFindActions) private var actions

    var body: some Commands {
        CommandGroup(after: .textEditing) {
            Divider()
            Button("Find…") { route(.commandF, action: .showFindPanel) }
                .keyboardShortcut("f", modifiers: .command)
            Button("Find Next") { route(.commandG, action: .next) }
                .keyboardShortcut("g", modifiers: .command)
            Button("Find Previous") { route(.shiftCommandG, action: .previous) }
                .keyboardShortcut("g", modifiers: [.command, .shift])
        }
    }

    private func route(_ key: RoutedKey, action: NSFindPanelAction) {
        let focus: FocusedSurface = actions?.terminalFocused == false ? .transcript : .terminal
        switch KeyRouting.route(focus: focus, key: key) {
        case .openFind: actions?.open()
        case .findNext: actions?.next()
        case .findPrevious: actions?.previous()
        case .terminal:
            let item = NSMenuItem()
            item.tag = Int(action.rawValue)
            NSApp.sendAction(
                #selector(NSTextView.performFindPanelAction(_:)), to: nil, from: item
            )
        default: break
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
        } else if arguments.count == 3, arguments[0] == "--attach-check" {
            await attachCheck(prefix: arguments[1], agentID: SwarmAgentID(arguments[2]))
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

    private static func attachCheck(prefix: String, agentID: SwarmAgentID) async {
        do {
            let session = try await matchingSession(prefix: prefix)
            let bus = SwarmCLIBus()
            guard let agent = try await bus.agents(in: session).first(where: { $0.id == agentID }) else {
                throw SwarmProfileError.failed("agent not found")
            }
            if let reason = SwarmPanePolicy.unavailableReason(session: session, agent: agent) {
                throw SwarmProfileError.failed(reason)
            }
            await LoginShellPath.ready()
            let store = AgentPaneStore()
            let terminal = store.terminal(session: session, agent: agent)
            try await Task.sleep(for: .seconds(2))
            let alive = terminal.process.running
            print("child alive: \(alive)")
            print("first screen line: \(terminal.firstScreenLine)")
            store.stopAll()
            if !alive { exit(1) }
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
