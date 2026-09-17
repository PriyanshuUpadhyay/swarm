import SwiftUI
import BloomCore

struct SwarmAgentTabView: View {
    @Bindable var model: WorkspaceModel
    let agent: SwarmAgentID

    @State private var mode = DisplayMode.both
    @State private var draft = ""

    private var swarm: SwarmAgentWorkspaceModel { model.swarmAgents }

    private var currentAgent: SwarmAgent? { swarm.agents[agent] }

    var body: some View {
        VStack(spacing: 0) {
            header
            Divider()
            content
            if mode != .pane {
                Divider()
                composer
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(Palette.windowBackground)
        .onAppear {
            if draft.isEmpty { draft = swarm.pendingComposer(for: agent) ?? "" }
            updateVisibility()
        }
        .onDisappear { swarm.hide(agent) }
        .onChange(of: mode) { _, _ in updateVisibility() }
    }

    private var header: some View {
        HStack(spacing: Metrics.spacing) {
            VStack(alignment: .leading, spacing: 2) {
                Text(agent.rawValue).font(Typo.body).fontWeight(.semibold)
                Text(currentAgent?.role ?? "Role unavailable")
                    .font(Typo.caption)
                    .foregroundStyle(Palette.textTertiary)
            }

            status
            Spacer()

            Picker("View", selection: $mode) {
                ForEach(DisplayMode.allCases) { choice in
                    Text(choice.title).tag(choice)
                }
            }
            .labelsHidden()
            .pickerStyle(.segmented)
            .frame(width: 210)
        }
        .padding(.horizontal, Metrics.spacingWide)
        .padding(.vertical, Metrics.spacing)
    }

    private var status: some View {
        let value: (String, String) = switch currentAgent {
        case .some(let agent) where agent.pane == nil: ("Ended", "stop.circle")
        case .some(let agent) where agent.alive == true: ("Running", "circle.fill")
        case .some(let agent) where agent.alive == false: ("Ended", "stop.circle")
        case .some: ("Status unknown", "questionmark.circle")
        case nil: ("Loading", "clock")
        }
        return Label(value.0, systemImage: value.1)
            .font(Typo.caption)
            .foregroundStyle(Palette.textSecondary)
    }

    @ViewBuilder
    private var content: some View {
        switch mode {
        case .chat:
            chat
        case .both:
            HSplitView {
                chat.frame(minWidth: 260)
                pane.frame(minWidth: 260)
            }
        case .pane:
            pane
        }
    }

    private var chat: some View {
        VStack(spacing: 0) {
            if let error = swarm.lastError {
                HStack {
                    Text(error)
                        .font(Typo.caption)
                        .foregroundStyle(Palette.negative)
                    Spacer()
                    Button("Dismiss") { swarm.dismissError() }
                        .buttonStyle(.plain)
                }
                .padding(Metrics.spacing)
                .background(Palette.surfaceSunken)
            }

            ScrollViewReader { proxy in
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: Metrics.spacingWide) {
                        ForEach(swarm.rows(for: agent)) { row in
                            chatRow(row).id(row.id)
                        }
                    }
                    .padding(Metrics.spacingWide)
                    .frame(maxWidth: .infinity, alignment: .leading)
                }
                .onChange(of: swarm.messages.count) { _, _ in
                    if let last = swarm.rows(for: agent).last?.id {
                        proxy.scrollTo(last, anchor: .bottom)
                    }
                }
            }
        }
    }

    private func chatRow(_ row: SwarmChatRow) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack(spacing: Metrics.spacingTight) {
                Text(author(row.author)).font(Typo.caption).fontWeight(.semibold)
                if let kind = row.kindLabel {
                    Text(kind)
                        .font(Typo.caption)
                        .foregroundStyle(Palette.textTertiary)
                }
            }
            if let body = row.body {
                MarkdownView(body)
            } else {
                Text("Message unavailable")
                    .font(Typo.body)
                    .italic()
                    .foregroundStyle(Palette.textTertiary)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    @ViewBuilder
    private var pane: some View {
        if let session = swarm.sessionID {
            SwarmAgentPaneView(agent: agent, session: session, isAlive: currentAgent?.alive)
        } else {
            ContentUnavailableView(
                "Swarm session unavailable",
                systemImage: "exclamationmark.triangle",
                description: Text(swarm.lastError ?? "Bloom is loading the workspace's swarm session.")
            )
        }
    }

    private var composer: some View {
        HStack(alignment: .bottom, spacing: Metrics.spacing) {
            TextField("Message \(agent.rawValue)", text: $draft, axis: .vertical)
                .lineLimit(1...6)
            Button("Send", action: send)
                .keyboardShortcut(.return, modifiers: .command)
                .disabled(draft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
        }
        .padding(Metrics.spacingWide)
    }

    private func send() {
        let body = draft
        Task {
            if await swarm.send(body, to: agent) { draft = "" }
        }
    }

    private func updateVisibility() {
        if mode == .pane { swarm.hide(agent) } else { swarm.show(agent) }
    }

    private func author(_ author: SwarmChatRow.Author) -> String {
        switch author {
        case .you: "You"
        case .agent(let id): id.rawValue
        }
    }
}

private enum DisplayMode: String, CaseIterable, Identifiable {
    case chat
    case both
    case pane

    var id: String { rawValue }

    var title: String {
        switch self {
        case .chat: "Chat"
        case .both: "Both"
        case .pane: "Pane"
        }
    }
}
