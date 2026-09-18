import SwiftUI
import SwarmCore

/// A discovered swarm session, with the chair chat and each agent's bus history.
struct SwarmSessionView: View {
    var item: SwarmProjectSession
    @State private var reader: SwarmSessionReaderModel
    @State private var showsTerminal = false
    @State private var localModel: WorkspaceModel?
    @Environment(AppModel.self) private var app

    init(item: SwarmProjectSession, bus: any SwarmBus) {
        self.item = item
        _reader = State(initialValue: SwarmSessionReaderModel(item: item, bus: bus))
    }

    var body: some View {
        VStack(spacing: 0) {
            HStack(alignment: .firstTextBaseline, spacing: Metrics.spacing) {
                Text(item.title)
                    .font(Typo.heading)
                    .lineLimit(1)
                Spacer()
                Text(item.sessions.count == 1
                    ? "Session \(item.id.rawValue)"
                    : "\(item.sessions.count) sessions")
                    .font(Typo.caption)
                    .foregroundStyle(Palette.textTertiary)
                if let localSession, TerminalSessionStore.shared.interactiveState(
                    for: localSession.id
                ) == .stopped {
                    Button("Resume") {
                        Task { await localModel?.resumeCLI(localSession) }
                    }
                    .buttonStyle(.borderedProminent)
                }
                if item.session.adapter == "tmux", localTab != nil {
                    Button(showsTerminal ? "Show chat" : "Show terminal") {
                        showsTerminal.toggle()
                    }
                    .buttonStyle(.bordered)
                }
            }
            .padding(.horizontal, Metrics.pane)
            .padding(.vertical, Metrics.spacing)

            Divider()

            if showsTerminal, let localModel, let localTab {
                ToolPaneView(
                    model: localModel, tab: localTab,
                    splitColumn: { _, _ in }, paneMenu: nil
                )
            } else {
                HSplitView {
                    SwarmSessionChat(reader: reader, directory: item.session.cwd)
                        .frame(minWidth: 420)
                    SwarmSessionAgentsView(reader: reader)
                        .frame(minWidth: 260, idealWidth: 320, maxWidth: 420)
                }
            }
        }
        .background(Palette.surface)
        .task {
            if let workspaceID = item.workspaceID,
               let workspace = app.workspaces.first(where: { $0.id == workspaceID }) {
                let model = app.model(for: workspace)
                await model.reloadSessions()
                localModel = model
            }
            await reader.follow()
        }
    }

    private var localSession: Session? {
        guard let id = item.localSessionID else { return nil }
        return localModel?.sessions.first { $0.id == id }
    }

    private var localTab: CenterTab? {
        guard let id = item.localSessionID, let workspaceID = item.workspaceID else { return nil }
        return CenterTabStore.shared.terminal(for: id, in: workspaceID)
    }
}

private struct SwarmSessionChat: View {
    var reader: SwarmSessionReaderModel
    var directory: String

    @State private var position = ScrollPosition(edge: .bottom)
    @State private var followsEnd = true
    @State private var bubbleWidth = TranscriptBubbleWidth()
    @State private var hoverHost = TranscriptHoverHost()
    /// Which rows the lazy stack is handed. A chair log runs to thousands of rows, and a lazy stack
    /// walks every child it holds on each layout pass, so the pane opens on the newest rows and
    /// takes another chunk when the reader scrolls back towards the top. See `TranscriptWindow`.
    @State private var window = TranscriptWindow(start: 0, end: 0)
    @State private var isGrowing = false

    private var textSize: ChatTextSize { ColourThemePreference.shared.chatTextSize }
    private var chatFontID: String { ColourThemePreference.shared.chatFont }
    private var lineHeight: ChatLineHeight { ColourThemePreference.shared.chatLineHeight }

    /// One chunk of older rows, once per scroll that reaches the top of the window.
    ///
    /// The live end is checked because a window shorter than the pane is at its top and its bottom
    /// at once, and growing that one would put rows above a reader who is reading the newest one.
    /// `isGrowing` is checked because a scroll asks this on every frame it is near the top.
    private func growWindow() {
        guard !followsEnd, window.canGrowUp, !isGrowing else { return }
        isGrowing = true
        window = window.grownUp()
        Task { @MainActor in
            try? await Task.sleep(for: .milliseconds(120))
            isGrowing = false
        }
    }

