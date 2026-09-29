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
    var snapshot: ChairTranscriptSnapshot = .loading
    private(set) var transcriptRevision = 0
    var draft = ""
    private var sendState = ComposerSendState()
    private var loadedSessionCount = 1
    private var snapshots: [SwarmSessionID: ChairTranscriptSnapshot] = [:]
    var hasOlder = false
    var isLoadingOlder = false
    var historyError: String?

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
        if snapshot == .loading {
            // Let the selection and loading state draw before publishing a cold history page.
            do { try await Task.sleep(for: .milliseconds(50)) }
            catch { return }
        }
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

    /// Keeps only the selected chat, so a switch frees the previous transcript and poll (ADR 0024).
    func activate(_ id: SwarmSessionID?) {
        guard let id else { entries = []; return }
        if entries.count == 1, entries[0].id == id { return }
        entries = [entries.first { $0.id == id } ?? Entry(id: id, model: SessionDetailModel())]
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

    @State private var followsTail = true
    @State private var atBottom = true
    @State private var userScrolling = false
    @State private var nearTop = false
    @State private var loadingHistory = false
    @State private var historyAnchor: String?
    @State private var loadedHistoryThisGesture = false
    @State private var showHiddenRows = false
    @AppStorage("showRawData") private var showRawData = false
    @State private var findPresented = false
    @State private var findQuery = ""
    @State private var findMatches: [String] = []
    @State private var findMatchSet: Set<String> = []
    @State private var isSearching = false
    @State private var findMatchID: String?
    @State private var pendingScrollID: String?
    @State private var didShowRows = false
    @FocusState private var composerFocused: Bool
    @FocusState private var findFieldFocused: Bool
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
                findFieldFocused = false
            }
        }
        .onChange(of: panes.focusedKey) { _, key in
            guard key != nil else { return }
            transcriptFocused = false
            composerFocused = false
            findFieldFocused = false
        }
    }

    private func loadOlder(automatic: Bool = false) {
        guard model.hasOlder, !loadingHistory, !automatic || !loadedHistoryThisGesture else { return }
        loadedHistoryThisGesture = true
        loadingHistory = true
        followsTail = false
        historyAnchor = showRawData ? model.rawEntries.first?.id : visibleRows.first?.eventID
        let oldRows = model.rows
        let oldRaw = model.rawEntries
        Task { @MainActor in
            await model.loadOlder(row: row, chairProvider: chairProvider)
            // The row-change handler restores position after SwiftUI has received the new rows.
            if model.historyError != nil || (showRawData ? model.rawEntries == oldRaw : model.rows == oldRows) {
                historyAnchor = nil
            }
            loadingHistory = false
        }
    }

    private func restoreHistoryPosition(using proxy: ScrollViewProxy) -> Bool {
        guard let anchor = historyAnchor else { return false }
        historyAnchor = nil
        Task { @MainActor in proxy.scrollTo(anchor, anchor: .top) }
        return true
    }

    private func followLatest(using proxy: ScrollViewProxy) {
        guard followsTail, !userScrolling, !loadingHistory, !findPresented, isVisible,
              let id = lastVisibleID else { return }
        proxy.scrollTo(id, anchor: .bottom)
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
        VStack(spacing: 0) {
            HStack {
                Text("Transcript").font(.headline)
                Spacer()
                if showRawData {
                    Text("RAW")
                        .font(.caption2.bold())
                        .padding(.horizontal, 6)
                        .padding(.vertical, 2)
                        .background(Capsule().fill(Color.orange.opacity(0.2)))
                        .foregroundStyle(.orange)
                }
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 8)
            Divider()
            if findPresented { findBar }
            ScrollViewReader { proxy in
                VStack(spacing: 0) {
                    ScrollView {
                        LazyVStack(alignment: .leading, spacing: 12) {
                            if model.hasOlder {
                                Button(model.isLoadingOlder ? "Loading earlier messages…" : "Load earlier messages") {
                                    loadOlder()
                                }
                                .disabled(model.isLoadingOlder)
                            }
                            if let error = model.historyError {
                                Text(verbatim: error).font(.caption).foregroundStyle(.red)
                            }
                            switch model.snapshot {
                            case .loading:
                                ProgressView("Loading chat…")
                            case .waiting:
                                Text(ChairTranscriptSnapshot.waitingMessage(isRunning: row.isRunning))
                                    .foregroundStyle(.secondary)
                            case .notice(let message):
                                Text(verbatim: message).foregroundStyle(.secondary)
                            case .unavailable(let message):
                                Text(verbatim: message).foregroundStyle(.red)
                            case .rows(let rows, let raw):
                                if showRawData {
                                    rawSessionBlock
                                    ForEach(raw) { entry in
                                        rawEntry(entry)
                                            .onGeometryChange(for: CGFloat.self) { $0.size.height } action: { old, height in
                                                guard entry.id == lastVisibleID, height > old else { return }
                                                followLatest(using: proxy)
                                            }
                                    }
                                } else {
                                    let hidden = rows.filter(\.isHiddenByDefault).count
                                    if hidden > 0 {
                                        Button(showHiddenRows ? "Hide \(hidden) hidden rows" : "Show \(hidden) hidden rows") {
                                            showHiddenRows.toggle()
                                        }
                                    }
                                    ForEach(visibleRows) { transcriptRow in
                                        TranscriptRowView(
                                            row: transcriptRow,
                                            chair: self.row.provider ?? chairProvider,
                                            revealForSearch: currentMatchID == transcriptRow.eventID
                                        )
                                        .environment(\.transcriptSearchQuery, currentMatchID == transcriptRow.eventID ? findQuery : "")
                                        .padding(3)
                                        .background(matchBackground(transcriptRow.eventID))
                                        .onGeometryChange(for: CGFloat.self) { $0.size.height } action: { old, height in
                                            guard transcriptRow.eventID == lastVisibleID, height > old else { return }
                                            followLatest(using: proxy)
                                        }
                                    }
                                }
                            }
                        }
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .padding()
                    }
                    .defaultScrollAnchor(.bottom, for: .initialOffset)
                    .contentMargins(.top, 8, for: .scrollContent)
                    .frame(maxHeight: .infinity)
                    .simultaneousGesture(TapGesture().onEnded {
                        transcriptFocused = true
                        panes.clearFocus()
                    })
                    .onScrollGeometryChange(for: Bool.self) { geometry in
                        geometry.contentOffset.y + geometry.containerSize.height
                            >= geometry.contentSize.height - 32
                    } action: { _, atBottom in
                        self.atBottom = atBottom
                        followsTail = TranscriptTail.follows(
                            current: followsTail, atBottom: atBottom, userScrolled: userScrolling
                        )
                    }
                    .onScrollGeometryChange(for: Bool.self) { geometry in
                        geometry.contentOffset.y < 120
                    } action: { _, nearTop in
                        self.nearTop = nearTop
                        if nearTop, userScrolling { loadOlder(automatic: true) }
                    }
                    .onScrollPhaseChange { _, phase, context in
                        let wasScrolling = userScrolling
                        userScrolling = phase == .tracking || phase == .interacting || phase == .decelerating
                        if userScrolling {
                            // Cancel tail following before layout or queued updates can move the viewport.
                            followsTail = false
                        } else if wasScrolling, phase == .idle {
                            let geometry = context.geometry
                            followsTail = geometry.contentOffset.y + geometry.containerSize.height
                                >= geometry.contentSize.height - 32
                        }
                        if phase == .idle { loadedHistoryThisGesture = false }
                        if phase == .interacting, nearTop { loadOlder(automatic: true) }
                    }
                    .onChange(of: model.rows) {
                        guard !showRawData else { return }
                        if restoreHistoryPosition(using: proxy) { return }
                        Task { @MainActor in followLatest(using: proxy) }
                    }
                    .onChange(of: model.rawEntries) {
                        guard showRawData else { return }
                        if restoreHistoryPosition(using: proxy) { return }
                        Task { @MainActor in followLatest(using: proxy) }
                    }
                    .onChange(of: pendingScrollID) { _, id in
                        guard let id else { return }
                        proxy.scrollTo(id, anchor: .center)
                        pendingScrollID = nil
                    }
                    if !followsTail {
                        Button("Jump to latest") {
                            followsTail = true
                            if let id = lastVisibleID { proxy.scrollTo(id, anchor: .bottom) }
                        }
                        .padding(6)
                    }
                }
            }
            .frame(maxHeight: .infinity)
            Divider()
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
            .padding(12)
        }
        .focusable()
        .focusEffectDisabled()
        .focused($transcriptFocused)
        // Scene-wide, so the menu finds the transcript without it holding keyboard focus.
        .background {
            if isVisible {
                Color.clear.focusedSceneValue(\.paneFindActions, PaneFindActions(
                    terminalFocused: panes.focusedKey != nil,
                    open: openFind, next: { stepFind(1) }, previous: { stepFind(-1) }
                ))
            }
        }
        .task(id: searchRequest) {
            await updateSearch()
        }
    }

    private var findBar: some View {
        HStack(spacing: 8) {
            TextField("Find in loaded messages", text: $findQuery)
                .textFieldStyle(.roundedBorder)
                .focused($findFieldFocused)
                .onSubmit { stepFind(1) }
                .onKeyPress(.return, phases: .down) { press in
                    guard press.modifiers.contains(.shift) else { return .ignored }
                    stepFind(-1)
                    return .handled
                }
                .onKeyPress(.escape) {
                    closeFind()
                    return .handled
                }
            Text(findCountText)
                .font(.caption.monospacedDigit())
                .foregroundStyle(.secondary)
                .frame(minWidth: 48)
            Button { stepFind(-1) } label: { Image(systemName: "chevron.up") }
                .help("Previous match")
                .disabled(isSearching || findMatches.isEmpty)
            Button { stepFind(1) } label: { Image(systemName: "chevron.down") }
                .help("Next match")
                .disabled(isSearching || findMatches.isEmpty)
            Button(action: closeFind) { Image(systemName: "xmark") }
                .help("Close find")
        }
        .buttonStyle(.borderless)
        .padding(.horizontal, 12)
        .padding(.vertical, 8)
        .background(.bar)
    }

    private var visibleRows: [TranscriptRow] {
        model.rows.filter { showHiddenRows || !$0.isHiddenByDefault }
    }

    private var rawSessionJSON: String {
        TranscriptDebugData.sessionJSON(session: row.session, agents: agents)
    }

    private struct SearchRequest: Hashable {
        var query: String
        var revision: Int
        var raw: Bool
        var hidden: Bool
        var active: Bool
        var agents: [SwarmAgent]
    }

    private var searchRequest: SearchRequest {
        SearchRequest(
            query: findQuery, revision: model.transcriptRevision, raw: showRawData,
            hidden: showHiddenRows, active: findPresented && isActive,
            agents: showRawData ? agents : []
        )
    }

    private func updateSearch() async {
        let request = searchRequest
        guard request.active, !request.query.isEmpty else {
            findMatches = []
            findMatchSet = []
            findMatchID = nil
            isSearching = false
            return
        }
        isSearching = true
        let snapshot = model.snapshot
        let session = row.session
        let previous = findMatches
        let selected = findMatchID
        let search = Task.detached { () throws -> [String] in
            var items: [PaneSearchItem] = []
            if request.raw {
                items.append(PaneSearchItem(
                    id: "raw-session",
                    text: TranscriptDebugData.sessionJSON(session: session, agents: request.agents)
                ))
            }
            if case .rows(let rows, let raw) = snapshot {
                if request.raw {
                    for entry in raw {
                        try Task.checkCancellation()
                        items.append(PaneSearchItem(id: entry.id, text: entry.displayText))
                    }
                } else {
                    for row in rows where request.hidden || !row.isHiddenByDefault {
                        try Task.checkCancellation()
                        let diff = row.tool?.diffs.map {
                            ([$0.path] + $0.hunks.flatMap(\.lines)).joined(separator: "\n")
                        }.joined(separator: "\n")
                        let text = [row.text, row.detail, row.tool?.command, row.tool?.output, row.tool?.path, diff]
                            .compactMap { $0 }.joined(separator: "\n")
                        items.append(PaneSearchItem(id: row.eventID, text: text))
                    }
                }
            }
            try Task.checkCancellation()
            return PaneSearch.matches(query: request.query, in: items)
        }
        do {
            let matches = try await withTaskCancellationHandler {
                try await search.value
            } onCancel: {
                search.cancel()
            }
            guard !Task.isCancelled, request == searchRequest else { return }
            findMatches = matches
            findMatchSet = Set(matches)
            let index = PaneSearch.reconcile(
                current: selected.flatMap { previous.firstIndex(of: $0) },
                previousMatches: previous, newMatches: matches
            )
            findMatchID = index.map { matches[$0] }
            if selected != findMatchID { pendingScrollID = findMatchID }
            isSearching = false
        } catch {
            if !Task.isCancelled, request == searchRequest { isSearching = false }
        }
    }

    private var currentMatchID: String? {
        guard let findMatchID, findMatchSet.contains(findMatchID) else { return nil }
        return findMatchID
    }

    private var findIndex: Int? {
        findMatchID.flatMap { findMatches.firstIndex(of: $0) }
    }

    private var findCountText: String {
        if isSearching { return "Searching…" }
        guard let findIndex, !findMatches.isEmpty else { return "0 of 0" }
        return "\(findIndex + 1) of \(findMatches.count)"
    }

    private var lastVisibleID: String? {
        showRawData ? model.rawEntries.last?.id : visibleRows.last?.eventID
    }

    private var rawSessionBlock: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text("Session and agents").font(.caption).foregroundStyle(.secondary)
            TranscriptBoundedTextView(text: rawSessionJSON)
                .environment(\.transcriptSearchQuery, currentMatchID == "raw-session" ? findQuery : "")
        }
        .id("raw-session")
        .padding(8)
        .background(matchBackground("raw-session"))
    }

    private func rawEntry(_ entry: RawTranscriptEntry) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            Text("[\(entry.index)] \(entry.rowKind)")
                .font(.caption.monospaced())
                .foregroundStyle(.secondary)
            TranscriptBoundedTextView(text: entry.displayText)
                .environment(\.transcriptSearchQuery, currentMatchID == entry.id ? findQuery : "")
        }
        .id(entry.id)
        .padding(8)
        .background(matchBackground(entry.id))
    }

    private func matchBackground(_ id: String) -> some ShapeStyle {
        guard findPresented else { return Color.clear }
        if currentMatchID == id { return Color.accentColor.opacity(0.28) }
        if findMatchSet.contains(id) { return Color.yellow.opacity(0.14) }
        return Color.clear
    }

    private func openFind() {
        guard KeyRouting.route(focus: .transcript, key: .commandF) == .openFind else { return }
        findPresented = true
        resetFindSelection()
        Task { @MainActor in findFieldFocused = true }
    }

    private func closeFind() {
        findPresented = false
        findFieldFocused = false
        transcriptFocused = true
    }

    private func stepFind(_ delta: Int) {
        guard !isSearching else { return }
        let key: RoutedKey = delta < 0 ? .shiftCommandG : .commandG
        let expected: KeyRoute = delta < 0 ? .findPrevious : .findNext
        guard KeyRouting.route(focus: .transcript, key: key) == expected else { return }
        let index = PaneSearch.step(current: findIndex, count: findMatches.count, delta: delta)
        findMatchID = index.map { findMatches[$0] }
        pendingScrollID = currentMatchID
    }

    private func resetFindSelection() {
        findMatchID = findMatches.first
        pendingScrollID = findMatches.first
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
                    if let agent = byID[cell.id]?.agent { _ = panes.terminal(session: session, agent: agent) }
                }
            case .notice(let reason)?:
                ContentUnavailableView(reason, systemImage: "terminal")
            case nil:
                EmptyView()
            }
        }
    }
}

