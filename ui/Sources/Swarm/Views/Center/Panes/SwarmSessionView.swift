import SwiftUI
import SwarmCore

/// A discovered swarm session, with the chair chat and each agent's bus history.
struct SwarmSessionView: View {
    var item: SwarmProjectSession
    @State private var reader: SwarmSessionReaderModel
    @State private var showsTerminal = false
    /// The document a link in this session was clicked on, drawn where the chat is. Nil for none.
    ///
    /// **This pane exists because a swarm session has no workspace**, and every door in this
    /// window that opens a file is a workspace's: the review tab, the browser tab and the
    /// inspector all hang off `selection.workspaceID`, which `.swarmSession` answers nil for. So a
    /// path an agent named had nowhere to go and went to the user's editor instead, which is the
    /// one place the reader was not. `DocumentPreviewView` needs no worktree, and says so, so the
    /// chat's half of the split is a place it can be drawn in.
    @State private var preview: String?
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
                // Beside the terminal toggle rather than over the document, because it is the same
                // question that button asks: which of this session's things is in the left half.
                if preview != nil {
                    Button("Show chat") { preview = nil }
                        .buttonStyle(.bordered)
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
                        if let preview {
                            SwarmSessionDocument(path: preview)
                        } else if let transcript {
                            SwarmSessionChat(reader: reader, transcript: transcript)
                        }
                    }
                    .frame(minWidth: 420)
                    SwarmSessionAgentsView(reader: reader)
                        .frame(minWidth: 260, idealWidth: 320, maxWidth: 420)
                        .markdownLinkActions(
                            TranscriptLink.actions(for: localModel, showsFile: showsFile)
                        )
                }
                // The chair's chat builds its own link actions out of the transcript's workspace,
                // so it is reached through the environment rather than through the call above.
                // Both halves of the split have to land in the same pane.
                .environment(\.transcriptShowsFile, showsFile)
            }
        }
        // Fills the pane rather than sitting in the middle of it.
        //
        // **Without this the whole session floated.** The pane hands this view a flexible frame,
        // and a flexible frame CENTRES a child that does not fill it. Every child here used to be
        // greedy, because the chat is a transcript, so the stack filled by accident; the document
        // pane's empty state is not greedy, and the first picture of it showed the title, the
        // split and the agents squeezed into a 280 point band with 540 points of window above it
        // and 545 below. `InspectorView` carries the same line for the same reason.
        .frame(maxWidth: .infinity, maxHeight: .infinity)
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

    /// Where a clicked document lands. Held as one value so that the agents panel and the chair
    /// chat, which reach their link actions by different roads, open into the same pane.
    private var showsFile: @MainActor @Sendable (String) -> Void {
        let preview = $preview
        return { preview.wrappedValue = $0 }
    }
}

/// A document named in this session, drawn by Swarm's own Markdown and HTML viewer.
///
/// `worktree` is nil, which `DocumentPreviewView` accepts and documents: the preview may then read
/// the folder the file sits in and nothing above it, which is what one clicked path asked for.
struct SwarmSessionDocument: View {
    var path: String

    /// Asked once per draw rather than held, because the pane is rebuilt on each clicked path and
    /// a file that arrives while it is open is a case nothing here has to serve.
    private var exists: Bool { FileManager.default.fileExists(atPath: path) }

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: Metrics.spacing) {
                Image(systemName: "doc.text")
                    .font(Typo.micro)
                    .foregroundStyle(Palette.textTertiary)
                    .accessibilityHidden(true)
                Text((path as NSString).lastPathComponent)
                    .font(Typo.labelEmphasis)
                    .lineLimit(1)
                    .truncationMode(.middle)
                Spacer(minLength: Metrics.spacingSmall)
                // The one way out to the real file. A viewer with no door to the thing it is
                // showing is a dead end, and this is where the click used to go.
                if exists {
                    Button("Open in Editor") { Reveal.inEditor(path) }
                        .buttonStyle(.borderless)
                        .controlSize(.small)
                }
            }
            .padding(.horizontal, Metrics.inset)
            .frame(height: Metrics.barHeight)
            .help(path)
            .overlay(alignment: .bottom) { Hairline() }

            if exists {
                // ponytail: loaded once, so a file rewritten while it is open does not redraw. The
                // revision is the workspace's change generation everywhere else, and this pane has
                // no workspace; watch the file's folder if that turns out to matter.
                DocumentPreviewView(path: path, worktree: nil, revision: 0)
            } else {
                // Said rather than left silent. An agent names a file it wrote into a scratch
                // directory, the run that owned that directory removes it, and the message
                // naming it stays in the bus for ever. See `TranscriptLink.openWithoutAWorktree`.
                EmptyStateView(
                    glyph: "doc.questionmark",
                    title: "That file is not there any more",
                    message: path
                )
            }
        }
        // The half of the split this pane is, whatever is in it. See `SwarmSessionView.body`.
        .frame(maxWidth: .infinity, maxHeight: .infinity)
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

