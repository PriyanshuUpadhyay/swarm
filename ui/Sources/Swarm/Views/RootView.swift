import AppKit
import SwiftUI
import SwarmCore

/// The window: a real `NavigationSplitView` with a real toolbar.
///
/// This used to be a hand-rolled `HStack` with its own drag handles, which is precisely why the
/// window had no title bar, no toolbar, an opaque sidebar and a hard divider running straight
/// through the traffic lights. `NavigationSplitView` hands all of that back to AppKit: the
/// translucent sidebar material, the sidebar toggle, traffic light placement, unified toolbar
/// integration and remembered column widths. The centre column and the inspector are an AppKit
/// `NSSplitViewController`, for the reason spelled out on `DetailSplitViewController`.
///
/// The columns themselves are `SidebarView` and `DetailColumn`, and the toolbar is
/// `SwarmWindowToolbar`. What is left here is only what belongs to the window as a whole: the
/// split view, the inspector, the archive confirmation and the alert.
struct RootView: View {
    @Environment(AppModel.self) private var app
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @Environment(\.openWindow) private var openWindow

    @Bindable private var projectSetup = ProjectSetup.shared
    @Bindable private var closeSession = CloseSessionAlert.shared
    @Bindable private var closeSwarmAgent = SwarmAgentCloseAlert.shared
    @Bindable private var setupRun = SetupRunAlert.shared

    @State private var columnVisibility: NavigationSplitViewVisibility = .all

    private var sidebarTintOpacity: Double {
        let preference = ColourThemePreference.shared
        return preference.glass.tintOpacity(maximum: preference.choice.surfaces.glassTintOpacity ?? 0.4)
    }

