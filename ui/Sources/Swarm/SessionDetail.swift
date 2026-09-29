import AppKit
import Observation
import SwiftUI
import SwarmCore

@MainActor @Observable
final class SessionDetailModel {
    private var transcripts: [SwarmSessionID: SwarmChairTranscript] = [:]
    private let bus = SwarmCLIBus()
    private let drafts = ComposerDraftStore()
    private var activeSessionID: String?

    var usage = ChatUsage()
    var currentModel: String?
    var snapshot: ChairTranscriptSnapshot
    private(set) var transcriptRevision = 0
    var draft = ""
    private var sendState = ComposerSendState()
    private var loadedSessionCount = 1
    private var snapshots: [SwarmSessionID: ChairTranscriptSnapshot] = [:]
    var hasOlder = false
    var isLoadingOlder = false
    var historyError: String?

    /// Starts from the chat's last known content, so reopening it shows that at once while the
    /// transcript reloads and updates it in place.
    init(lastKnown: ChairTranscriptSnapshot? = nil) {
        snapshot = lastKnown ?? .loading
    }

    func isSending(sessionID: String) -> Bool {
        sendState.isSending(sessionID: sessionID)
    }

    var rows: [TranscriptRow] {
        if case .rows(let rows, _) = snapshot { return rows }
        return []
    }

    var rawEntries: [RawTranscriptEntry] {
        if case .rows(_, let raw) = snapshot { return raw }
        return []
    }

    func poll(row: SwarmProjectSession, chairProvider: String?) async {
        let openTiming = SwarmPerformance.begin("ChatOpen")
        var opened = false
        defer { if !opened { openTiming.end() } }
        activate(sessionID: row.id.rawValue)
        while !Task.isCancelled {
            let cycleTiming = SwarmPerformance.begin("TranscriptComposition")
            if isLoadingOlder {
                cycleTiming.end()
                try? await Task.sleep(for: .milliseconds(100))
                continue
            }
            let session = row.session
            let transcript = transcripts[session.id] ?? SwarmChairTranscript()
            transcripts[session.id] = transcript
            let result = await transcript.poll(
                session: session, chairProvider: session.chairProvider ?? chairProvider
            )
            guard !Task.isCancelled else { cycleTiming.end(); break }
            snapshots[session.id] = result
            currentModel = await transcript.currentModel
            usage = await transcript.usage
            await updateHistoryAvailability(row: row)
            compose(row: row)
            cycleTiming.end(count: rows.count)
            if !opened {
                openTiming.end(count: rows.count)
                opened = true
            }
            try? await Task.sleep(for: .seconds(1))
        }
    }

    func loadOlder(row: SwarmProjectSession, chairProvider: String?) async {
        guard hasOlder, !isLoadingOlder else { return }
        isLoadingOlder = true
        historyError = nil
        defer { isLoadingOlder = false }
        do {
            let oldest = row.sessions[loadedSessionCount - 1]
            if let transcript = transcripts[oldest.id], await transcript.hasOlder {
                snapshots[oldest.id] = try await transcript.loadOlder()
            } else if loadedSessionCount < row.sessions.count {
                let previous = row.sessions[loadedSessionCount]
                let transcript = SwarmChairTranscript()
                transcripts[previous.id] = transcript
                snapshots[previous.id] = await transcript.poll(
                    session: previous, chairProvider: previous.chairProvider ?? chairProvider
                )
                loadedSessionCount += 1
            }
            if let current = transcripts[row.id] {
                usage = await current.usage
                currentModel = await current.currentModel
            }
            await updateHistoryAvailability(row: row)
            compose(row: row)
        } catch {
            historyError = String(describing: error)
        }
    }

    private func updateHistoryAvailability(row: SwarmProjectSession) async {
        let oldest = row.sessions[loadedSessionCount - 1]
        let moreInSession = await transcripts[oldest.id]?.hasOlder ?? false
        hasOlder = moreInSession || loadedSessionCount < row.sessions.count
    }

