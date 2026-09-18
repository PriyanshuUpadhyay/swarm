import SwiftUI
import SwarmCore

/// Home, New and Ask Swarm, at the leading end of the sidebar's status bar.
///
/// **These were two rows and a plus at the top of the list.** Home carried the pane's one `+`, and
/// Ask Swarm sat under it as a destination of its own. The owner found "Home / Ask Swarm" a waste of
/// the pane's best real estate and asked for Ask behind a button, as Amp does it; the plus came
/// with them, as a third button, second in the row because making something is reached for more
/// often than asking. The rows' sixty points went back to the list.
///
/// Buttons in a status bar rather than a toolbar, because this strip is already where the pane
/// keeps the controls that are about the whole of it, and it is in both shapes of the pane.
struct SidebarDock: View {
    @Environment(AppModel.self) private var app

    var onNewWorkspace: () -> Void
    var onStartProject: () -> Void

    @State private var hovered: Item?

    private enum Item { case home, new, ask }

    var body: some View {
        HStack(spacing: Metrics.spacingTight) {
            Button { goHome() } label: {
                label("Home", systemImage: "house", item: .home, isActive: app.selection == .home)
            }
            .buttonStyle(.plain)
            .help("Home")

            Menu {
                // The same words, and the same order, the plus on Home's row had.
                Button("New workspace…", action: onNewWorkspace)
                Button("New project…", action: onStartProject)
            } label: {
                label("New", systemImage: "plus", item: .new, isActive: false)
            }
            // `.button` with `.plain` rather than `.borderlessButton`, for the reason measured on
            // the workspace row's own menu: the borderless style inks its label itself and ignores
            // the colour it is handed.
            .menuStyle(.button)
            .buttonStyle(.plain)
            .menuIndicator(.hidden)
            .fixedSize()
            .help("New workspace or project")

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

    private func goHome() {
        AskPanelModel.shared.close()
        app.selection = .home
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
