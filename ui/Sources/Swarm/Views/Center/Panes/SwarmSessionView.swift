import SwiftUI
import SwarmCore

/// A discovered swarm session, with the chair chat and each agent's bus history.
struct SwarmSessionView: View {
    var item: SwarmProjectSession
    @State private var reader: SwarmSessionReaderModel

    init(item: SwarmProjectSession, bus: any SwarmBus) {
        self.item = item
        _reader = State(initialValue: SwarmSessionReaderModel(session: item.session, bus: bus))
    }

    var body: some View {
        VStack(spacing: 0) {
            HStack(alignment: .firstTextBaseline, spacing: Metrics.spacing) {
                Text(item.title)
                    .font(Typo.heading)
                    .lineLimit(1)
                Spacer()
                Text("Session \(item.id.rawValue)")
                    .font(Typo.caption)
                    .foregroundStyle(Palette.textTertiary)
            }
            .padding(.horizontal, Metrics.pane)
            .padding(.vertical, Metrics.spacing)

            Divider()

            HSplitView {
                SwarmSessionChat(reader: reader, directory: item.session.cwd)
                    .frame(minWidth: 420)
                SwarmSessionAgentsView(reader: reader)
                    .frame(minWidth: 260, idealWidth: 320, maxWidth: 420)
            }
        }
        .background(Palette.surface)
        .task { await reader.follow() }
    }
}

private struct SwarmSessionChat: View {
    var reader: SwarmSessionReaderModel
    var directory: String

    @State private var position = ScrollPosition(edge: .bottom)
    @State private var followsEnd = true
    @State private var bubbleWidth = TranscriptBubbleWidth()
    @State private var hoverHost = TranscriptHoverHost()

    private var textSize: ChatTextSize { ColourThemePreference.shared.chatTextSize }
    private var chatFontID: String { ColourThemePreference.shared.chatFont }
    private var lineHeight: ChatLineHeight { ColourThemePreference.shared.chatLineHeight }

    var body: some View {
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
                    SubagentConversationView(
                        rows: reader.rows,
                        prompt: "",
                        home: TranscriptHome(workspaceID: nil, worktree: directory),
                        droppedRows: reader.droppedRows,
                        isRunning: false
                    )
                }
            }
            .padding(.bottom, Metrics.pane)
        }
        .scrollPosition($position)
        .defaultScrollAnchor(.bottom, for: .initialOffset)
        .onScrollGeometryChange(for: Bool.self) { geometry in
            ScrollEnd.isAtEnd(
                contentHeight: geometry.contentSize.height,
                viewportHeight: geometry.containerSize.height,
                offset: geometry.contentOffset.y
            )
        } action: { _, atEnd in
            followsEnd = atEnd
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
        .onChange(of: reader.rows.count) { _, _ in
            if followsEnd { position.scrollTo(edge: .bottom) }
        }
        .overlay { TranscriptHoverOverlay(host: hoverHost) }
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
                            SwarmSessionAgentView(digest: agent)
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
    @State private var showsHistory = false

    var body: some View {
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
    }
}
