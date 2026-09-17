import SwiftUI
import SwarmCore

/// The detail column's toolbar. The sidebar draws its own toggle while it is open, grouped with
/// New (see `SidebarView`), so this one draws a toggle only while the sidebar is folded. Search and the inspector use standard toolbar buttons; only the
/// pull request band needs a title-bar accessory to follow the inspector's width.
struct SwarmWindowToolbar: ToolbarContent {
    let app: AppModel
    let isSidebarFolded: Bool

    var body: some ToolbarContent {
        if isSidebarFolded {
            ToolbarItem(placement: .navigation) {
                SidebarToggleButton()
            }
        }

        ToolbarItem(placement: .navigation) {
            WindowTitleControl(app: app)
                .padding(.leading, Metrics.spacingWide)
        }
        // The editable window title is text, so it does not need a button's background.
        .sharedBackgroundVisibility(.hidden)

        ToolbarSpacer(.flexible, placement: .navigation)

        // First of the trailing group, so the inspector toggle keeps the window's edge the way
        // the sidebar toggle keeps the other one. Only on a workspace: Home and Ask Swarm have no
        // centre column to open a tab in, and a `+` that does nothing there is worse than none.
        // `selectedModel` rather than `selectedWorkspace`, because the menu acts on the model and
        // a model not prepared yet has nothing for it to act on.
        if let workspace = app.selectedModel, app.selectedWorkspace != nil {
            ToolbarItem(placement: .primaryAction) {
                NewTabMenu(model: workspace)
            }
        }

        ToolbarItem(placement: .primaryAction) {
            Button("Search", systemImage: "magnifyingglass") {
                SearchPanelModel.shared.open(app: app)
            }
            .help("Search workspaces, transcripts and commands")
        }

        if app.selectedWorkspace != nil {
            ToolbarItem(placement: .primaryAction) {
                Button("Inspector", systemImage: "sidebar.right") {
                    app.isInspectorVisible.toggle()
                }
                .accessibilityValue(app.isInspectorVisible ? "Shown" : "Hidden")
                .help(app.isInspectorVisible ? "Hide the changed files" : "Show the changed files")
            }
        }
    }
}