    var body: some View {
        VStack(spacing: 0) {
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 0) {
                    Color.clear
                        .frame(height: TranscriptLayout.topSpace)
                        .accessibilityHidden(true)

                    if let failure = reader.chatFailure {
                        Text(failure)
                            .font(Typo.body)
                            .foregroundStyle(Palette.textSecondary)
                            .subagentReadingColumn()
                    } else {
                        let drawn = window.clamped(rowCount: reader.rows.count)
                        SubagentConversationView(
                            rows: Array(reader.rows[drawn.start..<drawn.end]),
                            prompt: "",
                            home: TranscriptHome(workspaceID: nil, worktree: directory),
                            droppedRows: reader.droppedRows + drawn.start,
                            isRunning: false
                        )
                    }
                }
                .padding(.bottom, Metrics.pane)
            }
            .scrollPosition($position)
            .defaultScrollAnchor(.bottom, for: .initialOffset)
            // Older rows go in ABOVE the reader, so the anchor is what keeps the rows they are
            // reading under their eyes while the window grows.
            .defaultScrollAnchor(.bottom, for: .sizeChanges)
            .onScrollGeometryChange(for: Bool.self) { geometry in
                ScrollEnd.isAtEnd(
                    contentHeight: geometry.contentSize.height,
                    viewportHeight: geometry.containerSize.height,
                    offset: geometry.contentOffset.y
                )
            } action: { _, atEnd in
                followsEnd = atEnd
            }
            .onScrollGeometryChange(for: Bool.self) { geometry in
                ScrollEnd.isNearStart(
                    contentHeight: geometry.contentSize.height,
                    viewportHeight: geometry.containerSize.height,
                    offset: geometry.contentOffset.y
                )
            } action: { _, nearStart in
                if nearStart { growWindow() }
            }
            .onGeometryChange(for: CGFloat.self) { proxy in
                TranscriptGeometry.cap(
                    width: proxy.size.width,
                    share: TranscriptListView.bubbleShare,
                    gutter: Metrics.gutter,
                    floor: TranscriptListView.bubbleFloor
                )
            } action: { cap in
                if bubbleWidth.cap != cap { bubbleWidth.cap = cap }
            }
            .onChange(of: reader.rows.count, initial: true) { previous, count in
                window = window.count == 0
                    ? TranscriptWindow.liveEnd(rowCount: count)
                    : window.includingAppendedRows(previousCount: previous, rowCount: count)
                if followsEnd { position.scrollTo(edge: .bottom) }
            }
            .overlay { TranscriptHoverOverlay(host: hoverHost) }

            Divider()

            SwarmSessionInput(
                reader: reader,
                agent: SwarmAgentID("orchestrator"),
                sessionID: nil,
                target: .chair,
                placeholder: "Message chair",
                maxLines: 6
            )
            .padding(Metrics.pane)
        }
        .environment(\.transcriptHoverHost, hoverHost)
        .environment(\.transcriptBubbleWidth, bubbleWidth)
        .environment(\.fontScale, textSize.scale)
        .environment(\.chatFont, ChatFont(rawValue: chatFontID))
        .environment(\.chatLineHeight, lineHeight)
    }
}

private struct SwarmSessionAgentsView: View {
    var reader: SwarmSessionReaderModel

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Text("Agents")
                .font(Typo.label)
                .padding(Metrics.pane)

            Divider()

            ScrollView {
                LazyVStack(alignment: .leading, spacing: Metrics.spacing) {
                    if let failure = reader.agentsFailure {
                        Text(failure)
                            .font(Typo.body)
                            .foregroundStyle(Palette.textSecondary)
                    } else if reader.agents.isEmpty {
                        Text("This session has no agents to show.")
                            .font(Typo.body)
                            .foregroundStyle(Palette.textSecondary)
                    } else {
                        ForEach(reader.agents) { agent in
                            SwarmSessionAgentView(digest: agent, reader: reader)
                        }
                    }
                }
                .padding(Metrics.pane)
            }
        }
        .background(Palette.windowBackground)
    }
}

private struct SwarmSessionAgentView: View {
    var digest: SwarmSessionAgentDigest
    var reader: SwarmSessionReaderModel
    @State private var showsHistory = false