    var body: some View {
        @Bindable var app = app

        return windowWiring(
            NavigationSplitView(columnVisibility: $columnVisibility) {
                SidebarView()
                // Replaced by `SidebarToggleButton`, so it can share glass with New.
                .toolbar(removing: .sidebarToggle)
                .environment(\.isSidebarFolded, columnVisibility == .detailOnly)
                .scrollContentBackground(.hidden)
                .background {
                    Group {
                        if ColourThemePreference.shared.glass == .off {
                            Palette.surfaceSunken
                        } else {
                            SidebarMaterial(tint: Palette.sidebarGlassTint, opacity: sidebarTintOpacity)
                        }
                    }
                    .ignoresSafeArea()
                }
                // The rule down the sidebar's trailing edge.
                //
                // `NavigationSplitView` draws none: measured across the boundary, the sidebar's last
                // pixel is followed directly by the centre column's first, in both appearances. That
                // is survivable while both columns are the system's own white, and it is not once
                // they are two steps of a ramp, because then the two panes simply run into each other.
                .overlay(alignment: .trailing) {
                    Hairline(axis: .vertical).ignoresSafeArea(edges: .top)
                }
                // The ceiling is not always the reserve. On a display too narrow to hold all
                // three panes at their minimums, the sidebar is the one that gives, because it is
                // a list of rows that truncate where the other two hold a transcript and a diff
                // that do not. `WindowWidths.sidebarMaximum` is that decision, and it answers with
                // the plain 420 reserve on every display that can afford it, which is every Mac
                // one. A nil is a screen with no room for a sidebar at all, and the column folds.
                .navigationSplitViewColumnWidth(
                    min: SwarmApp.sidebarMinimumWidth,
                    ideal: Metrics.sidebarWidth,
                    max: sidebarCeiling ?? SwarmApp.sidebarMinimumWidth
                )
            } detail: {
                // An `NSSplitViewController`, not `.inspector()` and not `HSplitView`.
                //
                // `.inspector` cannot be used in this window. Presented, it throws "more Update
                // Constraints in Window passes than there are views in the window" out of the display
                // cycle, from a loop that runs through SwiftUI's own `SplitViewChildController`
                // reacting to its hosting view's min and max size. Verified again on this branch:
                // swapping the split below for `.inspector()` kills the window during a resize.
                //
                // `HSplitView` was the next attempt and it did not crash, but its divider is drawn
                // down the whole bounds of the split view while the SwiftUI content inside each pane
                // respects the safe area, so under a unified toolbar a hard rule crossed the title.
                // An `NSSplitViewController` is a view controller container rather than a view, so its
                // panes and its divider share one safe area and the rule starts under the toolbar.
                DetailSplitView(
                    app: app,
                    isInspectorPresented: isInspectorPresented,
                    animated: !reduceMotion
                )
                    .background { Palette.sidebar.ignoresSafeArea() }
                    .toolbar {
                        SwarmWindowToolbar(
                            app: app,
                            isSidebarFolded: columnVisibility == .detailOnly
                        )
                    }
                    // Said here as well as under `navigationTitle` below, and deliberately.
                    //
                    // The title is declared on the split view and the toolbar is declared on this
                    // column, so the two are not the same view, and which of them resolves the default
                    // title item is not a thing reading the interface settles. The window came up
                    // wearing its name twice once already. Both, until a picture says which one did it.
                    .toolbar(removing: .title)
                    // **There is no `.searchable` here any more, and the reason is a defect rather
                    // than a matter of taste.** The window carried an `NSSearchToolbarItem`, 206
                    // points of the system's glass with the system's drop shadow, packed against
                    // the toolbar's trailing end by a flexible spacer and sharing an edge with
                    // nothing. That was survivable. What was not is that typing one character into
                    // it set the selection to Home: a search that takes the page away from you is
                    // a search people stop using, and getting back meant clearing the field and
                    // clicking the workspace again.
                    //
                    // The same search is a panel now, opened on Cmd+K and from the magnifying
                    // glass in the title bar, and it moves nothing: no selection, no scroll, no
                    // column. Nothing it could reach was taken away with the field, which is the
                    // whole point of relocating it rather than replacing it. See `SearchPanelView`
                    // for the card and `SwarmWindowToolbar` for its button.
            }
            // As well as heading the toolbar (see SwarmApp), the title names the window in the
            // Window menu and in Mission Control, so it is worth setting.
            //
            // It takes an automatic rename straight, with no reveal. A window title is also its entry
            // in the Window menu and its label in Mission Control, and neither of those can be
            // animated: what they would show is one arbitrary frame of a scramble, which is a window
            // called `xqbn hgue` in a menu the user is reading to find it by name.
            // `menuWorkspace` rather than `selectedWorkspace`, so an archived workspace being read
            // names the window as well. It is still not what the inspector keys on, below: naming a
            // window costs nothing, and showing a diff for a worktree that is gone does not.
            .containerBackground(.clear, for: .window)
            .navigationTitle(app.menuWorkspace?.name ?? "Swarm")

            // And then removed from the toolbar again, because `WindowTitleControl` draws the name
            // itself and the window came up wearing it twice.
            //
            // The two titles are not one title drawn twice, they are two different mechanisms, which
            // is why hiding one did not hide the other. `NSWindow.titleVisibility`, which `WindowChrome`
            // sets, governs the title AppKit draws. The line above contributes a title item of
            // SwiftUI's own to the window toolbar, and SwiftUI owns that one: it re-resolves it with
            // the toolbar, so nothing set on the window from the side can take it away.
            // `ToolbarDefaultItemKind.title` is the switch for it, and it is the only one.
            //
            // The title itself stays. It is what the Window menu, Mission Control and the Dock read,
            // and the comment above is the reason it is set at all.
            .toolbar(removing: .title)

            // Marks this scene as the main window, so the menu items that act on a workspace grey out
            // while Settings or a project settings window is key. See `MainWindowFocus`.
            .focusedSceneValue(\.isMainWindowFocused, true)

            // A display with no room for a sidebar folds it rather than clipping it. The other two
            // panes cannot fold and cannot truncate, so this is the only pane there is a decision
            // to take about. It stays folded when the room comes back: unfolding a column the user
            // has not asked for is a window rearranging itself behind them, and the toggle and its
            // Command key are both a keystroke away. See `WindowWidths.sidebarMaximum`.
            .onChange(of: sidebarCeiling == nil, initial: true) { _, folds in
                if folds { columnVisibility = .detailOnly }
            }

            // Bottom trailing, out of the way of the sidebar and of the composer's send button.
            .overlay(alignment: .bottomTrailing) {
                if let notice = app.notice {
                    NoticeBanner(notice: notice) { app.notice = nil }
                        .transition(.opacity)
                }
            }
            .animation(reduceMotion ? nil : Motion.pane, value: app.notice)

            .task { await app.bootstrap() }
            // Debug builds can raise the search panel from a capture flag. The panel is otherwise
            // reachable only by a key equivalent and a glyph. See `SearchPanelModel.presentIfRequested`.
            .task { presentSearchPanelIfRequested() }
            // The offer to turn a folder into a repository. Presented here rather than at each of the
            // controls that can raise it, because there are five of them across two windows and they
            // all reach it through `AppModel.addRepository`.
            .sheet(item: $projectSetup.request.on(.main)) { request in
                ProjectSetupSheet(request: request) { path in
                    Task { await app.finishProjectSetup(path) }
                }
            }
            // This one stays on the window rather than moving to the row that asked for it. It is not
            // presented by a click: `AppModel.archive` runs a git safety check first and only refuses
            // afterwards, and it refuses identically whether the request came from a sidebar context
            // menu, the Workspace menu or a keyboard shortcut. There is no single control it could
            // animate out of, and anchoring it to the sidebar row would lose the refusals that arrive
            // for the selected workspace from the menu bar.
            //
            // The title is fixed rather than "Archive <name>?". Workspace names here are whole
            // sentences ("Show me the technolgies-used-in-this-project"), and a title built from one
            // wraps to two lines of bold text that the eye reads as the warning. The name belongs in
            // the message, where a long one costs nothing.
            .confirmationDialog(
                "Archive this workspace?",
                isPresented: $app.pendingArchive.isPresent(),
                titleVisibility: .visible,
                presenting: app.pendingArchive
            ) { request in
                // The request comes from `presenting:` and is handed straight to the model. Reading
                // `app.pendingArchive` back inside the action is what made Archive do nothing at all:
                // dismissing the dialog clears it before the action's task ever reaches the main
                // actor. See `AppModel.confirmArchive`.
                // The role follows the severity rather than the action, because the action is the
                // same either way. A worktree carrying nothing but a `.env` and a folder of generated
                // types gets a plain button: see `ArchiveRequest.Severity`.
                Button(
                    request.confirmLabel, role: request.isDestructive ? .destructive : nil
                ) { confirmArchive(request) }
                // No `.keyboardShortcut(.defaultAction)` on the cancel button, and that is not an
                // oversight. It used to be there, to keep Return off the destructive answer, and it
                // did that by REPLACING the cancel button's own key binding. A `.cancel` role button
                // is what Escape is wired to, so moving Return onto it took Escape off it, and no
                // destructive confirmation in the app could be waved away with the key every Mac user
                // reaches for. Verified on this build: with the modifier gone Escape dismisses, and
                // Return does nothing at all, because a confirmation dialog has no default button
                // unless one is named. Both halves of the rule hold, and the safe answer keeps the
                // key it is supposed to have.
                Button(request.cancelLabel, role: .cancel, action: app.cancelPendingArchive)
            } message: { request in
                // Naming what disappears, rather than asking "are you sure?". Written by
                // `ArchiveRequest` in the core, where it can be tested.
                Text(request.message)
            }
            // The question asked before a session that is still working is closed. On the window for
            // the reason the archive confirmation above is: it is raised from the tab strip's close
            // button and from Cmd+W in the menu bar, and there is no one control both of those could
            // animate out of. See `CloseSessionAlert`.
            .confirmationDialog(
                closeSession.request?.title ?? "",
                isPresented: $closeSession.request.isPresent(),
                titleVisibility: .visible,
                presenting: closeSession.request
            ) { request in
                // The wording answers the question that was asked, which is not always the same
                // question: a conversation can be mid turn, or the only one its workspace has, or
                // both. See `SessionClosure`.
                Button(request.cost.confirmTitle, role: .destructive) { closeSession.confirm() }
                // Escape keeps the conversation. See the archive confirmation above for why no cancel
                // button in this app carries `.keyboardShortcut(.defaultAction)`.
                Button(request.cost.cancelTitle, role: .cancel) { closeSession.cancel() }
            } message: { request in
                Text(request.message)
            }
            .confirmationDialog(
                "Close this swarm agent?",
                isPresented: $closeSwarmAgent.request.isPresent(),
                titleVisibility: .visible,
                presenting: closeSwarmAgent.request
            ) { _ in
                Button("Close agent", role: .destructive) { closeSwarmAgent.confirm() }
                Button("Keep agent", role: .cancel) { closeSwarmAgent.cancel() }
            } message: { request in
                Text("Swarm will close \(request.tab.swarmAgent?.rawValue ?? "this agent") and remove its tab.")
            }
            // The question asked before a setup script runs. On the window because the three controls
            // that raise it are two menus and a transcript row, and a `Commands` body is not a view
            // and can present nothing. See `SetupRunAlert`.
            .confirmation($setupRun.request) { request in
                Confirmation(
                    title: request.question.title,
                    message: request.question.message,
                    confirmLabel: request.question.confirmLabel,
                    cancelLabel: request.question.cancelLabel
                )
            } onConfirm: { request in
                request.model.runSetupAgain()
            }
            // A single OK that does nothing but dismiss is the system default, so the actions builder
            // is deliberately empty rather than spelling one out.
            .alert(
                app.alert?.title ?? "",
                isPresented: $app.alert.isPresent(),
                presenting: app.alert
            ) { _ in
            } message: { alert in
                Text(alert.message)
            }
            .onReceive(OpenWorkspaceNotification.publisher()) { id in
                // Through `open(workspaceID:)` rather than straight into the selection, so an id that
                // has since been archived opens its transcript instead of landing on Home with no
                // explanation. See `AppModel.open(workspaceID:)`.
                Task { await app.open(workspaceID: id) }
            }
            // Shift+Cmd+F, and Cmd+F where nothing in front can find, are handled in
            // `windowWiring` below, and they open the panel rather than putting a keyboard
            // anywhere in the window.
        )
    }

