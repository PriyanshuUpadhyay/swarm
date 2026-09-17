import SwiftUI
import SwarmCore

/// A terminal chat whose CLI process has ended, read from the provider's durable log.
struct StoppedTerminalChatView: View {
    @Bindable var model: WorkspaceModel
    var session: Session

    @State private var rows: [TranscriptRow] = []
    @State private var droppedRows = 0
    @State private var failure: String?
    @State private var position = ScrollPosition(edge: .bottom)
    @State private var bubbleWidth = TranscriptBubbleWidth()
    @State private var hoverHost = TranscriptHoverHost()

    private var textSize: ChatTextSize { ColourThemePreference.shared.chatTextSize }
    private var chatFontID: String { ColourThemePreference.shared.chatFont }
    private var lineHeight: ChatLineHeight { ColourThemePreference.shared.chatLineHeight }

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: Metrics.spacing) {
                Label(InteractiveChatLifecycle.State.stopped.label, systemImage: "pause.circle")
                    .font(Typo.label)
                    .foregroundStyle(Palette.textSecondary)
                Spacer()
                Button("Resume") {
                    Task { await model.resumeCLI(session) }
                }
                .buttonStyle(.borderedProminent)
                .accessibilityHint("Continues this provider session in its saved worktree")
            }
            .padding(.horizontal, Metrics.pane)
            .padding(.vertical, Metrics.spacing)

            Divider()

            ScrollView {
                LazyVStack(alignment: .leading, spacing: 0) {
                    Color.clear
                        .frame(height: TranscriptLayout.topSpace)
                        .accessibilityHidden(true)

                    if let failure {
                        Text(failure)
                            .font(Typo.body)
                            .foregroundStyle(Palette.textSecondary)
                            .subagentReadingColumn()
                    } else {
                        SubagentConversationView(
                            rows: rows,
                            prompt: "",
                            home: TranscriptHome(model.workspace),
                            droppedRows: droppedRows,
                            isRunning: false
                        )
                    }
                }
                .padding(.bottom, Metrics.pane)
            }
            .scrollPosition($position)
            .defaultScrollAnchor(.bottom, for: .initialOffset)
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
            .overlay { TranscriptHoverOverlay(host: hoverHost) }
        }
        .background(Palette.surface)
        .environment(\.transcriptHoverHost, hoverHost)
        .environment(\.transcriptBubbleWidth, bubbleWidth)
        .environment(\.fontScale, textSize.scale)
        .environment(\.chatFont, ChatFont(rawValue: chatFontID))
        .environment(\.chatLineHeight, lineHeight)
        .markdownLinkActions(TranscriptLink.actions(for: model))
        .task(id: session.agentSessionID) { await load() }
    }

    private func load() async {
        let agent = session.agentKind
        let providerID = session.agentSessionID
        let sessionID = session.id
        let result = await Task.detached(priority: .utility) {
            InteractiveChatTranscript.read(
                agent: agent, providerSessionID: providerID, sessionID: sessionID
            )
        }.value
        guard !Task.isCancelled else { return }
        switch result {
        case .success(let transcript):
            rows = TranscriptModel.rows(from: transcript.messages)
            droppedRows = transcript.droppedRows
            failure = nil
        case .failure(let reason):
            rows = []
            droppedRows = 0
            failure = reason.sentence
        }
    }
}