    var body: some View {
        VStack(alignment: .leading, spacing: Metrics.spacingSmall) {
            DisclosureGroup(isExpanded: $showsHistory) {
                VStack(alignment: .leading, spacing: Metrics.spacing) {
                    ForEach(digest.conversation) { row in
                        VStack(alignment: .leading, spacing: 2) {
                            Text(row.kind == "ask" ? "Ask" : "Summary")
                                .font(Typo.micro)
                                .foregroundStyle(Palette.textTertiary)
                            Text(row.body ?? "This message body could not be read.")
                                .font(Typo.body)
                                .textSelection(.enabled)
                        }
                        .frame(maxWidth: .infinity, alignment: .leading)
                    }
                }
                .padding(.top, Metrics.spacingSmall)
            } label: {
                VStack(alignment: .leading, spacing: 3) {
                    HStack {
                        Text(digest.agent.id.rawValue)
                            .font(Typo.label)
                        Spacer()
                        Text(digest.agent.role)
                            .font(Typo.micro)
                            .foregroundStyle(Palette.textTertiary)
                    }
                    Text(digest.latestSummary ?? "No summary yet.")
                        .font(Typo.caption)
                        .foregroundStyle(Palette.textSecondary)
                        .lineLimit(showsHistory ? nil : 4)
                }
            }
            .accessibilityHint("Shows all asks and summaries in order")

            SwarmSessionInput(
                reader: reader,
                agent: digest.agent.id,
                sessionID: digest.sessionID,
                target: .agent,
                placeholder: "Message \(digest.agent.id.rawValue)",
                maxLines: 3
            )
        }
    }
}

private struct SwarmSessionInput: View {
    var reader: SwarmSessionReaderModel
    var agent: SwarmAgentID
    var sessionID: SwarmSessionID?
    var target: SwarmSessionInputTarget
    var placeholder: String
    var maxLines: Int

    @State private var draft = ""
    @State private var caret = 0
    @State private var isFocused = false
    @State private var height = ComposerTextEditor.lineHeight
    @State private var isSending = false

    private var disabledReason: String? {
        reader.disabledReason(for: agent, in: sessionID, target: target)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: Metrics.spacingSmall) {
            HStack(alignment: .bottom, spacing: Metrics.spacing) {
                ComposerTextEditor(
                    text: $draft,
                    caret: $caret,
                    isFocused: $isFocused,
                    maxLines: maxLines,
                    accessibilityLabel: placeholder,
                    onHeightChange: { height = $0 },
                    onKey: handle(key:),
                    onAttach: { _, _ in false }
                )
                .frame(height: max(height, ComposerTextEditor.lineHeight))
                .background(alignment: .topLeading) {
                    if draft.isEmpty {
                        Text(placeholder)
                            .font(Typo.body)
                            .lineLimit(1)
                            .foregroundStyle(Palette.textPlaceholder)
                            .padding(.horizontal, ComposerTextEditor.textInset)
                            .allowsHitTesting(false)
                            .accessibilityHidden(true)
                    }
                }

                Button(action: submit) {
                    Image(systemName: "paperplane.fill")
                }
                .buttonStyle(.plain)
                .accessibilityLabel("Send to \(agent.rawValue)")
                .disabled(
                    isSending
                        || !reader.canSubmit(
                            draft, to: agent, in: sessionID, target: target
                        )
                )
            }
            .composerBox(isFocused: $isFocused)
            .disabled(disabledReason != nil)
            .help("Return sends. Shift-Return starts a new line. Esc interrupts.")

            if let disabledReason {
                Text(disabledReason)
                    .font(Typo.micro)
                    .foregroundStyle(Palette.textSecondary)
            } else if let failure = reader.inputFailure(for: agent, in: sessionID) {
                Text(failure)
                    .font(Typo.micro)
                    .foregroundStyle(.red)
                    .lineLimit(1)
                    .help(failure)
            }
        }
    }

    private func handle(key: ComposerKey) -> Bool {
        switch key {
        case .returnKey, .commandReturn:
            submit()
            return true
        case .escape:
            guard disabledReason == nil else { return true }
            Task { await reader.interrupt(agent, in: sessionID) }
            return true
        case .up, .down, .tab:
            return false
        }
    }

    private func submit() {
        guard !isSending,
              reader.canSubmit(draft, to: agent, in: sessionID, target: target)
        else { return }
        let text = draft
        isSending = true
        Task {
            if await reader.type(text, to: agent, in: sessionID) {
                draft = ""
                caret = 0
            }
            isSending = false
        }
    }
}
