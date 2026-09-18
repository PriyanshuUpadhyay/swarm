import SwiftUI
import SwarmCore

/// What the sidebar's filter control offers: which workspaces the pane lists, and whether it
/// lists the projects the owner has hidden.
///
/// A view of its own rather than a block inside `SidebarStatusBar`, for the reason
/// `ProjectMenuItems` is one: a menu written inside a body is a menu nothing can photograph, and
/// `MenuProbe --menu-part filter` builds this very view.
///
/// ## Two sections, because there are two nouns
///
/// The picker narrows WORKSPACES by their state, which is what this control has always done. The
/// toggle decides whether hidden PROJECTS are in the list, which is a different noun, so it goes
/// under a heading of its own rather than as a fourth row of the picker. Conductor puts the same
/// switch at the foot of its project list as a permanent row; the owner asked for it in the filter
/// instead, and a labelled section is what stops it reading as a fourth kind of workspace.
///
/// The toggle is a switch and not another picker row on purpose. The picker is one choice out of
/// three and only one of them can be true; showing hidden projects is orthogonal to all three,
/// and a tick that is not part of the same either/or has to look different from one that is.
struct SidebarFilterMenuItems: View {
    @Binding var filter: SidebarFilter
    @Binding var showsHiddenProjects: Bool
    /// How many projects are hidden right now, which the toggle says out loud. Passed in rather
    /// than read from the model, so the probe can photograph the menu for any number.
    var hiddenCount: Int
    /// Which shape the pane is in, as the stored string the preference holds.
    ///
    /// **Here rather than as a segmented control in the strip, and the owner asked for that in
    /// those words.** A picker in the status bar is two words of chrome under every project in the
    /// pane, permanently, for a choice somebody makes rarely; the filter is already the control
    /// that answers "why is the sidebar not what I expected", and this belongs with it. The strip
    /// goes back to the two icons it had.
    @Binding var grouping: String

    var body: some View {
        // The pane's shape leads, because it changes what the rows below the heading mean: the
        // filter narrows a list, and this decides what kind of list it is.
        Picker("Group by", selection: $grouping) {
            ForEach(SidebarGrouping.allCases, id: \.rawValue) { option in
                Label(option.title, systemImage: option.icon).tag(option.rawValue)
            }
        }
        .pickerStyle(.inline)

        // The picker's own label is the heading, rather than a `Section` around it. An inline
        // picker already draws its label as a section header, so wrapping it in a section drew
        // two headings above three rows, photographed and thrown away.
        Picker("Workspaces", selection: $filter) {
            ForEach(SidebarFilter.allCases, id: \.self) { option in
                Label(option.rawValue, systemImage: option.icon).tag(option)
            }
        }
        .pickerStyle(.inline)

        // No icon on this one, and not for want of trying: a menu draws a toggle's state as a
        // tick in the column an image would go in, so a `systemImage` here is simply not drawn.
        // The picker rows above keep theirs, because a picker draws both.
        Section("Projects") {
            Toggle(
                ProjectVisibility.toggleTitle(hiddenCount: hiddenCount),
                isOn: $showsHiddenProjects
            )
        }
    }
}