    /// Debug builds only. In a method of its own so `body` carries one call rather than a
    /// conditional compilation block, which the type checker in there can do without.
    private func presentSearchPanelIfRequested() {
        #if DEBUG
        SearchPanelModel.shared.presentIfRequested(app: app)
        #endif
    }

    /// The window's notification wiring, in a method rather than in `body`.
    ///
    /// Not tidiness. `body` was one chain of forty-odd modifiers, and the new-project sheet that
    /// used to be among them took it past the type checker's budget: the build fails with "unable
    /// to type-check this expression in reasonable time", which names no cause and points at
    /// whichever line the solver happened to give up on. A method has a signature of its own to
    /// solve against, so the two halves are solved separately.
    ///
    /// A method and not a `ViewModifier`, because half of these handlers write this view's own
    /// `@State` and `@FocusState`, and a modifier is a separate type that can see neither.
    private func windowWiring(_ content: some View) -> some View {
        content
        // The search panel, over the whole window rather than over one column: it reaches every
        // workspace on the Mac and the full text of every transcript, so it belongs to the window.
        .searchPanel(app: app)
        // Shift+Cmd+F, and Cmd+F wherever nothing in front can find. Both have always meant "the
        // whole search", and both open the panel on the Transcripts chip, because what neither of
        // them can be is find-in-place: that is the pane's own and stays there.
        //
        // **Nothing here navigates any more.** This used to set the selection to Home and put the
        // keyboard in the toolbar's field, which is what took the reader off the conversation they
        // were in. The panel opens over the window and closes leaving it exactly where it was.
        .onReceive(NotificationCenter.default.publisher(for: .swarmFocusSearch)) { _ in
            SearchPanelModel.shared.open(scope: .transcripts, app: app)
        }
        // The scope settles when Home's own query moves, because that is where the two sets of
        // chips cross. See `HomeScope.settle`. It no longer navigates: the only thing that writes
        // this query now is the panel's "search Home for this" row, which is already on its way to
        // Home, and clearing it is how somebody gets back to the list they were reading.
        .onChange(of: app.homeFilter.query) { old, new in
            let was = !WorkspaceSearch.needle(old).isEmpty
            let now = !WorkspaceSearch.needle(new).isEmpty
            guard was != now else { return }
            app.homeFilter.scope = HomeScope.settle(app.homeFilter.scope, searching: now)
        }
        .onReceive(NotificationCenter.default.publisher(for: .swarmToggleSidebar)) { _ in
            toggleSidebar()
        }
        .onReceive(NotificationCenter.default.publisher(for: .swarmOfferProjectSetup)) { note in
            guard let path = note.object as? String else { return }
            Task { await app.addRepository(at: path) }
        }
        // Opening this window is otherwise a menu item or a gear on a row, neither of which a
        // capture run can press, which left the project settings window with no way of being
        // looked at at all. Named by project, or the first one.
        .onReceive(NotificationCenter.default.publisher(for: .swarmOpenRepoSettings)) { note in
            let named = note.object as? String
            let repo = app.repos.first { $0.name == named } ?? app.repos.first
            guard let repo else { return }
            openWindow(id: RepoSettingsWindow.id, value: repo.id)
        }
        // Debug builds only, and it draws nothing on its own: it is how a capture run gets the
        // window into the state the two busy signals are for. See `Snapshot`.
        .acceptsCaptureRunningState(app)
        .acceptsCaptureNotice(app)
        // A window now rather than a sheet on this one, so this is the same translation the create
        // window's post gets in the handler below. The four controls that ask still post, because
        // a `Commands` body cannot reach `openWindow` and the other three have always gone through
        // one door. What that window does once its button is pressed is its own, which is why
        // nothing is parked here waiting for it to come down: see `StartProjectView.finish`.
        .onReceive(NotificationCenter.default.publisher(for: .swarmNewProject)) { _ in
            openWindow(id: StartProjectWindow.id)
        }
        // The post is still the one door every control goes through, and what it opens is a
        // window now rather than a sheet on this one. The translation is here because this is
        // where the post was already being received and because `openWindow` needs a view: the
        // window itself is `CreateWorkspaceWindow`, and it is keyed by project, so asking twice
        // for the same project brings the first one forward with its draft still in it.
        // File, New Ask Swarm Conversation adds a conversation and keeps the existing ones, and
        // opens the panel to show it.
        .onReceive(NotificationCenter.default.publisher(for: .swarmNewAskConversation)) { _ in
            AskPanelModel.shared.open(app: app)
            Task { await app.ask.newConversation() }
        }
        .onReceive(NotificationCenter.default.publisher(for: .swarmNewWorkspace)) { note in
            if note.userInfo?[Notification.swarmPullRequestKey] as? Bool == true {
                CreateWorkspaceOpening.shared.askForPullRequest()
            }
            openCreateWindow(in: note.object as? Repo)
        }
        // Makes the workspace the create window's terminal mode makes, for a capture run, which
        // cannot choose a mode or press a button. It goes through `createWorkspace` exactly as that
        // window does, sea and all, so what is photographed is the real workspace rather than a
        // hand-built row that looks like one. Debug builds only, through the same flag family as
        // `--create-sheet`.
        .onReceive(NotificationCenter.default.publisher(for: .swarmStartTerminalWorkspace)) { note in
            let named = note.object as? String
            let repo = app.repos.first { $0.name == named } ?? app.repos.first
            guard let repo else { return }
            Task { await app.createWorkspace(in: repo, prompt: "", opensWith: .terminal) }
        }
    }

