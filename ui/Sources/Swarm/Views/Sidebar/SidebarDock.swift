import SwiftUI
import SwarmCore

/// Ask Swarm, at the leading end of the sidebar's status bar.
///
/// Home and New moved to the rows over the list, where Conductor keeps them. See
/// `SidebarNavigation`. Ask stays down here as a button, which is where the owner asked for it.
struct SidebarDock: View {
    @Environment(AppModel.self) private var app

    @State private var hovered: Item?

    private enum Item { case ask }

    var body: some View {
        HStack(spacing: Metrics.spacingTight) {
            Button { askSwarm() } label: {
                label(
                    AskConversation.title, systemImage: PaneGlyph.chat, item: .ask,
                    isActive: AskPanelModel.shared.isOpen
                )
                // The mark the Ask Swarm row wore. A question left running behind a closed panel
                // is otherwise a turn nothing in the window reports. See `AppModel.askStatus`.
                .overlay(alignment: .topTrailing) {
                    if let status = app.askStatus {
                        WorkspaceStatusGlyph(status: status)
                            .scaleEffect(0.7)
                            .offset(x: Metrics.spacingSmall, y: -Metrics.spacingSmall)
                            .allowsHitTesting(false)
                    }
                }
            }
            .buttonStyle(.plain)
            .help("Ask Swarm")
            .accessibilityValue(app.askStatus?.label ?? "")
        }
    }

    private func askSwarm() {
        AskPanelModel.shared.toggle(app: app)
    }

    private func label(_ title: String, systemImage: String, item: Item, isActive: Bool) -> some View {
        Label(title, systemImage: systemImage)
            .labelStyle(.iconOnly)
            .font(Typo.label)
            .foregroundStyle(isActive ? Palette.accent : Palette.textSecondary)
            .frame(width: Metrics.rowHeight, height: Metrics.rowHeight)
            .contentShape(Rectangle())
            .background(
                hovered == item ? Palette.hover : .clear,
                in: RoundedRectangle(cornerRadius: Metrics.cornerSmall)
            )
            .onHoverChange { hovered = $0 ? item : (hovered == item ? nil : hovered) }
    }
}
