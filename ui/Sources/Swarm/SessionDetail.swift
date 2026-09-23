import AppKit
import Observation
import SwiftUI
import SwarmCore

@MainActor @Observable
final class SessionDetailModel {
    private let transcript = SwarmChairTranscript()
    private let bus = SwarmCLIBus()

    var snapshot: ChairTranscriptSnapshot = .waiting
    var draft = ""
    var sendError: String?
    private var sendState = ComposerSendState()

    var isSending: Bool { sendState.isSending }

    var rows: [TranscriptRow] {
        if case .rows(let rows, _) = snapshot { return rows }
        return []
    }

    var rawEntries: [RawTranscriptEntry] {
        if case .rows(_, let raw) = snapshot { return raw }
        return []
    }

    func poll(session: SwarmSession, chairProvider: String?) async {
        while !Task.isCancelled {
            snapshot = await transcript.poll(session: session, chairProvider: chairProvider)
            try? await Task.sleep(for: .seconds(1))
        }
    }

    func send(session: SwarmSession) async -> Bool {
        guard let text = sendState.begin(draft) else { return false }
        do {
            try await bus.type(text, to: SwarmPanePolicy.chair, in: session)
            draft = sendState.finish(currentDraft: draft, succeeded: true)
            sendError = nil
            return true
        } catch {
            draft = sendState.finish(currentDraft: draft, succeeded: false)
            sendError = String(describing: error)
            return false
        }
    }
}

struct SessionDetailView: View {
    let row: SwarmProjectSession
    let title: String
    let agents: [SwarmAgent]
    let panes: AgentPaneStore

    @State private var model = SessionDetailModel()
    @State private var followsTail = true
    @State private var atBottom = true
    @State private var userScrolling = false
    @State private var showHiddenRows = false
    @AppStorage("showRawData") private var showRawData = false
    @State private var findPresented = false
    @State private var findQuery = ""
    @State private var findMatchID: String?
    @State private var pendingScrollID: String?
    @FocusState private var composerFocused: Bool
    @FocusState private var findFieldFocused: Bool
    @FocusState private var transcriptFocused: Bool

