import SwarmCore

/// MCP splits are anchored to the conversation making the request. Keyboard focus can move
/// while that agent is thinking or while a new chat is being saved, so it cannot pick the target.
extension AppModel {
    func splitPaneForBridge(
        _ order: PaneOrder, axis: SplitAxis, anchor: PaneSplitAnchor, in workspaceID: WorkspaceID
    ) async -> PaneOutcome {
        guard let model = paneTarget(workspaceID) else { return .refused(Self.noWorkspaceForPane) }
        let tabs = WorkspaceTabsStore.shared
        let entries = tabs.entries(in: model)
        let snapshots = entries.map { tab in
            let layout = tabs.layout(of: tab)
            return PaneSplitAnchor.Tab(
                root: tab, layout: layout,
                contents: Dictionary(uniqueKeysWithValues: layout.panes.map {
                    ($0, tabs.content(of: $0, in: tab))
                })
            )
        }
        guard let destination = anchor.resolve(
            in: snapshots, selected: tabs.selectedTab(in: model, entries: entries)
        ) else {
            return .refused(
                anchor == .activePane
                    ? "There is no selected pane to split. Nothing was opened."
                    : "The chat making this request is not open in a tab. Nothing was opened."
            )
        }
        let original = tabs.content(of: destination.pane, in: destination.tab)

        let content: PaneContent
        if order.kind == .chat {
            guard await model.createChat(title: order.title) != nil else {
                return .refused("Swarm could not create the new chat. Nothing was opened.")
            }
            return .opened("Opened a new chat under its workspace.")
        } else {
            var opened: PaneContent?
            NewPane.open(order.kind, in: model, url: order.url ?? "", title: order.title) { opened = $0 }
            guard let opened else { return .refused("Swarm could not create the new pane. Nothing was split.") }
            content = opened
        }

        // Revalidate after the store await. A pane removed or repointed while creating the chat
        // must not turn this request into a split of unrelated content or a restored closed tab.
        guard paneTarget(workspaceID) === model,
              tabs.entries(in: model).contains(destination.tab),
              tabs.layout(of: destination.tab).contains(destination.pane),
              tabs.content(of: destination.pane, in: destination.tab) == original,
              tabs.split(tab: destination.tab, pane: destination.pane, axis: axis, showing: content) != nil else {
            return .refused("The target pane changed before it could be split. The new content is available as a separate tab.")
        }
        tabs.select(destination.tab, in: model)
        let placement = axis == .horizontal ? "to the right of" : "below"
        let target = anchor == .activePane ? "the selected pane" : "the chat making this request"
        return .opened("Opened a new \(order.kind.title) pane \(placement) \(target), inside the same tab.")
    }
}