    private func toggleSidebar() {
        withAnimation(reduceMotion ? nil : Motion.pane) {
            columnVisibility = columnVisibility == .detailOnly ? .all : .detailOnly
        }
    }

    /// The inspector answers one question, what this workspace's agent changed, so on Home it has
    /// nothing to say. It used to sit there as a 380pt column holding a single "No workspace
    /// selected" glyph, and because its divider hairline is almost invisible on white that glyph
    /// read as a stray mark floating in the middle of the window. Hiding it also hands those
    /// points back to Home's list, which is what keeps a workspace name, its diff counts and its
    /// age on one line without truncating any of them.
    ///
    /// It is the model's, because the window's own minimum width depends on the same answer and
    /// `SwarmApp` cannot read a private computed property on a view. See `AppModel`.
    private var isInspectorPresented: Bool { app.isInspectorPresented }

    /// What the first column may be dragged out to on the display this window is on, or nil when
    /// there is no room for one at all. See `WindowWidths.sidebarMaximum`.
    ///
    /// `NSScreen.main` is the screen holding the key window, which is this one whenever anybody is
    /// dragging anything. It is read rather than observed, so moving the window to a narrower
    /// display leaves this a redraw behind; the consequence of being late is a sidebar that could
    /// have been dragged 98 points wider than it was, on a display Swarm is unlikely to meet, and
    /// the consequence of not reading it at all is a clipped window on that display.
    private var sidebarCeiling: CGFloat? {
        SwarmApp.widths.sidebarMaximum(
            sharing: NSScreen.main?.visibleFrame.width ?? .greatestFiniteMagnitude,
            withInspector: isInspectorPresented
        )
    }