    private func compose(row: SwarmProjectSession) {
        var rows: [TranscriptRow] = []
        var raw: [RawTranscriptEntry] = []
        for session in row.sessions.prefix(loadedSessionCount).reversed() {
            guard let result = snapshots[session.id] else { continue }
            if case .rows(let sessionRows, let sessionRaw) = result {
                if !rows.isEmpty {
                    rows.append(TranscriptRow(
                        kind: .notice, text: "Model switched to \(session.chairProvider ?? "agent")",
                        eventID: "switch-\(session.id.rawValue)"
                    ))
                }
                rows += sessionRows.map { row in
                    var copy = row
                    copy.eventID = session.id.rawValue + ":" + row.eventID
                    return copy
                }
                raw += sessionRaw.map { entry in
                    var copy = entry
                    copy.sessionID = session.id.rawValue
                    return copy
                }
            } else if session.id != row.id {
                rows.append(TranscriptRow(
                    kind: .notice, text: result.printText, eventID: "history-\(session.id.rawValue)"
                ))
            }
        }
        let latest = snapshots[row.id] ?? .loading
        if !rows.isEmpty, case .unavailable(let message) = latest {
            rows.append(TranscriptRow(kind: .error, text: message, eventID: "unavailable-\(row.id.rawValue)"))
        }
        let next: ChairTranscriptSnapshot = rows.isEmpty ? latest : .rows(rows, raw: raw)
        if snapshot != next {
            snapshot = next
            transcriptRevision += 1
        }
    }

    /// Drops the chat's transcript readers, which stops their `transcript --follow` processes.
    /// The store calls this when it frees the chat, whoever else still holds the model.
    func release() {
        transcripts.removeAll()
        snapshots.removeAll()
        snapshot = .loading
    }

    func setDraft(_ value: String, sessionID: String) {
        if activeSessionID != sessionID { activate(sessionID: sessionID) }
        draft = value
        drafts.save(value, for: sessionID)
    }

    func send(_ requestedText: String, session: SwarmSession) async throws {
        let sessionID = session.id.rawValue
        guard let text = sendState.begin(sessionID: sessionID, draft: requestedText) else { return }
        do {
            try await bus.type(text, to: SwarmPanePolicy.chair, in: session)
            let current = activeSessionID == sessionID ? draft : drafts.draft(for: sessionID)
            let next = sendState.finish(
                sessionID: sessionID, currentDraft: current, succeeded: true
            )
            drafts.save(next, for: sessionID)
            if activeSessionID == sessionID { draft = next }
        } catch {
            let current = activeSessionID == sessionID ? draft : drafts.draft(for: sessionID)
            _ = sendState.finish(sessionID: sessionID, currentDraft: current, succeeded: false)
            throw error
        }
    }

    func interrupt(session: SwarmSession) async throws {
        try await bus.interrupt(SwarmPanePolicy.chair, in: session)
    }

    private func activate(sessionID: String) {
        guard activeSessionID != sessionID else { return }
        if let activeSessionID { drafts.save(draft, for: activeSessionID) }
        activeSessionID = sessionID
        currentModel = nil
        usage = ChatUsage()
        draft = drafts.draft(for: sessionID)
    }
}

@MainActor @Observable
final class SessionDetailStore {
    struct Entry: Identifiable {
        let id: SwarmSessionID
        let model: SessionDetailModel
    }

    private(set) var entries: [Entry] = []
    /// The last rows of the four most recent chats (ADR 0025). Readers, processes, and poll tasks
    /// are still freed on a switch; this keeps only their output.
    @ObservationIgnored private var lastKnown = RecentValues<SwarmSessionID, ChairTranscriptSnapshot>(capacity: 4)

    private func remember(_ snapshot: ChairTranscriptSnapshot, for id: SwarmSessionID) {
        if case .rows = snapshot { lastKnown.set(snapshot, for: id) }
    }

