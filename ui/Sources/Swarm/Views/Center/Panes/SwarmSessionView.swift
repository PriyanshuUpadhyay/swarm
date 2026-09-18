import SwiftUI
import SwarmCore

/// A discovered swarm session, with the chair chat and each agent's bus history.
struct SwarmSessionView: View {
    var item: SwarmProjectSession
    @State private var reader: SwarmSessionReaderModel
    @State private var showsTerminal = false
    @State private var localModel: WorkspaceModel?
    /// Built in `.task` rather than in `init`, because it needs the `AppModel` from the environment
    /// and an environment value does not exist yet while an initialiser runs.
    @State private var transcript: TranscriptModel?
    @Environment(AppModel.self) private var app

    init(item: SwarmProjectSession, bus: any SwarmBus) {
        self.item = item
        _reader = State(initialValue: SwarmSessionReaderModel(item: item, bus: bus))
    }

    /// The identity this conversation has inside the app.
    ///
    /// Prefixed, because a swarm session id is a small integer the bus hands out and a `SessionID`
    /// is the key of the store's own table. Two namespaces meeting on "10" would be one chat
    /// reading another's rows.
    private var chatSessionID: SessionID {
        SessionID("swarm-" + item.session.id.rawValue)
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
                    Group {
                        if let transcript {
                            SwarmSessionChat(reader: reader, transcript: transcript)
                        }
                    }
                    .frame(minWidth: 420)
                    SwarmSessionAgentsView(reader: reader)
                        .frame(minWidth: 260, idealWidth: 320, maxWidth: 420)
                        .markdownLinkActions(TranscriptLink.actions(for: localModel))
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
        // Its own task, because `reader.follow` above never returns and the chair log has to be
        // followed at the same time as the bus.
        .task {
            let model = transcript ?? TranscriptModel(
                swarmSession: Session(id: chatSessionID, workspaceID: nil, title: item.title),
                chairLog: ChairTranscriptOutput.reader(
                    path: item.session.chairLog, sessionID: chatSessionID
                ),
                directory: item.session.cwd,
                app: app
            )
            if transcript == nil { transcript = model }
            await model.follow()
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

/// The chair's conversation, drawn by the transcript every other chat is drawn by.
///
/// **This used to be a second chat**, a `ScrollView` over a `LazyVStack` over
/// `SubagentConversationView`, with its own window, its own scroll anchoring and its own hover
/// overlay. The rows inside it were already the transcript's rows, so what the owner saw was the
/// right content in the wrong list: no height cache, no minimap, no jump-to-newest, and a scroll
/// position that reset where the real chat remembers. The reason it existed was that
/// `TranscriptModel` could only read the store, and a swarm session started on the command line
/// has no store row. `TranscriptModel.chairLog` removes that reason, so the second list goes.
///
/// The composer below it is still `SwarmSessionInput` rather than `ComposerView`, and that is not
/// an oversight. A chair in a tmux pane is typed at through the bus, not through a store delivery
/// queue, so the send path is genuinely different even though the list is not.
private struct SwarmSessionChat: View {
    var reader: SwarmSessionReaderModel
    var transcript: TranscriptModel

    private var textSize: ChatTextSize { ColourThemePreference.shared.chatTextSize }
    private var chatFontID: String { ColourThemePreference.shared.chatFont }
    private var lineHeight: ChatLineHeight { ColourThemePreference.shared.chatLineHeight }

    var body: some View {
        VStack(spacing: 0) {
            if let failure = transcript.chatLogFailure {
                Text(failure)
                    .font(Typo.body)
                    .foregroundStyle(Palette.textSecondary)
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else {
                if transcript.droppedRows > 0 {
                    DetailCaption(
                        text: "\(Counted.of(transcript.droppedRows, "earlier step")) not shown"
                    )
                    .frame(maxWidth: .infinity, alignment: .center)
                    .padding(.top, Metrics.spacingSmall)
                }
                TranscriptView(transcript: transcript, drawsBackground: false)
            }

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
                            // The same renderer the transcript uses, so a bus message gets the
                            // code spans, lists and file links its author wrote, and a path in one
                            // previews on hover + Space like a path anywhere else in the app.
                            MarkdownView(row.body ?? "This message body could not be read.")
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
