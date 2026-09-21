import SwiftUI
import SwarmCore

/// Home, New workspace and Search, as rows over the list, which is where Conductor keeps them.
///
/// Outside the `List` rather than rows of it, so the list's selection, keyboard navigation and
/// drag reorder stay about workspaces alone.
struct SidebarNavigation: View {
    @Environment(AppModel.self) private var app

    var onNewWorkspace: () -> Void
    var onStartProject: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            VStack(alignment: .leading, spacing: Metrics.spacingHair) {
                Button {
                    AskPanelModel.shared.close()
                    app.selection = .home
                } label: {
                    SidebarNavigationLabel(title: "Home", systemImage: "house", isActive: app.selection == .home)
                }
                .buttonStyle(.plain)

                Menu {
                    Button("New workspace…", action: onNewWorkspace)
                    Button("New project…", action: onStartProject)
                } label: {
                    SidebarNavigationLabel(title: "New workspace", systemImage: "plus")
                }
                // `.button` with `.plain`, so the label keeps the ink it is handed. See `SidebarDock`.
                .menuStyle(.button)
                .buttonStyle(.plain)
                .menuIndicator(.hidden)

                Button {
                    SearchPanelModel.shared.open(app: app)
                } label: {
                    SidebarNavigationLabel(title: "Search", systemImage: "magnifyingglass")
                }
                .buttonStyle(.plain)
            }
            .padding(.horizontal, Metrics.inset)
            .padding(.vertical, Metrics.spacingSmall)

            Hairline()
        }
    }
}

/// One of those rows: a glyph and a word, lit under the pointer and while it is where you are.
private struct SidebarNavigationLabel: View {
    var title: String
    var systemImage: String
    var isActive = false

    @State private var isHovered = false

    var body: some View {
        HStack(spacing: Metrics.spacingWide) {
            Image(systemName: systemImage)
                .font(Typo.label)
                .frame(width: SidebarMetrics.markColumn)
            Text(title)
                .font(Typo.body)
            Spacer(minLength: 0)
        }
        .foregroundStyle(isActive || isHovered ? Palette.textPrimary : Palette.textSecondary)
        .padding(.horizontal, Metrics.spacing)
        .frame(height: Metrics.rowHeight)
        .contentShape(Rectangle())
        .background(
            isActive ? Palette.selected : (isHovered ? Palette.hover : .clear),
            in: RoundedRectangle(cornerRadius: Metrics.corner, style: .continuous)
        )
        .onHoverChange { isHovered = $0 }
    }
}
