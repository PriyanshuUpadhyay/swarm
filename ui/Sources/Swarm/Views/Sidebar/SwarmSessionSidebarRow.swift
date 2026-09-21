import SwiftUI
import SwarmCore

/// One read-only swarm session under its project.
struct SwarmSessionSidebarRow: View {
    var session: SwarmProjectSession

    @Environment(AppModel.self) private var app
    @Environment(\.backgroundProminence) private var prominence
    @Environment(\.sidebarRowIndent) private var rowIndent

    private var isOnSelection: Bool { prominence == .increased }

    var body: some View {
        Label {
            // One line, with the short age at the trailing edge, the way a workspace row reads.
            HStack(spacing: Metrics.spacing) {
                Text(session.title)
                    .foregroundStyle(isOnSelection ? Palette.textInverted : Palette.textPrimary)
                    .lineLimit(1)
                    .truncationMode(.tail)
                Spacer(minLength: Metrics.spacingSmall)
                TimelineView(.everyMinute) { context in
                    Text(HomeAge.short(
                        for: Date(timeIntervalSince1970: TimeInterval(session.lastActivity)),
                        now: context.date
                    ))
                    .font(Typo.caption)
                    .monospacedDigit()
                    .foregroundStyle(isOnSelection ? Palette.textInverted : Palette.textTertiary)
                }
                .accessibilityHidden(true)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        } icon: {
            // An empty ring when nothing is running, rather than a pause sign: nothing was paused,
            // the chat is simply not working right now.
            Image(systemName: session.isRunning ? "circle.fill" : "circle")
                .font(Typo.micro)
                .foregroundStyle(isOnSelection ? Palette.textInverted : Palette.textSecondary)
                .accessibilityLabel(session.isRunning ? "Running" : "Stopped")
        }
        .labelStyle(SidebarRowLabelStyle())
        .padding(.leading, rowIndent + SidebarMetrics.rowIndent)
        .accessibilityElement(children: .combine)
        .accessibilityLabel(session.title)
        .accessibilityValue(session.isRunning ? "Running chat" : "Stopped chat")
        .contextMenu {
            Button("Archive", role: .destructive) {
                Task { await app.archiveSwarmChat(session) }
            }
        }
    }
}
