import SwiftUI
import SwarmCore

struct ChatWindow: View {
    let model: SessionsTreeModel
    @State private var state: ChatWindowState
    @State private var details = SessionDetailStore()
    @State private var panes = AgentPaneStore()
    @State private var error: String?
    @State private var confirmingChild: SwarmAgentID?
    @State private var switching = false
    @State private var showingUsage = false
    @State private var currentModel: String?

    init(sessionID: SwarmSessionID, model: SessionsTreeModel) {
        self.model = model
        _state = State(initialValue: ChatWindowState(selection: sessionID))
    }

    private var row: SwarmProjectSession? { model.tree.session(state.selection) }
    private var agents: [SwarmAgent] { row.flatMap { model.tree.agentsBySession[$0.id] } ?? [] }
    private var readOnlyReason: String? {
        row.flatMap { model.workspace(containingChat: $0.id) }
            .flatMap { model.navigation.readOnlyReason(in: $0.id) }
    }

    var body: some View {
        VStack(spacing: 0) {
            if let error = error ?? model.error {
                Label(error, systemImage: "exclamationmark.triangle")
                    .foregroundStyle(.red).textSelection(.enabled).padding(DesignTokens.Spacing.m)
            }
            if let child = confirmingChild {
                HStack {
                    Text("Close “\(child.rawValue)”? This stops the agent.")
                    Button("Cancel") { confirmingChild = nil }
                    Button("Close", role: .destructive) {
                        confirmingChild = nil
                        if let row { performChildAction(child, in: row.session, close: true) }
                    }
                }
                .padding(DesignTokens.Spacing.m)
            }
            if let row, let detail = details.entries.first(where: { $0.id == row.id })?.model {
                SessionDetailView(
                    row: row, model: detail, agents: agents, readOnlyReason: readOnlyReason,
                    launchedModel: model.launchedModels[row.id], launchedTrust: model.launchedTrust[row.id] ?? [],
                    panes: panes, paneWidths: state.paneWidths,
                    onPaneWidthsChanged: { state.paneWidths = $0 }, commandSource: nil,
                    onSwitchModel: { currentModel = $0; switching = true },
                    isCurrentSession: { self.row?.id == row.id }, isActive: true, isVisible: true,
                    onUsageChanged: { _ in }, onShowUsage: { showingUsage = true },
                    dismissedChildren: model.navigation.dismissedChildren[ChatTitle.key(row)] ?? [],
                    onDismissChildren: { model.navigation.dismissFinishedChildren($0, in: row, agents: agents) },
                    onStopChild: { performChildAction($0, in: row.session, close: false) },
                    onCloseChild: { requestChildClose($0, in: row.session) }
                )
            } else if model.hasLoaded {
                ContentUnavailableView("Chat unavailable", systemImage: "bubble.left",
                                       description: Text("The chat is no longer in the active list."))
            } else {
                ProgressView("Loading chat…")
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .navigationTitle(row.map { model.navigation.title(for: $0) } ?? "Chat")
        .task {
            LoginShellPath.begin()
            model.attachWindow()
        }
        .onChange(of: row?.id, initial: true) { _, id in
            details.activate(id)
            panes.stop(keepingSession: id)
        }
        .onDisappear {
            panes.stopAll()
            details.activate(nil)
            model.detachWindow()
        }
        .sheet(isPresented: $switching) {
            if let row {
                SwitchModelSheet(
                    directory: row.session.cwd, currentProvider: row.provider, currentModel: currentModel,
                    launch: { plan, progress in
                        if let reason = readOnlyReason { throw SwarmProfileError.failed(reason) }
                        return try await SwarmChatHandoff.start(plan, after: row, bus: SwarmCLIBus(), onProgress: progress)
                    }
                ) { _ in
                    Task {
                        do { try await model.refresh() }
                        catch { self.error = error.localizedDescription }
                    }
                }
            }
        }
        .sheet(isPresented: $showingUsage) {
            VStack(alignment: .leading, spacing: DesignTokens.Spacing.m) {
                UsageDetails(usage: details.entries.first?.model.usage, hasChat: row != nil)
                Button("Close") { showingUsage = false }
            }
            .padding(DesignTokens.Spacing.xl)
        }
    }

    private func requestChildClose(_ id: SwarmAgentID, in session: SwarmSession) {
        guard readOnlyReason == nil else { return }
        Task {
            do {
                let agents = try await SwarmCLIBus().agents(in: session)
                guard let agent = agents.first(where: { $0.id == id }), agent.status != .ended else { return }
                if SwarmAgentCell(agent: agent).requiresCloseConfirmation { confirmingChild = id }
                else { performChildAction(id, in: session, close: true) }
            } catch { self.error = error.localizedDescription }
        }
    }

    private func performChildAction(_ id: SwarmAgentID, in session: SwarmSession, close: Bool) {
        guard readOnlyReason == nil else { return }
        Task {
            do {
                if close { try await SwarmCLIBus().close(id, in: session) }
                else { try await SwarmCLIBus().interrupt(id, in: session) }
            } catch { self.error = error.localizedDescription }
        }
    }
}
