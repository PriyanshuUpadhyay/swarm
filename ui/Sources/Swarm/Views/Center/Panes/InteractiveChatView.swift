import SwiftUI
import SwarmCore

/// A chat whose agent is a CLI in a pane, drawn as a conversation.
///
/// **The pane itself is not the chat.** A CLI chat used to show its terminal, so reading it meant
/// reading a TUI redraw, and the owner said so: "Why do we have to look at the terminal? It should
/// be hidden behind a debug flag." What the agent said is in its own log, which
/// `TranscriptModel.chairLog` reads, so the same list and the same composer every other chat has
/// serve this one. `InteractiveChatPane.showsTerminal` is the flag that brings the pane back.
///
/// The composer sends through `TranscriptModel.submit`, which types into the CLI's pane, starts a
/// stopped CLI with the message, or hands a chair Swarm did not start to the bus.
struct InteractiveChatView<Header: View>: View {
    var transcript: TranscriptModel
    var model: WorkspaceModel?
    /// Rows under the conversation. The swarm session's agents ride here.
    var appended: (@MainActor () -> AnyView)?
    /// A line above the conversation, for a chat that is starting again.
    @ViewBuilder var header: () -> Header

    /// See `ChatPaneView.room`.
    @State private var room = ComposerRoom()
    @State private var isScrolledUp = false

    private var textSize: ChatTextSize { ColourThemePreference.shared.chatTextSize }
    private var chatFontID: String { ColourThemePreference.shared.chatFont }
    private var lineHeight: ChatLineHeight { ColourThemePreference.shared.chatLineHeight }

    var body: some View {
        VStack(spacing: 0) {
            header()

            if let failure = transcript.chatLogFailure {
                // Anything the caller adds under the conversation is still drawn: a log that will
                // not open says nothing about the agents that reported through the bus.
                ScrollView {
                    VStack(alignment: .leading, spacing: Metrics.gutter) {
                        Text(failure)
                            .font(Typo.body)
                            .foregroundStyle(Palette.textSecondary)
                            .frame(maxWidth: .infinity)
                            .padding(.top, Metrics.pane)
                        if let appended { appended() }
                    }
                    .padding(.horizontal, TranscriptLayout.inset)
                    .padding(.bottom, room.clearance)
                }
            } else {
                if transcript.droppedRows > 0 {
                    DetailCaption(
                        text: "\(Counted.of(transcript.droppedRows, "earlier step")) not shown"
                    )
                    .frame(maxWidth: .infinity, alignment: .center)
                    .padding(.top, Metrics.spacingSmall)
                }
                TranscriptView(
                    transcript: transcript,
                    drawsBackground: false,
                    appended: appended,
                    onScrolledUpChange: { isScrolledUp = $0 }
                )
            }
        }
        .environment(\.composerRoom, room)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        // A CLI that stops to ask is invisible now that its pane is, so its question is drawn over
        // the conversation the way a managed chat's is. See `InteractivePermissionCardView`.
        .overlay(alignment: .bottom) {
            if let card = TerminalSessionStore.shared.permissionCard(for: transcript.session.id) {
                InteractivePermissionCardView(card: card, sessionID: transcript.session.id)
                    .id(card.id)
                    .frame(maxWidth: 720)
                    .padding(.horizontal, TranscriptLayout.cardInset)
                    .padding(.bottom, room.clearance)
            }
        }
        .overlay(alignment: .bottom) {
            ComposerDock(showsJumpToNewest: isScrolledUp, onJumpToNewest: transcript.jumpToLiveEnd) {
                ComposerView(transcript: transcript, model: model, room: room)
            }
        }
        .onGeometryChange(for: CGFloat.self) { PaneMeasure.room($0.size.height) } action: {
            room.height = $0
        }
        .environment(\.fontScale, textSize.scale)
        .environment(\.chatFont, ChatFont(rawValue: chatFontID))
        .environment(\.chatLineHeight, lineHeight)
    }
}

/// What a CLI stopped to ask, drawn where the conversation can be read behind it.
struct InteractivePermissionCardView: View {
    var card: InteractivePermissionCard
    var sessionID: SessionID

    var body: some View {
        let answer: (InteractivePermissionAnswer) -> Void = {
            TerminalSessionStore.shared.answerPermissionCard(for: sessionID, with: $0)
        }
        if card.ask.isQuestion {
            AgentQuestionCard(
                ask: card.ask,
                decision: nil,
                onAnswer: { decision in
                    guard case .answer(let input) = decision else { return }
                    answer(.answer(input: input))
                },
                onAnswerInTerminal: { answer(.terminal) }
            )
        } else {
            PermissionAskRowView(
                ask: card.ask,
                decision: nil,
                note: "",
                projectName: nil,
                onInteractiveAnswer: answer
            )
        }
    }
}

/// The chat of one CLI-backed session, read from the provider's own log.
///
/// The log is re-read when the provider's session id changes, because a CLI started again writes
/// a new one, and the id arrives from the hook a moment after the pane starts.
struct InteractiveChatPane: View {
    @Bindable var model: WorkspaceModel
    var session: Session
    /// Swaps this pane for the shell the CLI runs in. Nil where there is nowhere to swap to.
    ///
    /// **A chair splits its own tmux window for every seat it spawns, and this chat hid them.**
    /// The transcript says "Three visible panes now exist" while the reader has no door to them,
    /// which is the whole promise of a swarm broken by the view that replaced the terminal.
    var onShowPanes: (() -> Void)?
    var onShowTerminal: (() -> Void)?

    /// The debug switch that brings the raw pane back. Off, a CLI chat is a conversation.
    static let terminalKey = "terminal.showAgentPane"