    // MARK: - Actions

    /// Opens the create window, on the project that was asked for, or else on the project a
    /// workspace was last started in, or else on the one the window is already looking at.
    ///
    /// The project is resolved here rather than inside the window, because a window keyed by
    /// project has to be given one before it exists. A machine with no projects at all opens it
    /// with none, which is the empty state that offers to add one; every control that could ask
    /// is disabled or diverted in that state anyway.
    ///
    /// The last project started in wins over the selection, which is what the owner asked for.
    /// The sidebar's `+` still names its project outright, and that is never second-guessed.
    private func openCreateWindow(in repo: Repo?) {
        if let repo { return openWindow(id: CreateWorkspaceWindow.id, value: repo.id) }
        Task {
            let target = await app.recentProjects().first
                ?? app.selectedWorkspace.flatMap(app.repo(for:))
                ?? app.repos.first
            guard let target else { return openWindow(id: CreateWorkspaceWindow.id) }
            openWindow(id: CreateWorkspaceWindow.id, value: target.id)
        }
    }

    private func confirmArchive(_ request: ArchiveRequest) {
        Task { await app.confirmArchive(request) }
    }
}

extension Notification.Name {
    // The channel that opens a workspace is `OpenWorkspaceNotification`, in the core, and it keeps
    // its own name private so nothing can post an id down it untyped. These two carry no id and
    // are only ever posted by views, so they live here.
    static let swarmToggleSidebar = Notification.Name("swarm.toggleSidebar")
    /// Opens the search panel on the Transcripts chip. Posted by the Edit menu's Search item and
    /// by Cmd+F falling through, both of which mean "the whole search" rather than find-in-place.
    ///
    /// The name is older than the panel and is kept rather than churned: it is the same door, and
    /// what it used to open was a field in the toolbar. It no longer puts a keyboard anywhere in
    /// the window, which is the defect the panel exists to fix.
    static let swarmFocusSearch = Notification.Name("swarm.focusSearch")
    /// Asks for the create window. Still a notification rather than an `openWindow` at each of
    /// the four controls that can ask, because which project is meant depends on what this window
    /// has selected, and because the four have always behaved identically by going through one
    /// door. See `openCreateWindow`.
    static let swarmNewWorkspace = Notification.Name("swarm.newWorkspace")
    /// File, New Ask Swarm Conversation. The same act the toolbar's glyph performs, posted rather
    /// than called, because `RootView` owns the flag that raises it and starting fresh archives
    /// the conversation it replaces: one writer, whichever control was pressed.
    static let swarmNewAskConversation = Notification.Name("swarm.newAskConversation")
    /// Opens the window that starts a project. A notification for the same reason the create
    /// window is opened by one: the sidebar's `+`, Home's empty state, the toolbar and a
    /// `Commands` body can none of them reach `openWindow`, and this is where the receiver that
    /// can has always been. The name is the old one twice over, because what it raises is still
    /// the same surface: `StartProjectView` absorbed the second door rather than replacing the
    /// first, and it stopped being a sheet without changing what it asks.
    static let swarmNewProject = Notification.Name("swarm.newProject")
    /// Posted only by `Snapshot`, and only in a debug build. See the handler above.
    static let swarmStartTerminalWorkspace = Notification.Name("swarm.startTerminalWorkspace")
    /// The Workspace menu's Rename, aimed at whichever list is drawing that row.
    ///
    /// A notification rather than a flag on `AppModel`, for the same reason the create window is
    /// one: the field belongs to a row inside a list, the list owns the one field that can be open
    /// at a time, and a menu item cannot reach into either. Both lists listen, and each ignores a
    /// workspace it is not drawing, so the post can be made without knowing which is on screen.
    static let swarmRenameWorkspace = Notification.Name("swarm.renameWorkspace")
    /// The File menu's Rename Tab, aimed at the strip that owns the field.
    ///
    /// The same shape as the workspace rename above and for the same reason: the field belongs to
    /// a tab inside a strip, the strip owns the one field that can be open at a time, and a menu
    /// item can reach neither. It carries no id, because unlike the two workspace lists there is
    /// only ever one strip on screen and it renames the tab it has selected.
    static let swarmRenameTab = Notification.Name("swarm.renameTab")
}

extension Notification {
    /// Whether a `swarmNewWorkspace` post wants the window opened on the pull request route. Absent
    /// on every other post, which is the ordinary new branch opening.
    static let swarmPullRequestKey = "swarm.newWorkspace.pullRequest"
    /// Which workspace a `swarmRenameWorkspace` post is about, as its raw id.
    static let swarmWorkspaceIDKey = "swarm.workspaceID"
}
