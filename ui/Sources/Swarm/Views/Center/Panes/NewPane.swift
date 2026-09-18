import SwiftUI
import SwarmCore

/// The one way a new chat, terminal or browser is made for the centre column.
///
/// `BrowserTab` and `FileReview` next door are the same shape and exist for the same reason: a
/// menu should not have to know how a session is created or where a tab is stored in order to put
/// one in front of the user, and every route to a terminal should produce exactly the tab a
/// terminal normally is. A chat goes through `WorkspaceModel.createChat`, a terminal and a
/// browser through `CenterTabStore.add`, which is what the title bar's `+` menu already called.
///
/// Where the tab goes is the caller's business, not this one's. The `+` shows it in the pane the
/// user is in; the pane's context menu splits and shows it in the half that opens. That is the
/// whole difference between the two, so it is the only thing that is passed in.
@MainActor
enum NewPane {
    /// Makes a tool tab and hands it to `place`, or selects the sidebar row for a new chat.
    ///
    /// `place` is called after a tool exists rather than before, which is why it is a closure and
    /// not a return value. A chat is a sidebar row, so it selects that row instead of filling a
    /// workspace pane.
    ///
    /// `url` is only read for a browser. It is empty by default because a browser pane opened from
    /// a split has nowhere in particular to go: the address field is where somebody says. The
    /// strip's `+` passes the workspace's own dev server, which is what its setup and run scripts
    /// were told to bind.
    ///
    /// `title` is nothing for every menu that calls this, and something only when a caller knows
    /// what the pane is for: `pane_open` and `pane_split` carry the name an agent gave, because
    /// four tabs called Terminal are four a reader cannot tell apart. Nil takes the numbering each
    /// kind has always had, so no menu changed when this argument arrived.
    ///
    /// `directory` is only read for a terminal, and only a folder row in the inspector passes one.
    /// Empty is the worktree root, which is where every other route to a shell starts. See
    /// `FolderTerminal`.
    static func open(
        _ kind: PaneKind,
        in model: WorkspaceModel,
        url: String = "",
        title: String? = nil,
        directory: String = "",
        place: @escaping @MainActor (PaneContent) -> Void
    ) {
        switch kind {
        case .chat:
            Task { _ = await model.createChat(title: title) }

        // The shell itself is not started here. `ToolPaneView` settles the environment and the
        // port first, because both are baked into the process the moment it is forked.
        case .terminal:
            let tab = CenterTabStore.shared.add(
                kind: .terminal, workspaceID: model.workspace.id, title: title,
                directory: directory
            )
            place(.tool(tab.id))

        case .browser:
            let tab = CenterTabStore.shared.add(
                kind: .browser, workspaceID: model.workspace.id, url: url, title: title
            )
            place(.tool(tab.id))
        }
    }
}
