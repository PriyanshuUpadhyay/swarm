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

    var rows: [TranscriptRow] {
        if case .rows(let rows) = snapshot { return rows }
        return []
    }

    func poll(session: SwarmSession, chairProvider: String?) async {
        while !Task.isCancelled {
            snapshot = await transcript.poll(session: session, chairProvider: chairProvider)
            try? await Task.sleep(for: .seconds(1))
        }
    }

    func send(session: SwarmSession) async -> Bool {
        guard let text = Composer.outgoing(draft) else { return false }
        do {
            try await bus.type(text, to: SwarmPanePolicy.chair, in: session)
            draft = ""
            sendError = nil
            return true
        } catch {
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
    @FocusState private var composerFocused: Bool

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
    }

    private var chairProvider: String? {
        agents.first { $0.id == SwarmPanePolicy.chair }?.provider
    }

    private var transcriptColumn: some View {
        VStack(spacing: 0) {
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
                            case .rows(let rows):
                                let hidden = rows.filter(\.isHiddenByDefault).count
                                if hidden > 0 {
                                    Button(showHiddenRows ? "Hide \(hidden) hidden rows" : "Show \(hidden) hidden rows") {
                                        showHiddenRows.toggle()
                                    }
                                }
                                ForEach(rows.filter { showHiddenRows || !$0.isHiddenByDefault }) { transcriptRow in
                                    TranscriptRowView(
                                        row: transcriptRow, chair: self.row.provider ?? chairProvider
                                    )
                                }
                            }
                        }
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .padding()
                    }
                    .contentMargins(.top, 8, for: .scrollContent)
                    .frame(maxHeight: .infinity)
                    .simultaneousGesture(TapGesture().onEnded {
                        NSApp.keyWindow?.makeFirstResponder(nil)
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
                        if followsTail,
                           let last = model.rows.last(where: { showHiddenRows || !$0.isHiddenByDefault }) {
                            Task { @MainActor in proxy.scrollTo(last.eventID, anchor: .bottom) }
                        }
                    }
                    if !followsTail {
                        Button("Jump to latest") {
                            followsTail = true
                            if let last = model.rows.last(where: { showHiddenRows || !$0.isHiddenByDefault }) {
                                proxy.scrollTo(last.eventID, anchor: .bottom)
                            }
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
                    .disabled(Composer.outgoing(model.draft) == nil)
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