struct SwarmSessionAgentsView: View {
    var reader: SwarmSessionReaderModel

    /// Which agent is open, by `SwarmSessionAgentDigest.id`, and nil for none.
    ///
    /// One at a time, which is what makes the closed list worth reading. Held here rather than as
    /// a flag on each row so that opening one closes the last, the way a Mail mailbox or a Finder
    /// column does.
    @State private var opened: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            header
            content
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
        .background(Palette.windowBackground)
    }

    /// The same height as the centre column's own first band, so the two panes start their first
    /// line together and the rule between them runs straight across the join. `Metrics.pane` of
    /// padding put this title 24 points down a column whose neighbour's title is at 8, which is
    /// what made the panel read as a floating box rather than as the other half of the window.
    /// See `InspectorView`, which draws its top band the same way and for the same reason.
    private var header: some View {
        HStack(spacing: Metrics.spacingSmall) {
            Text("Agents")
                .font(Typo.labelEmphasis)
                .foregroundStyle(Palette.textPrimary)
            Spacer(minLength: Metrics.spacingSmall)
            if !reader.agents.isEmpty {
                Text(reader.agents.count.formatted())
                    .font(Typo.caption)
                    .monospacedDigit()
                    .foregroundStyle(Palette.textTertiary)
            }
        }
        .padding(.horizontal, Metrics.inset)
        .frame(height: Metrics.barHeight)
        .overlay(alignment: .bottom) { Hairline() }
    }

    @ViewBuilder
    private var content: some View {
        // `ContentUnavailableView` under both, through `EmptyStateView`, rather than a grey
        // sentence pinned to the top left corner. It is the system's own empty state, so a panel
        // with nothing in it is centred, marked and worded the way every other empty pane in this
        // app and on this Mac is.
        if let failure = reader.agentsFailure {
            EmptyStateView(
                glyph: "exclamationmark.triangle",
                title: "The bus could not be read",
                message: failure
            )
        } else if reader.agents.isEmpty {
            EmptyStateView(
                glyph: "person.2",
                title: "No agents yet",
                message: "Agents this session launches appear here."
            )
        } else {
            ScrollView {
                SwarmSessionAgentList(reader: reader, opened: $opened)
            }
        }
    }
}

/// The rows themselves, apart from the scroller that holds them.
///
/// **Split out so a picture can be taken of it.** `ImageRenderer` proposes no height to a
/// `ScrollView`, so a column photographed whole comes out as an empty box with a header on it,
/// which is what the first capture of this panel was. `CrewMessageGallery` records the same lesson.
/// The app wraps this in the scroller and the gallery draws it directly, so what is photographed is
/// the list the app runs rather than a copy of it.
struct SwarmSessionAgentList: View {
    var reader: SwarmSessionReaderModel
    @Binding var opened: String?

    var body: some View {
        LazyVStack(alignment: .leading, spacing: 0) {
            ForEach(reader.agents) { agent in
                SwarmSessionAgentRow(
                    digest: agent,
                    reader: reader,
                    isOpen: opened == agent.id,
                    toggle: { opened = opened == agent.id ? nil : agent.id }
                )
                // Under the mark rather than across the pane, which is where AppKit puts the rule
                // between two rows of a source list.
                if agent.id != reader.agents.last?.id {
                    Hairline().padding(.leading, Metrics.inset)
                }
            }
        }
        .padding(.vertical, Metrics.spacingSmall)
    }
}

