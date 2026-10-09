import SwiftUI
import SwarmCore

struct ChatWindow: View {
    @Environment(\.designTokens) private var tokens
    let model: SessionsTreeModel
    @State private var state: ChatWindowState
    @State private var details = SessionDetailStore()
    @State private var panes = AgentPaneStore()
    @State private var banner = ChatBanner()
    @State private var refreshRunning = false
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
            if let message = banner.visible {
                HStack {
                    Label(message.text, systemImage: "exclamationmark.triangle")
                        .foregroundStyle(.red).textSelection(.enabled)
                    Spacer()
                    if message.source == .refresh {
                        Button("Retry") { Task { await refreshAfterSwitch() } }
                            .disabled(refreshRunning || model.activeRefreshes > 0)
                    }
                    Button("Dismiss") { updateBanner { $0.dismiss() } }
                }
                .padding(tokens.spacing.m)
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
                .padding(tokens.spacing.m)
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
            model.attachChatWindow(state.selection)
        }
        .onChange(of: row?.id, initial: true) { _, id in
            details.activate(id)
            panes.stop(keepingSession: id)
        }
        .onChange(of: model.refreshSuccessToken, initial: true) { _, _ in
            syncBanner()
        }
        .onChange(of: model.error, initial: true) { _, _ in
            syncBanner()
        }
        .onDisappear {
            panes.stopAll()
            details.activate(nil)
            model.detachWindow()
            model.detachChatWindow(state.selection)
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
                    Task { await refreshAfterSwitch() }
                }
            }
        }
        .sheet(isPresented: $showingUsage) {
            VStack(alignment: .leading, spacing: tokens.spacing.m) {
                ScrollView {
                    UsageDetails(usage: details.entries.first?.model.usage, chainUsage: row.map(model.chainUsage),
                                 hasChat: row != nil)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
                .frame(maxHeight: DesignTokens.Size.settingsHeight)
                Button("Close") { showingUsage = false }
            }
            .padding(tokens.spacing.xl)
        }
    }

    private func refreshAfterSwitch() async {
        guard !refreshRunning else { return }
        refreshRunning = true
        defer { refreshRunning = false }
        let token = model.refreshRevision + 1
        do {
            if try await model.refresh() {
                syncBanner()
            }
        } catch {
            // Read the latest success directly even if SwiftUI has not delivered its observer yet.
            syncBanner()
            let message = ErrorText.sentence("The chat switched, but the chat list did not refresh: \(error.localizedDescription)")
            updateBanner { $0.refreshFailed(message, token: token) }
        }
    }

    private func syncBanner() {
        // Sync model recovery before a refresh clear can reveal its old failure.
        updateBanner { $0.setModel(model.error) }
        updateBanner { $0.refreshSucceeded(token: model.refreshSuccessToken) }
    }

    private func updateBanner(_ update: (inout ChatBanner) -> Void) {
        update(&banner)
        if let message = banner.announcement { AccessibilityNotification.Announcement(message).post() }
    }

    private func childActions(in session: SwarmSession) -> ChildAgentActions {
        ChildAgentActions(bus: SwarmCLIBus(), session: session,
                          confirm: { confirmingChild = $0 },
                          error: { message in updateBanner { $0.setAction(message) } })
    }

    private func requestChildClose(_ id: SwarmAgentID, in session: SwarmSession) {
        guard readOnlyReason == nil else { return }
        Task { await childActions(in: session).requestChildClose(id) }
    }

    private func performChildAction(_ id: SwarmAgentID, in session: SwarmSession, close: Bool) {
        guard readOnlyReason == nil else { return }
        Task { await childActions(in: session).performChildAction(id, close: close) }
    }
}
