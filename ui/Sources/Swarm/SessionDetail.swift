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

    func send(session: SwarmSession) async {
        guard let text = SwarmPanePolicy.typedText(draft) else { return }
        do {
            try await bus.type(text, to: SwarmPanePolicy.chair, in: session)
            draft = ""
            sendError = nil
        } catch {
            sendError = String(describing: error)
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
        HSplitView {
            transcriptColumn
                .frame(minWidth: 320)
            paneColumn
                .frame(minWidth: 640)
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
                                Text("The chair has not written its log yet")
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
                                ForEach(rows.filter { showHiddenRows || !$0.isHiddenByDefault }) { row in
                                    TranscriptRowView(row: row)
                                }
                            }
                        }
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .padding()
                    }
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
            TextField("Type to chair", text: $model.draft)
                .textFieldStyle(.roundedBorder)
                .focused($composerFocused)
                .simultaneousGesture(TapGesture().onEnded {
                    composerFocused = true
                    panes.clearFocus()
                })
                .onKeyPress(.escape) {
                    guard KeyRouting.route(focus: .composer, key: .escape) == .clearComposer else {
                        return .ignored
                    }
                    model.draft = ""
                    return .handled
                }
                .onSubmit {
                    if KeyRouting.route(focus: .composer, key: .return) == .sendComposer {
                        Task { await model.send(session: row.session) }
                    }
                }
                .padding(8)
            if let error = model.sendError {
                Text(verbatim: error).foregroundStyle(.red).padding(.horizontal, 8)
            }
        }
    }

    private var paneColumn: some View {
        let cells = SwarmPanePolicy.cells(session: row.session, agents: agents)
        return Group {
            if cells.isEmpty {
                Text("No live child agents")
                    .foregroundStyle(.secondary)
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else {
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

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(row.kind.rawValue.capitalized).font(.caption).foregroundStyle(.secondary)
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
            default:
                Text(verbatim: row.text)
            }
        }
        .id(row.eventID)
    }
}