/// One agent: whether it is working, what it last said, and, once it is open, everything it said
/// and a box to answer it in.
///
/// **The composer is inside the open row, and that is the change worth arguing.** Every row used
/// to carry one, always drawn, so a session with six agents was six text boxes stacked down a 320
/// point column and the summaries between them had nowhere to go. The question this panel is
/// opened to answer is which agent is doing what; typing at one is the second question, and it is
/// asked of one agent at a time. A closed row is now a name, a mark and two lines, so the list can
/// be read at a glance, and the row that is open holds the whole conversation and the box.
struct SwarmSessionAgentRow: View {
    var digest: SwarmSessionAgentDigest
    var reader: SwarmSessionReaderModel
    var isOpen: Bool
    var toggle: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HoverRow(isSelected: isOpen) {
                Button(action: toggle) { summary }
                    .buttonStyle(.plain)
                    .accessibilityHint(
                        isOpen ? "Hides this agent's messages" : "Shows this agent's messages"
                    )
            }
            .padding(.horizontal, Metrics.spacingSmall)

            if isOpen { details }
        }
    }

    private var summary: some View {
        VStack(alignment: .leading, spacing: Metrics.spacingHair) {
            HStack(spacing: Metrics.spacing) {
                // `ActivityDot` rather than a dot of this pane's own. It is the app's one busy
                // mark, its idle state is the grey this needs, and until now the only thing
                // drawing it was the component gallery.
                ActivityDot(isActive: digest.agent.alive == true)
                    .frame(width: Metrics.glyph, height: Metrics.glyph)
                    .accessibilityLabel(liveness)
                    .help(liveness)

                Text(digest.agent.id.rawValue)
                    .font(Typo.labelEmphasis)
                    .lineLimit(1)
                    .truncationMode(.middle)

                Spacer(minLength: Metrics.spacingSmall)

                Chip(text: digest.agent.role)

                Image(systemName: "chevron.right")
                    .font(Typo.micro)
                    .imageScale(.small)
                    .foregroundStyle(.tertiary)
                    .rotationEffect(.degrees(isOpen ? 90 : 0))
                    .accessibilityHidden(true)
            }

            Text(digest.latestSummary ?? "No summary yet.")
                .font(Typo.caption)
                .foregroundStyle(Palette.textSecondary)
                // Two lines closed, all of it open. A picture of this page caught it clipped at
                // "became n…" on the open row, where there is no reason left to ration the height.
                .lineLimit(isOpen ? nil : 2)
                .fixedSize(horizontal: false, vertical: true)
                // Under the name rather than under the mark, so the closed list has one left edge
                // for its words and the marks sit outside it in a column of their own.
                .padding(.leading, Metrics.glyph + Metrics.spacing)
        }
        .padding(.horizontal, Metrics.spacing)
        .padding(.vertical, Metrics.spacingWide)
        .contentShape(Rectangle())
    }

    /// Three states and not two. `SwarmAgent.alive` is nil until `SwarmPaneLiveness` has asked the
    /// adapter, which is up to five seconds after a session opens, and a panel that said "Ended"
    /// for those five seconds would be wrong about every agent in it. The dot is grey for both,
    /// and the words are what separate them.
    private var liveness: String {
        switch digest.agent.alive {
        case true?: "Running"
        case false?: "Ended"
        case nil: "Status unknown"
        }
    }

    private var details: some View {
        VStack(alignment: .leading, spacing: Metrics.spacingWide) {
            ForEach(digest.conversation) { row in
                VStack(alignment: .leading, spacing: Metrics.spacingTight) {
                    // Who spoke, rather than which kind of row it is. "Ask" and "Summary" named
                    // the bus verb and left the reader to work out the direction from it.
                    Text(caption(for: row))
                        .font(Typo.micro)
                        .foregroundStyle(Palette.textTertiary)
                    // The same renderer the transcript uses, so a bus message gets the code spans,
                    // lists and file links its author wrote, and a path in one previews on hover +
                    // Space like a path anywhere else in the app.
                    MarkdownView(row.body ?? "This message body could not be read.")
                }
                .frame(maxWidth: .infinity, alignment: .leading)
            }

            SwarmSessionInput(
                reader: reader,
                agent: digest.agent.id,
                sessionID: digest.sessionID,
                target: .agent,
                placeholder: "Message \(digest.agent.id.rawValue)",
                maxLines: 3
            )
        }
        .padding(.horizontal, Metrics.inset)
        .padding(.bottom, Metrics.spacingWide)
    }

    private func caption(for message: SwarmMessage) -> String {
        message.sender == digest.agent.id
            ? "\(digest.agent.id.rawValue) answered"
            : "Chair asked"
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
