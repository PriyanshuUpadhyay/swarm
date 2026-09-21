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
    /// The uncommitted work in this session's own folder, which is what the right hand column
    /// shows now that the agents are in the conversation.
    ///
    /// **The window's own inspector cannot serve this pane.** `AppModel.isInspectorPresented` is
    /// `isInspectorVisible && selectedWorkspace != nil`, and `SidebarSelection.swarmSession`
    /// answers nil for a workspace, so the whole right column was switched off here and the
    /// toolbar's Inspector button was hidden with it. Rather than teach the window that a session
    /// is a workspace, which it is not, this pane keeps its own column and its own switch.
    @State private var changes: SwarmSessionChangesModel
    @State private var showsChanges = true
    @State private var localModel: WorkspaceModel?
    @State private var isLocalLoaded = false
    @State private var closedSession: Session?
    /// Built in `.task` rather than in `init`, because it needs the `AppModel` from the environment
    /// and an environment value does not exist yet while an initialiser runs.
    @State private var transcript: TranscriptModel?
    @Environment(AppModel.self) private var app

    init(item: SwarmProjectSession, bus: any SwarmBus) {
        self.item = item
        _reader = State(initialValue: SwarmSessionReaderModel(item: item, bus: bus))
        _changes = State(initialValue: SwarmSessionChangesModel(cwd: item.session.cwd))
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
            // The same strip every other pane draws: the chat, and its terminal when there is one,
            // as underlined tabs, with the session's own controls at the trailing end.
            HStack(spacing: 0) {
                SessionHeaderTab(title: item.title, isActive: !showsTerminal && preview == nil) {
                    showsTerminal = false
                    preview = nil
                }
                if let preview {
                    SessionHeaderTab(title: (preview as NSString).lastPathComponent, isActive: true) {}
                        .accessibilityHint("A document the chat pointed at")
                }
                // **Not behind the debug flag, unlike every other CLI chat's terminal.** A swarm
                // chair splits its own tmux window for each seat it spawns, so this one view is
                // every worker's pane as well as the chair's. Hiding it hid the whole swarm, and
                // the point of a swarm is that its workers are watchable.
                if item.session.adapter == "tmux", localTab != nil {
                    SessionHeaderTab(title: "Panes", isActive: showsTerminal) {
                        showsTerminal = true
                        preview = nil
                    }
                }
                Spacer(minLength: Metrics.spacing)
                Text(item.sessions.count == 1
                    ? "Session \(item.id.rawValue)"
                    : "\(item.sessions.count) sessions")
                    .font(Typo.caption)
                    .foregroundStyle(Palette.textTertiary)
                    .padding(.trailing, Metrics.spacing)
                // Only where this pane draws a column of its own. A session inside a workspace
                // gets the window's real inspector and the toolbar's own Inspector button, so a
                // second switch here would be two controls for one column.
                if ownsChangesColumn, !showsTerminal {
                    Button("Changes", systemImage: "sidebar.right") {
                        showsChanges.toggle()
                    }
                    .labelStyle(.iconOnly)
                    .buttonStyle(.borderless)
                    .padding(.leading, Metrics.spacing)
                    .accessibilityValue(showsChanges ? "Shown" : "Hidden")
                    .help(showsChanges ? "Hide the changed files" : "Show the changed files")
                }
            }
            .padding(.leading, Metrics.spacingSmall)
            .padding(.trailing, Metrics.gutter)
            .frame(height: Metrics.barHeight)
            .background(Palette.surface)
            // A chat whose CLI stopped (Swarm quit with terminal persistence off) starts again as
            // soon as it is opened. Once per launch, so a CLI that exits at once is not started in
            // a loop; after that a send starts it. See `TranscriptModel.submit`.
            .task(id: stoppedChat) {
                guard let localSession, let localModel, stoppedChat == localSession.id else { return }
                await localModel.restartStoppedCLI(localSession)
            }

            Hairline()

            if showsTerminal, let localModel, let localTab {
                ToolPaneView(
                    model: localModel, tab: localTab,
                    splitColumn: { _, _ in }, showsShell: true, paneMenu: nil
                )
            } else {
                HSplitView {
                    Group {
                        if let preview {
                            SwarmSessionDocument(path: preview)
                        } else if let transcript {
                            SwarmSessionChat(reader: reader, transcript: transcript, model: localModel)
                        }
                    }
                    .frame(minWidth: 420)
                    if ownsChangesColumn, showsChanges {
                        SwarmSessionChangesView(model: changes)
                            .frame(minWidth: 260, idealWidth: 320, maxWidth: 420)
                    }
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
            if let id = item.localSessionID, localSession == nil {
                closedSession = try? await app.store?.session(id: id)
            }
            isLocalLoaded = true
            await reader.follow()
        }
        // Its own task, because `reader.follow` above never returns and the chair log has to be
        // followed at the same time as the bus.
        .task(id: transcriptKey) {
            guard transcriptKey != nil else { return }
            let model = TranscriptModel(
                swarmSession: localSession ?? busChair,
                chairLog: ChairTranscriptOutput.reader(
                    path: item.session.chairLog, sessionID: chatSessionID,
                    provider: item.session.chairProvider, chairID: item.session.chairID
                ),
                directory: item.session.cwd,
                app: app
            )
            // A chair Swarm started is its session's chat, and the composer reaches its CLI through
            // that session's terminal. The bus cannot: its tmux adapter runs a plain `tmux`, which
            // is the default server rather than Swarm's, so `swarm type` failed with "can't find
            // pane". Only a chair started elsewhere is typed at through the bus.
            if localSession == nil {
                let reader = reader
                let app = app
                let chair = SwarmAgentID("orchestrator")
                model.chairInput = { text in
                    guard await reader.type(text, to: chair, in: nil) else {
                        app.notice = SwarmNotice(
                            message: reader.inputFailure(for: chair, in: nil)
                                ?? "The chair's pane did not accept the message."
                        )
                        return false
                    }
                    return true
                }
            }
            transcript = model
            await model.follow()
        }
    }

    /// Nil until the workspace's sessions are read, so the chat is built once, on the session it
    /// belongs to.
    private var transcriptKey: String? {
        isLocalLoaded ? localSession?.id.rawValue ?? chatSessionID.rawValue : nil
    }

    /// The chat of a chair started outside Swarm, which has no row in the store.
    private var busChair: Session {
        Session(
            id: chatSessionID, workspaceID: nil, title: item.title,
            agentSessionID: item.session.chairID?.rawValue,
            agentKind: item.session.chairProvider.flatMap(AgentKind.init(rawValue:)) ?? .claudeCode
        )
    }

    private var stoppedChat: SessionID? {
        guard let localSession,
              TerminalSessionStore.shared.interactiveState(for: localSession.id) == .stopped
        else { return nil }
        return localSession.id
    }

    /// This session's chat, open or closed. A closed one still draws and still takes a message,
    /// which opens it again. See `WorkspaceModel.reopen`.
    private var localSession: Session? {
        guard let id = item.localSessionID else { return nil }
        return localModel?.sessions.first { $0.id == id } ?? closedSession
    }

    private var localTab: CenterTab? {
        guard let id = item.localSessionID, let workspaceID = item.workspaceID else { return nil }
        return CenterTabStore.shared.terminal(for: id, in: workspaceID)
    }

    /// Where a clicked document lands. Held as one value so that the agents panel and the chair
    /// chat, which reach their link actions by different roads, open into the same pane.
    /// Whether this pane draws its own changes column.
    ///
    /// Only for a session with no workspace behind it. Where there is one, the window draws the
    /// real inspector against it, with the changed files, the diff, the history and the toolbar's
    /// own button, and this pane must not put a second poorer column beside it. That is what the
    /// owner saw: one workspace with two different right hand columns depending on which of its
    /// rows was clicked. See `SidebarSelection.swarmSession`, which is where the nil was.
    private var ownsChangesColumn: Bool { item.workspaceID == nil }

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
/// The composer is the one every other chat has, with its `/` menu of commands and skills. It
/// sends through `TranscriptModel.submit`, which reaches a chair Swarm started through its
/// terminal and any other chair through the bus. See `TranscriptModel.chairInput`.
private struct SwarmSessionChat: View {
    var reader: SwarmSessionReaderModel
    var transcript: TranscriptModel
    var model: WorkspaceModel?

    var body: some View {
        InteractiveChatView(
            transcript: transcript,
            model: model,
            appended: { AnyView(SwarmSessionAgentReport(reader: reader)) }
        ) {
            EmptyView()
        }
    }
}

/// What this session's agents reported, drawn under the chair's conversation in the same scroll.
///
/// **It used to be a column beside the chat, and that was the wrong shape for what it holds.** A
/// swarm agent is asked one thing and answers once; what it leaves behind when it closes is a
/// summary, and a summary is a paragraph. A paragraph belongs next to the sentence that asked for
/// it, not in a 320 point column with a disclosure triangle and a message box on every row. The
/// column also took the only place the window has for a right hand pane, so a session could not
/// show its changed files at all.
///
/// **Read only, deliberately.** The rows carry the mark, the name, the role and the summary, and
/// nothing opens. Talking to one agent is a different question from reading what they all did,
/// and it is asked at the pane the agent is running in. See `TranscriptTableEntry.appended`, which
/// is how these reach the transcript's own list.
///
/// **An agent appears here when it has reported, and the block starts closed.** It used to list
/// every agent the session had ever launched, each with "Nothing reported yet." under it, under
/// every answer the chair gave. Four running agents put four paragraphs of nothing between the
/// reader and the end of the conversation. A summary is what an agent leaves when it finishes, so
/// it is also the right test for whether there is anything to read.
struct SwarmSessionAgentReport: View {
    var reader: SwarmSessionReaderModel

    @State private var isExpanded = false

    private var finished: [SwarmSessionAgentDigest] {
        reader.agents.filter { $0.latestSummary != nil }
    }

    var body: some View {
        // Nothing at all while nobody has reported, rather than an empty state: this sits under a
        // conversation that is already saying something, and a panel announcing that there is no
        // second thing would be the loudest object on the page. A column had to fill itself; a
        // block in a scroll does not.
        if let failure = reader.agentsFailure {
            note("The bus could not be read. " + failure)
        } else if case let done = finished, !done.isEmpty {
            VStack(alignment: .leading, spacing: Metrics.spacingWide) {
                heading(count: done.count)
                if isExpanded {
                    ForEach(done) { agent in
                        SwarmSessionAgentSummaryRow(digest: agent)
                    }
                }
            }
            .padding(.horizontal, TranscriptLayout.inset)
            .padding(.vertical, TranscriptLayout.block)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
    }

    private func heading(count: Int) -> some View {
        ExpandableRowHeader(isExpanded: isExpanded, onToggle: { isExpanded.toggle() }) {
            HStack(spacing: Metrics.spacingSmall) {
                TranscriptDisclosure(isExpanded: isExpanded, isVisible: true)
                Text(Counted.of(count, "agent") + " reported")
                    .font(Typo.micro)
                    .foregroundStyle(Palette.textTertiary)
                Hairline()
            }
        }
        .accessibilityElement(children: .combine)
    }

    private func note(_ text: String) -> some View {
        Text(text)
            .font(Typo.caption)
            .foregroundStyle(Palette.textSecondary)
            .fixedSize(horizontal: false, vertical: true)
            .padding(.horizontal, TranscriptLayout.inset)
            .padding(.top, TranscriptLayout.block)
            .frame(maxWidth: .infinity, alignment: .leading)
    }
}

/// One agent, as one paragraph: whether it is running, what it is called, what it was for, and the
/// summary it left.
struct SwarmSessionAgentSummaryRow: View {
    var digest: SwarmSessionAgentDigest

    /// The mark's box and the gap after it, which is what stands between the row's leading edge
    /// and its first letter. The summary hangs on the name's column rather than on the mark's, so
    /// the block has one left edge for its words.
    private static let markGutter = Metrics.glyph + Metrics.spacing

    /// The summary's own words when the parse fails, because a summary that will not parse still
    /// has to be readable.
    private static func rendered(_ text: String) -> AttributedString {
        (try? AttributedString(
            markdown: text,
            options: .init(interpretedSyntax: .inlineOnlyPreservingWhitespace)
        )) ?? AttributedString(text)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: Metrics.spacingHair) {
            HStack(spacing: Metrics.spacing) {
                // `ActivityDot` rather than a dot of this pane's own. It is the app's one busy
                // mark and its idle state is the grey this needs.
                ActivityDot(isActive: digest.agent.alive == true)
                    .frame(width: Metrics.glyph, height: Metrics.glyph)
                    .accessibilityLabel(liveness)
                    .help(liveness)

                Text(digest.agent.id.rawValue)
                    .font(Typo.labelEmphasis)
                    .lineLimit(1)
                    .truncationMode(.middle)

                Chip(text: digest.agent.role)

                Spacer(minLength: Metrics.spacingSmall)
            }

            // Inline markdown, because an agent's summary names files and symbols in backticks and
            // a plain `Text` put the backticks on screen. Foundation's own parse rather than the
            // chat's `MarkdownView`, which holds a text view that `ImageRenderer` cannot draw, so
            // the design page would photograph a placeholder instead of this row.
            Text(Self.rendered(digest.latestSummary ?? ""))
                .font(Typo.caption)
                .foregroundStyle(Palette.textSecondary)
                .fixedSize(horizontal: false, vertical: true)
                .frame(maxWidth: TranscriptLayout.proseMeasure, alignment: .leading)
                .padding(.leading, Self.markGutter)
                .textSelection(.enabled)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .accessibilityElement(children: .combine)
    }

    /// Three states and not two. `SwarmAgent.alive` is nil until `SwarmPaneLiveness` has asked the
    /// adapter, which is up to five seconds after a session opens, and a row that said "Ended" for
    /// those five seconds would be wrong about every agent in it. The dot is grey for both, and the
    /// words are what separate them.
    private var liveness: String {
        switch digest.agent.alive {
        case true?: "Running"
        case false?: "Ended"
        case nil: "Status unknown"
        }
    }
}


/// One tab in the session pane's strip, drawn the way `TabItemView` draws a tab: full ink and a
/// rule under it while selected, a step quieter otherwise.
private struct SessionHeaderTab: View {
    var title: String
    var isActive: Bool
    var action: () -> Void

    @State private var isHovered = false

    var body: some View {
        Button(action: action) {
            Text(title)
                .font(Typo.label)
                .lineLimit(1)
                .foregroundStyle(isActive || isHovered ? Palette.textPrimary : Palette.textSecondary)
                .padding(.horizontal, Metrics.gutter)
                .frame(height: Metrics.barHeight)
                .overlay(alignment: .bottom) {
                    if isActive {
                        Rectangle().fill(Palette.textPrimary).frame(height: 2)
                            .padding(.horizontal, Metrics.spacingSmall)
                    }
                }
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .onHoverChange { isHovered = $0 }
        .accessibilityAddTraits(isActive ? [.isButton, .isSelected] : .isButton)
    }
}
