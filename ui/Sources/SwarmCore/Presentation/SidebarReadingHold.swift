import Foundation

/// The one workspace the status view keeps in Ready to read after it has been read.
///
/// Opening an unread workspace clears its flag straight away (`WorkspaceModel.onAppear`), and the
/// status view regrouped on that, so the row the owner had just clicked jumped to Idle under the
/// pointer while its transcript was still arriving. The owner asked for it to stay where it was
/// clicked for as long as it is the selected workspace, and to go to Idle when they leave it.
///
/// Only the grouping is held. The flag itself still clears on arrival, so the Dock badge and the
/// menu bar stop counting the workspace the moment it is on screen, which is the rule
/// `AppModel.markRead` argues for. The row's weight follows the flag too; what does not move is
/// the row.
///
/// Decided when the selection changes and not on every regroup, because the store write that
/// clears the flag lands a moment after the selection does. At the moment of the change the row
/// still says unread, and that is the reading this captures.
public enum SidebarReadingHold {
    /// The workspace to hold after the selection moves to `selection`.
    ///
    /// - Parameter current: what was held before. Kept while the same workspace stays selected,
    ///   so a selection change that re-fires for the same target cannot drop a hold whose flag has
    ///   already cleared.
    public static func next(
        selection: SidebarSelection,
        current: WorkspaceID?,
        workspaces: [Workspace]
    ) -> WorkspaceID? {
        guard case .workspace(let id) = selection else { return nil }
        if id == current { return current }
        let unread = workspaces.first { $0.id == id }.map(WorkspaceUnreadMark.isUnread) ?? false
        return unread ? id : nil
    }
}
