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
            VStack(alignment: .leading, spacing: 1) {
                Text(session.title)
                    .foregroundStyle(isOnSelection ? Palette.textInverted : Palette.textPrimary)
                    .lineLimit(1)
                    .truncationMode(.tail)
                Text(
                    Date(timeIntervalSince1970: TimeInterval(
                        session.lastActivity
                    )),
                    style: .relative
                )
                    .font(Typo.micro)
                    .foregroundStyle(
                        isOnSelection ? Palette.textInverted.opacity(0.8) : Palette.textTertiary
                    )
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        } icon: {
            Image(systemName: session.isRunning ? "circle.fill" : "pause.circle")
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
