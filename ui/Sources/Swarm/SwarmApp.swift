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

    var tree = SessionsTree(projects: [])
    var selectedSessionID: SwarmSessionID?
    private var pendingID: SwarmSessionID?
    var agents: [SwarmAgent] = []
    var commandSource: ComposerCommandSource?
    private var commandSourceKey: String?
    var error: String?

    var selectedSession: SwarmProjectSession? { selectedSessionID.flatMap(tree.session) }

    func select(_ id: SwarmSessionID?) {
        pendingID = nil
        selectedSessionID = id
        agents = []
        commandSource = nil
        commandSourceKey = nil
    }

    func startChat(_ plan: SwarmChatLaunchPlan) async throws -> SwarmSessionID {
        try await SwarmChatLauncher.start(plan, bus: bus) { id in
            await MainActor.run {
                self.pendingID = id
                self.selectedSessionID = id
                self.agents = []
            }
        }
    }

    func switchChat(_ plan: SwarmChatLaunchPlan, from row: SwarmProjectSession) async throws -> SwarmSessionID {
        let id = try await SwarmChatHandoff.start(plan, after: row, bus: bus)
        pendingID = id
        selectedSessionID = id
        agents = []
        return id
    }

    func refresh() async throws {
        let sessions = try await bus.sessions()
        drafts.prune(keeping: Set(sessions.map { $0.id.rawValue }))
        tree = try await discovery.tree(sessions: sessions, projectPaths: projects.paths(), bus: bus)
        if let selectedSessionID, let row = tree.session(selectedSessionID) {
            pendingID = nil
            self.selectedSessionID = row.id
            agents = try await bus.agents(in: row.session)
            let provider = row.provider ?? agents.first {
                $0.id == SwarmPanePolicy.chair
            }?.provider
            let key = row.id.rawValue + (row.session.chairLog ?? "") + (provider ?? "")
            if commandSourceKey != key {
                let source = await discovery.composerCommandSource(
                    for: row.session, provider: provider
                )
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
        let path = try projects.add(url)
        try await refresh()
        return SwarmSessionDiscovery.identity(for: path, repositoryPathsResolver: Git.repositoryPaths)
    }

    func createProject(at url: URL) async throws -> SwarmPathIdentity {
        let path = try projects.create(at: url)
        try await refresh()
        return SwarmSessionDiscovery.identity(for: path, repositoryPathsResolver: Git.repositoryPaths)
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
        while !Task.isCancelled {
            do { try await refresh() }
            catch { self.error = String(describing: error) }
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
    @State private var switchChatFrom: SwarmProjectSession?
    @State private var selectedProjectID: SwarmPathIdentity?
    @State private var actionError: String?

    var body: some View {
        NavigationSplitView {
            List(selection: $model.selectedSessionID) {
                Button {
                    selectedProjectID = nil
                    model.select(nil)
                } label: {
                    Label("Home", systemImage: "house")
                }
                .buttonStyle(.plain)
                ForEach(sidebarRows) { entry in
                    switch entry {
                    case .project(let project):
                        ProjectRowLabel(
                            name: project.name,
                            onSelect: {
                                model.select(nil)
                                selectedProjectID = project.id
                            },
                            onNewChat: { newChatDirectory = project.launchDirectory }
                        )
                    case .chat(let row):
                        chatRow(row)
                        .padding(.leading, 16)
                        .tag(row.id)
                        .contextMenu { chatMenu(row) }
                    }
                }
            }
            .listStyle(.sidebar)
            .navigationTitle("Workspaces")
            .navigationSplitViewColumnWidth(min: 240, ideal: 300)
            .onChange(of: model.selectedSessionID) { oldID, id in
                guard oldID != id else { return }
                NSApp.keyWindow?.makeFirstResponder(nil)
                panes.clearFocus()
                if id != nil { selectedProjectID = nil }
                model.select(id)
            }
            .toolbar {
                Menu {
                    Button("Open Project…", action: openExistingProject)
                    Button("Create Project…", action: createProject)
                } label: {
                    Image(systemName: "folder.badge.plus")
                }
                .help("Projects")
                .accessibilityLabel("Projects")
                Button {
                    if KeyRouting.route(focus: .sidebar, key: .commandN) == .openNewChat,
                       let id = model.selectedSessionID,
                       let path = model.tree.launchDirectory(for: id) {
                        newChatDirectory = path
                    }
                } label: {
                    Image(systemName: "plus.circle")
                }
                .help("New chat")
                .accessibilityLabel("New chat")
                .keyboardShortcut("n", modifiers: .command)
                .disabled(model.selectedSession == nil)
            }
        } detail: {
            if let row = model.selectedSession {
                SessionDetailView(
                    row: row,
                    title: model.selectedSessionID.flatMap(model.tree.windowTitle) ?? row.title,
                    agents: model.agents, panes: panes,
                    commandSource: model.commandSource,
                    onSwitchModel: { switchChatFrom = row },
                    isCurrentSession: { model.selectedSession?.id == row.id }
                )
                    .id(row.id)
            } else if let project = model.tree.projects.first(where: { $0.id == selectedProjectID }) {
                ProjectHome(project: project) {
                    newChatDirectory = project.launchDirectory
                }
            } else {
                AgentProfilesHome(
                    sessionsError: model.error,
                    onOpenProject: openExistingProject,
                    onCreateProject: createProject
                )
            }
        }
        .background(WindowFrameRestorer())
        .task {
            LoginShellPath.begin()
            await model.run()
        }
        .onDisappear { panes.stopAll() }
        .onReceive(NotificationCenter.default.publisher(for: NSApplication.willTerminateNotification)) { _ in
            panes.stopAll()
        }
        .sheet(item: Binding(
            get: { newChatDirectory.map(LaunchTarget.init) },
            set: { newChatDirectory = $0?.directory }
        )) { target in
            NewChatSheet(directory: target.directory, launch: model.startChat) { _ in
                Task { try? await model.refresh() }
            }
        }
        .sheet(item: $switchChatFrom) { row in
            NewChatSheet(
                directory: row.session.cwd, isSwitch: true, initialProvider: row.provider,
                launch: { plan in try await model.switchChat(plan, from: row) }
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

    private func chatRow(_ row: ChatRow) -> some View {
        let presentation = SessionRowPresentation.make(
            row, now: Int(Date().timeIntervalSince1970)
        )
        return HStack(spacing: 8) {
            RoundedRectangle(cornerRadius: 1.5)
                .fill(presentation.state == .live ? Color.green : Color.clear)
                .frame(width: 3)
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 2) {
                HStack(spacing: 5) {
                    Text(presentation.title)
                        .font(.body)
                        .foregroundStyle(
                            presentation.state == .ended ? Color.secondary : Color.primary
                        )
                        .lineLimit(1)
                        .truncationMode(.tail)
                    if let provider = presentation.provider {
                        Text(providerBadge(provider))
                            .font(.caption2)
                            .frame(width: 16, height: 16)
                            .background(Circle().fill(.quaternary))
                            .accessibilityLabel(provider)
                    }
                }
                Text(presentation.caption)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
            Spacer()
            Text(presentation.age)
                .font(.caption)
                .foregroundStyle(.secondary)
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
        Button("Archive") {
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
                model.select(nil)
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
                model.select(nil)
                selectedProjectID = id
            } catch {
                actionError = error.localizedDescription
            }
        }
    }

    private var sidebarRows: [SidebarRow] {
        var rows: [SidebarRow] = []
        for project in model.tree.projects {
            rows.append(.project(project))
            rows += project.chats.map(SidebarRow.chat)
        }
        return rows
    }
}

private struct ProjectRowLabel: View {
    let name: String
    let onSelect: () -> Void
    let onNewChat: () -> Void
    @State private var hovered = false

    var body: some View {
        HStack {
            Button(action: onSelect) {
                Text(name)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .truncationMode(.middle)
            }
            .buttonStyle(.plain)
            Spacer()
            Button(action: onNewChat) {
                Image(systemName: "plus.circle")
            }
            .buttonStyle(.borderless)
            .help("New chat")
            .accessibilityLabel("New chat")
            .opacity(hovered ? 1 : 0)
            .allowsHitTesting(hovered)
        }
        .onHover { hovered = $0 }
    }
}

private struct ProjectHome: View {
    let project: ProjectNode
    let onNewChat: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text(project.name).font(.largeTitle.bold())
            Text(project.launchDirectory).foregroundStyle(.secondary).textSelection(.enabled)
            Button("New chat", action: onNewChat)
        }
        .padding(24)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
    }
}

private enum SidebarRow: Identifiable {
    case project(ProjectNode)
    case chat(ChatRow)

    var id: String {
        switch self {
        case .project(let project): "project:\(project.path)"
        case .chat(let row): "chat:\(row.id.rawValue)"
        }
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

    var body: some Commands {
        CommandMenu("Debug") {
            Toggle("Show Raw Data", isOn: $showRawData)
                .keyboardShortcut("r", modifiers: [.command, .option])
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