private struct TranscriptRowView: View {
    let row: TranscriptRow
    let chair: String?
    var revealForSearch = false
    @State private var copying = false
    @State private var detailExpanded = false

    var body: some View {
        Group {
            if let activity = row.tool {
                TranscriptToolCard(title: row.text, activity: activity, revealForSearch: revealForSearch)
                    .frame(maxWidth: 880, alignment: .leading)
            } else if row.kind == .user {
                rowBody
                    .padding(14)
                    .frame(maxWidth: 720, alignment: .leading)
                    .background { RoundedRectangle(cornerRadius: 10).fill(.quaternary) }
            } else if row.kind == .assistant {
                rowBody.frame(maxWidth: 880, alignment: .leading).padding(.vertical, 6)
            } else {
                rowBody
            }
        }
        .id(row.eventID)
        .onChange(of: revealForSearch, initial: true) { _, reveal in
            if reveal { detailExpanded = true }
        }
    }

    private var rowBody: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(row.label(chair: chair))
                .font(labelFont)
                .foregroundStyle(.secondary)
            switch row.kind {
            case .diff:
                if let diff = row.diff { TranscriptDiffView(diff: diff, revealForSearch: revealForSearch) }
            case .thought:
                DisclosureGroup("Show reasoning", isExpanded: $detailExpanded) {
                    TranscriptBoundedTextView(text: row.text)
                }
            case .toolResult:
                TranscriptOutputView(text: row.text,
                                     title: row.toolStatus == .failed ? "Failed tool result" : "Tool result",
                                     revealAll: revealForSearch)
            case .toolUse:
                DisclosureGroup(isExpanded: $detailExpanded) {
                    TranscriptBoundedTextView(text: row.detail ?? "")
                } label: {
                    Text(verbatim: row.text).font(.system(.body, design: .monospaced))
                        .lineLimit(1)
                }
            case .system where row.text.split(separator: "\n", maxSplits: 3).count > 3:
                DisclosureGroup(isExpanded: $detailExpanded) {
                    TranscriptBoundedTextView(text: row.text)
                } label: {
                    Text(verbatim: String(row.text.prefix(while: { !$0.isNewline })))
                        .font(.system(.body, design: .monospaced))
                        .lineLimit(1)
                }
            case .error:
                Text(verbatim: row.text).foregroundStyle(.red)
            case .user, .assistant:
                TranscriptMessageView(text: row.text)
                Button(copying ? "Copying…" : "Copy message", systemImage: "doc.on.doc") {
                    copying = true
                    Task {
                        try? await Task.sleep(for: .milliseconds(30))
                        NSPasteboard.general.clearContents()
                        NSPasteboard.general.setString(row.text, forType: .string)
                        copying = false
                    }
                }
                .disabled(copying)
                .font(.caption).foregroundStyle(.secondary).buttonStyle(.borderless)
                .padding(.top, 4)
            case .result where row.endsTurn:
                Label(row.text == "aborted" ? "Turn interrupted" : "Turn finished",
                      systemImage: row.text == "aborted" ? "stop.circle" : "checkmark.circle")
                    .font(.caption).foregroundStyle(.secondary)
            default:
                Text(verbatim: row.text)
            }
        }
    }

    private var labelFont: Font {
        switch row.kind {
        case .thought, .toolUse, .toolResult, .result, .system:
            .system(.callout, design: .monospaced)
        default:
            .caption
        }
    }
}