    var body: some View {
        HStack(spacing: 0) {
            transcriptColumn
                .frame(minWidth: 320, maxWidth: .infinity)
            if SwarmPanePolicy.hasLiveChildAgents(session: row.session, agents: agents) {
                paneColumn
                    .frame(minWidth: 360, maxWidth: .infinity)
                    .id(row.session.id.rawValue)
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .navigationTitle(title)
        .task(id: row.id.rawValue + (row.session.chairLog ?? "") + (chairProvider ?? "")) {
            await model.poll(session: row.session, chairProvider: chairProvider)
        }
        .onAppear { transcriptFocused = true }
        .onChange(of: panes.focusedKey) { _, key in
            guard key != nil else { return }
            transcriptFocused = false
            composerFocused = false
            findFieldFocused = false
        }
    }

    private var chairProvider: String? {
        agents.first { $0.id == SwarmPanePolicy.chair }?.provider
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
                            switch model.snapshot {
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
                                            chair: self.row.provider ?? chairProvider
                                        )
                                        .padding(3)
                                        .background(matchBackground(transcriptRow.eventID))
                                    }
                                }
                            }
                        }
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .padding()
                    }
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
                    .onScrollPhaseChange { oldPhase, phase in
                        if oldPhase == .interacting || oldPhase == .decelerating {
                            followsTail = TranscriptTail.follows(
                                current: followsTail, atBottom: atBottom, userScrolled: true
                            )
                        }
                        userScrolling = phase == .interacting || phase == .decelerating
                    }
                    .onChange(of: model.rows) {
                        if followsTail, !showRawData, let last = visibleRows.last {
                            Task { @MainActor in proxy.scrollTo(last.eventID, anchor: .bottom) }
                        }
                    }
                    .onChange(of: model.rawEntries) {
                        if followsTail, showRawData, let last = model.rawEntries.last {
                            Task { @MainActor in proxy.scrollTo(last.id, anchor: .bottom) }
                        }
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
            VStack(alignment: .leading, spacing: 4) {
                HStack(alignment: .bottom) {
                    TextField("Message the chair", text: $model.draft, axis: .vertical)
                        .lineLimit(1...8)
                        .textFieldStyle(.plain)
                        .font(.body)
                        .focused($composerFocused)
                        .simultaneousGesture(TapGesture().onEnded {
                            composerFocused = true
                            panes.clearFocus()
                        })
                        .onKeyPress(.return, phases: .down) { press in
                            guard press.modifiers.contains(.shift),
                                  KeyRouting.route(focus: .composer, key: .shiftReturn) == .insertNewline
                            else { return .ignored }
                            model.draft.append("\n")
                            return .handled
                        }
                        .onKeyPress(.escape) {
                            guard KeyRouting.route(focus: .composer, key: .escape) == .clearComposer else {
                                return .ignored
                            }
                            model.draft = ""
                            return .handled
                        }
                        .onSubmit {
                            if KeyRouting.route(focus: .composer, key: .return) == .sendComposer {
                                send()
                            }
                        }
                    Button(action: send) {
                        Group {
                            if Composer.outgoing(model.draft) == nil {
                                Image(systemName: "arrow.up.circle.fill").foregroundStyle(.tertiary)
                            } else {
                                Image(systemName: "arrow.up.circle.fill").foregroundStyle(Color.accentColor)
                            }
                        }
                        .font(.title2)
                    }
                    .buttonStyle(.plain)
                    .disabled(Composer.outgoing(model.draft) == nil || model.isSending)
                    .accessibilityLabel("Send")
                }
                .padding(.horizontal, 10)
                .padding(.vertical, 8)
                .background {
                    RoundedRectangle(cornerRadius: 10).fill(.background)
                }
                .overlay {
                    RoundedRectangle(cornerRadius: 10).stroke(.separator, lineWidth: 1)
                }
                if let error = model.sendError {
                    Text(verbatim: error)
                        .foregroundStyle(.red)
                        .font(.caption)
                }
            }
            .padding(12)
        }
        .focusable()
        .focused($transcriptFocused)
        .focusedValue(\.paneFindActions, PaneFindActions(
            open: openFind, next: { stepFind(1) }, previous: { stepFind(-1) }
        ))
        .onChange(of: findMatches) { previousMatches, newMatches in
            let index = PaneSearch.reconcile(
                current: findMatchID.flatMap { previousMatches.firstIndex(of: $0) },
                previousMatches: previousMatches,
                newMatches: newMatches
            )
            findMatchID = index.map { newMatches[$0] }
        }
        .onChange(of: findQuery) {
            resetFindSelection()
        }
        .onChange(of: showRawData) {
            resetFindSelection()
        }
    }

    private var findBar: some View {
        HStack(spacing: 8) {
            TextField("Find", text: $findQuery)
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
                .disabled(findMatches.isEmpty)
            Button { stepFind(1) } label: { Image(systemName: "chevron.down") }
                .help("Next match")
                .disabled(findMatches.isEmpty)
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

    private var searchItems: [PaneSearchItem] {
        if showRawData {
            return [PaneSearchItem(id: "raw-session", text: rawSessionJSON)]
                + model.rawEntries.map { PaneSearchItem(id: $0.id, text: $0.displayText) }
        }
        return visibleRows.map {
            PaneSearchItem(id: $0.eventID, text: [$0.text, $0.detail].compactMap { $0 }.joined(separator: "\n"))
        }
    }

    private var findMatches: [String] {
        PaneSearch.matches(query: findQuery, in: searchItems)
    }

    private var currentMatchID: String? {
        guard let findMatchID, findMatches.contains(findMatchID) else { return nil }
        return findMatchID
    }

    private var findIndex: Int? {
        findMatchID.flatMap { findMatches.firstIndex(of: $0) }
    }

    private var findCountText: String {
        guard let findIndex, !findMatches.isEmpty else { return "0 of 0" }
        return "\(findIndex + 1) of \(findMatches.count)"
    }

    private var lastVisibleID: String? {
        showRawData ? model.rawEntries.last?.id : visibleRows.last?.eventID
    }

    private var rawSessionBlock: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text("Session and agents").font(.caption).foregroundStyle(.secondary)
            Text(verbatim: rawSessionJSON)
                .font(.system(.body, design: .monospaced))
                .textSelection(.enabled)
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
            Text(verbatim: entry.displayText)
                .font(.system(.body, design: .monospaced))
                .textSelection(.enabled)
        }
        .id(entry.id)
        .padding(8)
        .background(matchBackground(entry.id))
    }

    private func matchBackground(_ id: String) -> some ShapeStyle {
        if currentMatchID == id { return Color.accentColor.opacity(0.28) }
        if findMatches.contains(id) { return Color.yellow.opacity(0.14) }
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

    private func send() {
        Task {
            if await model.send(session: row.session) { composerFocused = true }
        }
    }

    private var paneColumn: some View {
        let cells = SwarmPanePolicy.cells(session: row.session, agents: agents)
        return Group {
            ScrollView {
                LazyVGrid(columns: [GridItem(.flexible()), GridItem(.flexible())], spacing: 8) {
                    ForEach(cells) { cell in
                        AgentCellView(session: row.session, cell: cell, panes: panes)
                            .frame(height: 320)
                    }
                }
                .padding(8)
            }
        }
    }
}

