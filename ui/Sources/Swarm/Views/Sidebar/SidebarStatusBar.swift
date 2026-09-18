import SwiftUI
import SwarmCore

/// The strip pinned to the bottom of the sidebar: Home, New and Ask Swarm (`SidebarDock`), the two
/// controls that narrow or explain the list, and a line the pane borrows when it has something to
/// say about itself.
///
/// The filter lives down here rather than in a header, which is where Xcode and Finder put the
/// controls that narrow a source list.
struct SidebarStatusBar: View {
    @Environment(AppModel.self) private var app

    @Binding var filter: SidebarFilter
    /// Raised to the sidebar so the projects submenu's New workspace posts for the create window
    /// through the same door every other entry point uses. See `SidebarView.presentCreate`.
    var onCreateWorkspace: (Repo) -> Void = { _ in }
    /// New workspace with no project named, which the create window asks for.
    var onNewWorkspace: () -> Void = {}
    var onStartProject: () -> Void = {}
    /// Whether the projects the owner has hidden are in the list. A preference rather than this
    /// window's state, which is why it is `@AppStorage` here and in `SidebarView` rather than a
    /// second `@State` passed down. See `ProjectVisibility.showsHiddenKey`.
    @AppStorage(ProjectVisibility.showsHiddenKey) private var showsHiddenProjects = false
    /// Something the pane has to say about itself, for a moment, in place of the running count.
    /// The sidebar owns both the sentence and how long it lasts. See `SidebarView.move(from:to:)`.
    var note: String?

    /// Which shape the pane is in. The same key `SidebarView` binds, rather than a value passed
    /// down, for the reason `showsHiddenProjects` is: two views reading one preference cannot
    /// disagree about it. See `SidebarGrouping`.
    @AppStorage(SidebarGrouping.storageKey) private var storedGrouping = SidebarGrouping.status.rawValue

    @State private var isShowingLegend = false
    @State private var isFilterHovered = false
    @State private var isLegendHovered = false

    var body: some View {
        VStack(spacing: 0) {
            Hairline()

            HStack(spacing: Metrics.spacingSmall) {
                SidebarDock(onNewWorkspace: onNewWorkspace, onStartProject: onStartProject)

                noteLabel

                Spacer(minLength: Metrics.spacingSmall)

                Menu {
                    SidebarFilterMenuItems(
                        filter: $filter,
                        showsHiddenProjects: $showsHiddenProjects,
                        hiddenCount: ProjectVisibility.hiddenCount(app.repos),
                        grouping: $storedGrouping
                    )
                    SidebarProjectsMenu(repos: app.repos, onCreateWorkspace: onCreateWorkspace)
                    // The same words the New button's menu uses, so one action is not named two
                    // ways inside one pane. See `SidebarDock`.
                    Button("New project…", action: onStartProject)
                } label: {
                    controlLabel(
                        "Filter the sidebar",
                        systemImage: filter == .all ? "line.3.horizontal.decrease" : filter.icon,
                        isHovered: isFilterHovered
                    )
                    .foregroundStyle(isDefaultView ? Palette.textSecondary : Palette.accent)
                }
                // Icon only visually, but the label is still there for VoiceOver and Voice
                // Control, and the tint says whether the pane is showing something other than its
                // default set. Showing hidden projects lights it as much as narrowing the
                // workspaces does, because both answer the question somebody asks when the pane
                // is not what they expected: is this control doing something.
                .menuStyle(.button)
                .buttonStyle(.plain)
                .menuIndicator(.hidden)
                .fixedSize()
                .onHoverChange { isFilterHovered = $0 }
                .help("Filter the sidebar")
                .accessibilityValue(filterValue)

                Button {
                    isShowingLegend.toggle()
                } label: {
                    controlLabel(
                        "What the sidebar glyphs mean",
                        systemImage: "questionmark.circle",
                        isHovered: isLegendHovered || isShowingLegend
                    )
                    .foregroundStyle(Palette.textSecondary)
                }
                .buttonStyle(.plain)
                .onHoverChange { isLegendHovered = $0 }
                .help("What the sidebar glyphs mean")
                .popover(isPresented: $isShowingLegend, arrowEdge: .top) {
                    SidebarLegend()
                }

                // **No Settings cogwheel.** There was one here, and it was the one control in this
                // strip that duplicated something every Mac user already knows: Command-comma, and
                // the Swarm menu. The other two earn their place because neither is reachable any
                // other way, and a third glyph beside them spent the strip's width saying what the
                // menu bar says for free.
            }
            .padding(.horizontal, Metrics.spacingSmall)
            .frame(height: Metrics.barHeight)
        }
        // Let the native sidebar ground continue behind these controls without a second material.
    }

    // The native accessory style did not show a hover fill here. Both labels own the square
    // target so the menu and button draw and respond over the same area.
    private func controlLabel(_ title: String, systemImage: String, isHovered: Bool) -> some View {
        Label(title, systemImage: systemImage)
            .labelStyle(.iconOnly)
            .font(Typo.label)
            .frame(width: Metrics.rowHeight, height: Metrics.rowHeight)
            .contentShape(Rectangle())
            .background(
                isHovered ? Palette.hover : .clear,
                in: RoundedRectangle(cornerRadius: Metrics.cornerSmall)
            )
    }

    /// Whether the pane is showing what it shows when nothing has been asked of it.
    private var isDefaultView: Bool {
        filter == .all && !showsHiddenProjects
    }

    /// Both halves of the control, in words, since the glyph can only show one of them.
    private var filterValue: String {
        showsHiddenProjects ? "\(filter.rawValue), hidden projects showing" : filter.rawValue
    }

    /// A readout, so it is set as text rather than as the filled `Chip` it used to be. A pill in a
    /// status bar reads as a control that does nothing when clicked, and this one sits beside two
    /// controls that really are clickable.
    ///
    /// **The running count went, and the line stayed.** It read "Idle" most of the time and
    /// "2 running" the rest, and the list above says both already. What is left is the note: a drag
    /// that could not land where it was let go borrows this line rather than raising anything of
    /// its own. With nothing to say the strip is the two controls alone, which is what it was
    /// before the grouping picker briefly joined them and what the owner asked to have back.
    @ViewBuilder
    private var noteLabel: some View {
        if let note {
            Label(note, systemImage: "arrow.uturn.backward")
                .font(Typo.caption)
                .foregroundStyle(Palette.textSecondary)
                .padding(.leading, Metrics.spacing)
                .lineLimit(1)
                .accessibilityLabel(note)
        }
    }
}
