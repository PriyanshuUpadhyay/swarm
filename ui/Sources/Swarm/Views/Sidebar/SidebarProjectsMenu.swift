import SwiftUI
import SwarmCore

/// Every project, as a submenu of the sidebar's own filter menu: the one route to a project that
/// does not depend on its header being drawn.
///
/// Two things the pane deliberately no longer does make this necessary. A project with no
/// workspaces is not listed, and in the status view no project headers are listed either, so the
/// header's `+` and its settings gear have nowhere to live. Both are here, per project, with the
/// hidden ones included and marked, which also makes this the one route back to a project somebody
/// hid other than the switch below it.
///
/// A submenu rather than the popover this replaced. The popover needed a button of its own in the
/// status bar, and the owner's objection to the strip growing controls applies to that button as
/// much as it did to the segmented picker it sat beside.
struct SidebarProjectsMenu: View {
    var repos: [Repo]
    /// Raised to the sidebar, which posts for the create window, so every entry point behaves
    /// identically. See `SidebarView.presentCreate`.
    var onCreateWorkspace: (Repo) -> Void

    @Environment(AppModel.self) private var app
    @Environment(\.openWindow) private var openWindow

    var body: some View {
        Menu("All projects") {
            ForEach(repos) { repo in
                Menu(repo.hidden ? "\(repo.name) (hidden)" : repo.name) {
                    Button("New workspace") { onCreateWorkspace(repo) }
                    Button("Project settings…") {
                        openWindow(id: RepoSettingsWindow.id, value: repo.id)
                    }
                    Button(repo.hidden ? "Unhide project" : "Hide project") {
                        Task { await app.toggleHidden(repo) }
                    }
                }
            }
        }
        .disabled(repos.isEmpty)
    }
}