    /// Keeps only the selected chat, so a switch frees the previous transcript and poll (ADR 0025).
    func activate(_ id: SwarmSessionID?) {
        if let id, entries.count == 1, entries[0].id == id { return }
        let kept = entries.first { $0.id == id }
        for entry in entries where entry.id != id {
            remember(entry.model.snapshot, for: entry.id)
            entry.model.release()
        }
        entries = id.map { [kept ?? Entry(id: $0, model: SessionDetailModel(lastKnown: lastKnown[$0]))] } ?? []
    }
}

struct SessionDetailView: View {
    let row: SwarmProjectSession
    let model: SessionDetailModel
    let agents: [SwarmAgent]
    let panes: AgentPaneStore
    let commandSource: ComposerCommandSource?
    let onSwitchModel: (String?) -> Void
    let isCurrentSession: () -> Bool
    let isActive: Bool
    let isVisible: Bool
    let onUsageChanged: (ChatUsage) -> Void
    let onShowUsage: () -> Void

    @State private var didShowRows = false
    @FocusState private var composerFocused: Bool
    @FocusState private var transcriptFocused: Bool

    var body: some View {
        paneStrip
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .task(id: row.id.rawValue + (row.session.chairLog ?? "") + (chairProvider ?? "") + String(isActive)) {
            guard isActive else { return }
            await model.poll(row: row, chairProvider: chairProvider)
        }
        .onAppear {
            SwarmPerformance.event("ChatDetailAppeared")
        }
        .onChange(of: model.usage, initial: true) { _, usage in
            if isCurrentSession() { onUsageChanged(usage) }
        }
        .onChange(of: model.snapshot) { _, snapshot in
            guard !didShowRows, case .rows = snapshot else { return }
            didShowRows = true
            SwarmPerformance.event("ChatRowsShown")
        }
        .onChange(of: isVisible, initial: true) { _, visible in
            if visible {
                transcriptFocused = true
                onUsageChanged(model.usage)
            } else {
                transcriptFocused = false
                composerFocused = false
            }
        }
        .onChange(of: panes.focusedKey) { _, key in
            guard key != nil else { return }
            transcriptFocused = false
            composerFocused = false
        }
    }

    private var chairProvider: String? {
        agents.first { $0.id == SwarmPanePolicy.chair }?.provider
    }

    private var modelLabel: String {
        if let current = model.currentModel { return current }
        if model.snapshot == .waiting || model.snapshot == .loading { return "Reading model…" }
        return "\((row.provider ?? chairProvider)?.capitalized ?? "Agent") · Model not reported"
    }

    private var modelSwitchDisabledReason: String? {
        if model.snapshot == .waiting || model.snapshot == .loading { return "Waiting for this chat's model information." }
        if model.isSending(sessionID: row.id.rawValue)
            || (row.isRunning == true && ChairTurn.isActive(model.rows)) {
            return "Wait for the reply to finish, or stop it before switching model."
        }
        return nil
    }

