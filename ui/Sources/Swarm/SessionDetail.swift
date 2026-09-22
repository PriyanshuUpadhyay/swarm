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
    let agents: [SwarmAgent]
    let panes: AgentPaneStore

    @State private var model = SessionDetailModel()
    @State private var agentID = SwarmPanePolicy.chair
    @State private var followsTail = true

    var body: some View {
        HSplitView {
            transcriptColumn
                .frame(minWidth: 320)
            paneColumn
                .frame(minWidth: 320)
        }
        .navigationTitle(row.id.rawValue)
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
                            ForEach(rows) { row in TranscriptRowView(row: row) }
                        }
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding()
                }
                .simultaneousGesture(TapGesture().onEnded {
                    NSApp.keyWindow?.makeFirstResponder(nil)
                    panes.clearFocus()
                })
                .onScrollGeometryChange(for: Bool.self) { geometry in
                    geometry.contentOffset.y + geometry.containerSize.height
                        >= geometry.contentSize.height - 32
                } action: { _, atBottom in
                    followsTail = atBottom
                }
                .onChange(of: model.rows) {
                    if followsTail, let last = model.rows.last {
                        proxy.scrollTo(last.eventID, anchor: .bottom)
                    }
                }
            }
            Divider()
            TextField("Type to chair", text: $model.draft)
                .textFieldStyle(.roundedBorder)
                .simultaneousGesture(TapGesture().onEnded { panes.clearFocus() })
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
        VStack(spacing: 0) {
            Picker("Agent", selection: $agentID) {
                ForEach(agents) { agent in
                    Text(agent.id.rawValue).tag(agent.id).disabled(agent.pane == nil)
                }
            }
            .pickerStyle(.segmented)
            .padding(8)
            .simultaneousGesture(TapGesture().onEnded { panes.clearFocus() })
            Divider()
            if let agent = SwarmPanePolicy.selectedAgent(in: agents, preferred: agentID) {
                if let reason = SwarmPanePolicy.unavailableReason(session: row.session, agent: agent) {
                    ContentUnavailableView(reason, systemImage: "terminal")
                } else {
                    AgentTerminalView(session: row.session, agent: agent, store: panes)
                        .id(panes.key(session: row.session, agent: agent))
                }
            } else {
                ContentUnavailableView("No agents", systemImage: "terminal")
            }
        }
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
                Text(verbatim: row.text).font(.system(.body, design: .monospaced))
                    .lineLimit(1)
            case .error:
                Text(verbatim: row.text).foregroundStyle(.red)
            default:
                Text(verbatim: row.text)
            }
        }
        .id(row.eventID)
    }
}