    static func terminalKey(for sessionID: SessionID) -> String {
        InteractiveChatPanePreferences.terminalKey(for: sessionID)
    }

    static func showsTerminal(for sessionID: SessionID) -> Bool {
        InteractiveChatPanePreferences.showsTerminal(for: sessionID)
    }

    static var showsTerminal: Bool { UserDefaults.standard.bool(forKey: terminalKey) }

    /// How long a CLI may take to say it started before this pane says it has not.
    ///
    /// **The fault this closes was a Codex update question.** Codex asked "Update available! 1.
    /// Update now 2. Skip" before it read anything, the chat sat behind a spinner that never
    /// ended, and nothing on screen said why. Its `SessionStart` hook writes within a second of a
    /// healthy start, which `SmokeChat.waitForProviderSession` relies on too, so twenty seconds is
    /// a stall rather than a slow machine. A trust screen and an expired login land here as well.
    private static let reportDeadline = Duration.seconds(20)

    @State private var transcript: TranscriptModel?
    @State private var isLate = false
    @Environment(AppModel.self) private var app

    private var state: InteractiveChatLifecycle.State {
        TerminalSessionStore.shared.interactiveState(for: session.id)
    }

    var body: some View {
        Group {
            if let transcript {
                InteractiveChatView(transcript: transcript, model: model) { starting }
            } else {
                VStack(spacing: 0) {
                    starting
                    Spacer(minLength: 0)
                }
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(Palette.surface)
        .markdownLinkActions(TranscriptLink.actions(for: model))
        .task(id: session.agentSessionID) { await follow() }
        // Started when the chat is opened, once per launch of the app, so a CLI that ends as soon
        // as it starts is not started again and again. A send starts it after that.
        .task(id: session.id) {
            guard state == .stopped else { return }
            await model.restartStoppedCLI(session)
        }
        .task(id: state) {
            isLate = false
            guard state != .running else { return }
            try? await Task.sleep(for: Self.reportDeadline)
            isLate = !Task.isCancelled
        }
    }

    /// The strip above the conversation. It says what the CLI is doing, and it holds the door to
    /// the panes, which is why a running chat draws it too.
    @ViewBuilder
    private var starting: some View {
        if state != .running || onShowPanes != nil {
            HStack(spacing: Metrics.spacing) {
                if isLate {
                    Text("The CLI has not reported in. It may be waiting at a question.")
                        .font(Typo.label)
                        .foregroundStyle(Palette.textSecondary)
                    Spacer()
                    Button("Show the terminal") {
                        InteractiveChatPanePreferences.setShowsTerminal(true, for: session.id)
                        onShowTerminal?()
                    }
                    .controlSize(.small)
                    .help("Draws the pane this chat runs in, where a question can be answered")
                } else if model.restartedCLIs.contains(session.id),
                   !model.restartingCLIs.contains(session.id),
                   state == .stopped {
                    Text("The chat's CLI stopped.")
                        .font(Typo.label)
                        .foregroundStyle(Palette.textSecondary)
                    Spacer()
                    Button("Start again") { Task { await model.resumeCLI(session) } }
                        .controlSize(.small)
                } else if state != .running {
                    ProgressView().controlSize(.small)
                    Text("Starting the chat")
                        .font(Typo.label)
                        .foregroundStyle(Palette.textSecondary)
                    Spacer()
                } else {
                    Spacer()
                }
                if let onShowPanes {
                    Button("Panes", systemImage: "rectangle.split.2x1", action: onShowPanes)
                        .controlSize(.small)
                        .help("Draws the pane this CLI runs in, and every pane it split for a worker")
                }
            }
            .padding(.horizontal, Metrics.pane)
            .padding(.vertical, Metrics.spacing)
            .overlay(alignment: .bottom) { Hairline() }
        }
    }

    /// Builds this chat on the provider's log and follows it for as long as the tab is open.
    ///
    /// A failed reader is nil rather than a sentence. It means the CLI has not reported its own
    /// session id yet, which is true for the first second of every new chat, and an empty list
    /// under a "Starting the chat" line already says that.
    private func follow() async {
        let made = TranscriptModel(
            swarmSession: session,
            chairLog: try? InteractiveChatTranscript.reader(
                agent: session.agentKind, providerSessionID: session.agentSessionID,
                sessionID: session.id
            ).get(),
            directory: model.workspace.path,
            workspace: model.workspace,
            app: app
        )
        transcript = made
        await made.follow()
    }
}

// MARK: - Interactive Chat Pane Preferences & Surfaces

public enum SwarmChatSurface: Equatable, Sendable {
    case chat
    case terminal
}

public enum SwarmChatSurfaceDecision {
    public static func surface(isTerminalToggleOn: Bool) -> SwarmChatSurface {
        isTerminalToggleOn ? .terminal : .chat
    }
}

public enum InteractiveChatPanePreferences {
    public static func terminalKey(for sessionID: SessionID) -> String {
        "terminal.showAgentPane.\(sessionID.rawValue)"
    }

    public static func showsTerminal(
        for sessionID: SessionID,
        in defaults: UserDefaults = .standard
    ) -> Bool {
        defaults.bool(forKey: terminalKey(for: sessionID))
    }

    public static func setShowsTerminal(
        _ shows: Bool,
        for sessionID: SessionID,
        in defaults: UserDefaults = .standard
    ) {
        defaults.set(shows, forKey: terminalKey(for: sessionID))
    }

    public static func surface(
        for sessionID: SessionID,
        in defaults: UserDefaults = .standard
    ) -> SwarmChatSurface {
        SwarmChatSurfaceDecision.surface(
            isTerminalToggleOn: showsTerminal(for: sessionID, in: defaults)
        )
    }
}