    private var transcriptColumn: some View {
        TranscriptView(
            snapshot: model.snapshot, revision: model.transcriptRevision,
            hasOlder: model.hasOlder, isLoadingOlder: model.isLoadingOlder,
            historyError: model.historyError,
            waitingMessage: ChairTranscriptSnapshot.waitingMessage(isRunning: row.isRunning),
            chair: row.provider ?? chairProvider,
            rawSessionJSON: TranscriptDebugData.sessionJSON(session: row.session, agents: agents),
            isActive: isActive, isVisible: isVisible,
            loadOlder: {
                let before = model.snapshot
                await model.loadOlder(row: row, chairProvider: chairProvider)
                return model.historyError == nil && model.snapshot != before
            },
            onTap: { panes.clearFocus() },
            focus: $transcriptFocused
        ) {
            ComposerView(
                sessionID: row.id.rawValue, isActive: isActive,
                draft: Binding(
                    get: { model.draft },
                    set: { model.setDraft($0, sessionID: row.id.rawValue) }
                ),
                isRunning: row.isRunning == true && ChairTurn.isActive(model.rows),
                isSending: model.isSending(sessionID: row.id.rawValue),
                modelLabel: modelLabel,
                modelSwitchDisabledReason: modelSwitchDisabledReason,
                selectModel: { onSwitchModel(model.currentModel) },
                usageLabel: model.usage.summary,
                showUsage: onShowUsage,
                sendDisabledReason: agents.first(where: { $0.id == SwarmPanePolicy.chair })?.alive == false
                    ? "This chat's pane has closed. Start a new chat or switch model."
                    : nil,
                commandSource: commandSource ?? ComposerCommandSource(
                    provider: row.provider ?? chairProvider,
                    homeDirectory: FileManager.default.homeDirectoryForCurrentUser.path,
                    projectDirectory: row.session.cwd
                ),
                mentionSource: ComposerMentionSource(root: row.session.cwd),
                scratchDirectory: AgentScratchDirectory.current(),
                focus: $composerFocused,
                send: { try await model.send($0, session: row.session) },
                interrupt: { try await model.interrupt(session: row.session) },
                onFocused: { panes.clearFocus() },
                isCurrentSession: isCurrentSession
            )
            .id(row.id.rawValue)
        }
        // Scene-wide, so the menu finds the chat without it holding keyboard focus.
        .background {
            if isVisible {
                Color.clear.focusedSceneValue(\.chatKeyActions, chatKeyActions)
            }
        }
    }

    private var paneKeys: [String] {
        guard isActive else { return [] }
        return SwarmPanePolicy.cells(session: row.session, agents: agents).map {
            AgentPaneStore.key(session: row.session.id, agent: $0.agent.id.rawValue)
        }
    }

    private var chatKeyActions: ChatKeyActions {
        let keys = paneKeys
        return ChatKeyActions(
            terminalFocused: panes.focusedKey != nil,
            focusComposer: {
                panes.revealChat()
                composerFocused = true
            },
            moveFocus: { direction in
                guard !panes.moveFocus(direction, among: keys) else { return }
                NSApp.keyWindow?.makeFirstResponder(nil)
                transcriptFocused = true
            },
            zoom: { panes.toggleZoom() },
            stop: { Task { try? await model.interrupt(session: row.session) } }
        )
    }

    private var paneStrip: some View {
        let session = row.session
        let agentCells = isActive ? SwarmPanePolicy.cells(session: session, agents: agents) : []
        let byID = Dictionary(agentCells.map { ($0.agent.id.rawValue, $0) }) { first, _ in first }
        func key(_ id: String) -> String { AgentPaneStore.key(session: session.id, agent: id) }
        return PaneStrip(
            cells: agentCells.map { cell in
                PaneCell(
                    id: cell.agent.id.rawValue, title: cell.agent.id.rawValue, role: cell.agent.role,
                    model: cell.agent.provider ?? "unknown",
                    // A closed pane connection shows as ended even while the agent lives.
                    status: panes.ended.contains(key(cell.agent.id.rawValue)) ? .ended : cell.agent.status
                )
            },
            focusedID: agentCells.first { key($0.agent.id.rawValue) == panes.focusedKey }?.agent.id.rawValue,
            zoomedID: agentCells.first { key($0.agent.id.rawValue) == panes.zoomedKey }?.agent.id.rawValue,
            revealID: agentCells.first { key($0.agent.id.rawValue) == panes.revealKey }?.agent.id.rawValue,
            onFocus: { panes.focus(key: key($0)) },
            onZoom: { panes.zoomedKey = $0.map(key) },
            onReconnect: { id in
                if let agent = byID[id]?.agent { panes.reconnect(session: session, agent: agent) }
            }
        ) {
            transcriptColumn
        } pane: { cell in
            switch byID[cell.id]?.kind {
            case .attach?:
                AgentTerminalView(key: key(cell.id), store: panes) {
                    guard !SwarmOpenScript.isActive, let agent = byID[cell.id]?.agent else { return }
                    _ = panes.terminal(session: session, agent: agent)
                }
            case .notice(let reason)?:
                ContentUnavailableView(reason, systemImage: "terminal")
            case nil:
                EmptyView()
            }
        }
    }
}
