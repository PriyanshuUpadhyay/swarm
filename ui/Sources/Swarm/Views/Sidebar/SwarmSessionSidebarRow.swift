import SwiftUI
import SwarmCore

/// One read-only swarm session under its project.
struct SwarmSessionSidebarRow: View {
    var session: SwarmProjectSession

    @Environment(\.backgroundProminence) private var prominence
    @Environment(\.sidebarRowIndent) private var rowIndent

    private var isOnSelection: Bool { prominence == .increased }

    var body: some View {
        Label {
            VStack(alignment: .leading, spacing: 1) {
                Text(session.title)
                    .font(Typo.caption)
                    .foregroundStyle(isOnSelection ? Palette.textInverted : Palette.textSecondary)
                    .lineLimit(1)
                    .truncationMode(.tail)
                Text(Date(timeIntervalSince1970: TimeInterval(session.session.createdAt)), style: .relative)
                    .font(Typo.micro)
                    .foregroundStyle(
                        isOnSelection ? Palette.textInverted.opacity(0.8) : Palette.textTertiary
                    )
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        } icon: {
            Image(systemName: "bubble.left.and.bubble.right")
                .font(Typo.micro)
                .foregroundStyle(isOnSelection ? Palette.textInverted : Palette.textTertiary)
                .accessibilityHidden(true)
        }
        .labelStyle(SidebarRowLabelStyle())
        .padding(.leading, rowIndent)
        .accessibilityElement(children: .combine)
        .accessibilityLabel(session.title)
        .accessibilityValue("Swarm session")
    }
}