private struct AgentCellView: View {
    let session: SwarmSession
    let cell: SwarmAgentCell
    let panes: AgentPaneStore

    var body: some View {
        VStack(spacing: 4) {
            Text("\(cell.agent.id.rawValue) · \(cell.agent.role) · \(cell.agent.provider ?? "unknown") · \(cell.agent.alive == false ? "ended" : "live")")
                .font(.caption)
                .lineLimit(1)
                .frame(maxWidth: .infinity, alignment: .leading)
            Group {
                switch cell.kind {
                case .attach:
                    AgentTerminalView(session: session, agent: cell.agent, store: panes)
                case .notice(let reason):
                    ContentUnavailableView(reason, systemImage: "terminal")
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
        .padding(6)
        .background(.background)
        .overlay { RoundedRectangle(cornerRadius: 4).stroke(.separator) }
    }
}

private struct TranscriptRowView: View {
    let row: TranscriptRow
    let chair: String?

    var body: some View {
        Group {
            if row.kind == .user {
                rowBody
                    .padding(10)
                    .frame(maxWidth: 720, alignment: .leading)
                    .background { RoundedRectangle(cornerRadius: 10).fill(.quaternary) }
            } else if row.kind == .assistant {
                rowBody.frame(maxWidth: 720, alignment: .leading)
            } else {
                rowBody
            }
        }
        .id(row.eventID)
    }

    private var rowBody: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(row.label(chair: chair))
                .font(labelFont)
                .foregroundStyle(.secondary)
            switch row.kind {
            case .thought, .toolResult:
                DisclosureGroup("Show text") {
                    Text(verbatim: row.text).font(.system(.body, design: .monospaced))
                }
            case .toolUse:
                DisclosureGroup {
                    Text(verbatim: row.detail ?? "").font(.system(.body, design: .monospaced))
                } label: {
                    Text(verbatim: row.text).font(.system(.body, design: .monospaced))
                        .lineLimit(1)
                }
            case .system where row.text.components(separatedBy: .newlines).count > 3:
                DisclosureGroup {
                    Text(verbatim: row.text).font(.system(.body, design: .monospaced))
                } label: {
                    Text(verbatim: String(row.text.prefix(while: { !$0.isNewline })))
                        .font(.system(.body, design: .monospaced))
                        .lineLimit(1)
                }
            case .error:
                Text(verbatim: row.text).foregroundStyle(.red)
            case .user, .assistant:
                Text(verbatim: row.text).font(.body)
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
