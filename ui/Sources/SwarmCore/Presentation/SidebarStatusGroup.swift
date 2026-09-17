import Foundation

/// The four things a workspace can be, when the pane is sorted by what it needs rather than by
/// where it lives.
///
/// The order of the cases is the order the sections are drawn in, and it is the argument this type
/// exists to make: **what you can act on comes before what is merely happening.** A finished turn
/// nobody has read is work waiting for a person, so it sits above an agent that is mid turn and
/// needs nothing from anybody. That is the one placement that surprised the owner enough to ask
/// about it, and it is deliberate.
///
/// Thirteen `WorkspaceStatus` cases collapse into four here, and the collapsing is the point. The
/// mark on a row says precisely what a workspace is; these sections say what to do about it, and
/// "draft pull request", "checks passed" and "no changes" are all the same answer: nothing, for now.
public enum SidebarStatusGroup: String, CaseIterable, Sendable, Hashable {
    /// Stopped until a person does something: a permission question, or a setup that failed.
    case needsYou
    /// A turn finished and nobody has read it.
    case readyToRead
    /// An agent has a turn open, or a worktree is still being cut.
    case working
    /// Everything else, which is most of the pane most of the time.
    case idle

    /// Which section a workspace belongs in, from the verdict its row's mark is already drawn from.
    ///
    /// Taking `WorkspaceStatus` rather than the workspace itself is what keeps one judgement in one
    /// place: whether an agent is running, whether GitHub has anything to say and which of the two
    /// wins are all decided by `WorkspaceStatus.resolve`, and a second copy of that precedence here
    /// is how the mark on a row and the section above it come to disagree.
    public static func of(_ status: WorkspaceStatus) -> Self {
        switch status {
        case .awaitingPermission, .setupFailed: .needsYou
        case .unread: .readyToRead
        case .running, .settingUp: .working
        // Every pull request state and both worktree states. A branch with changes, a draft, a
        // merge, red checks: all of them are things to look at when you choose to, and none of
        // them is the agent asking for you now. The mark on the row still tells them apart.
        case .merged, .closed, .conflicted, .checksFailing, .checksRunning, .checksPassed, .draft,
             .pullRequestOpen, .changed, .clean:
            .idle
        }
    }

    /// Which section a workspace belongs in, counting a turn nobody has read even when the row's
    /// mark says something else.
    ///
    /// `WorkspaceStatus.resolve` lets a pull request outrank unread, because the mark on a row is
    /// better spent on red checks or a merge than on a dot. Grouped by that verdict alone, every
    /// unread workspace with a pull request sat in Idle, and only once `gh` had answered for it,
    /// which is why it looked intermittent. The mark keeps the pull request; the section asks
    /// whether there is something to read. Needs you and Working still come first, because a
    /// question or a running turn is more urgent than output waiting.
    public static func of(_ status: WorkspaceStatus, unread: Bool) -> Self {
        let group = of(status)
        return group == .idle && unread ? .readyToRead : group
    }

    public var title: String {
        switch self {
        case .needsYou: "Needs you"
        case .readyToRead: "Ready to read"
        case .working: "Working"
        case .idle: "Idle"
        }
    }

    /// Whether this section can be folded away.
    ///
    /// Only `idle`, and only because of what it holds: a long tail of workspaces that are finished,
    /// parked or untouched, which is the one section that grows without anybody doing anything. The
    /// three above it are all short by construction and are the reason the pane is in this shape at
    /// all, so folding one would be hiding exactly what was asked for.
    public var isFoldable: Bool { self == .idle }

    /// Whether the section ranks its rows by when they were last touched rather than keeping the
    /// order the projects are drawn in.
    ///
    /// Only `idle`, and it is the section that needs it. The three above it are short, urgent and
    /// read top to bottom; this one is the long tail everything falls into when it is done, so a
    /// row that has just arrived from Ready to read would otherwise land somewhere in the middle of
    /// twenty others, which is the same as landing nowhere. Most recently touched first puts what
    /// you have just finished with at the top, where you can still find it.
    public var ranksByRecency: Bool { self == .idle }

    /// How many rows `idle` has to reach before it is worth offering to fold it.
    ///
    /// Below this the control is furniture on a section you can read in one glance. It is a count
    /// rather than a height because the pane's row height is not ours to set: see `SidebarMetrics`.
    public static let foldThreshold = 4
}

/// The pane's sections, in order, with the workspaces in each.
///
/// A row goes wherever its state says, including the selected one. It used to be held in the
/// section it was selected in, so that reading a turn or watching one end would not move it out
/// from under the pointer; with the move animated, a row travelling to its new section reads as
/// the state changing rather than as the list jumping, and the hold only left a running row sitting
/// under Idle until the selection moved.
public struct SidebarStatusListing: Equatable, Sendable {
    public struct Section: Equatable, Sendable {
        public var group: SidebarStatusGroup
        public var workspaces: [Workspace]

        public init(group: SidebarStatusGroup, workspaces: [Workspace]) {
            self.group = group
            self.workspaces = workspaces
        }
    }

    public var sections: [Section]

    public init(sections: [Section]) {
        self.sections = sections
    }

    public static let empty = SidebarStatusListing(sections: [])

    /// Builds the sections.
    ///
    /// - Parameter workspaces: every workspace the pane is drawing, in the order projects and their
    ///   rows are already drawn in. That order is kept inside each section, so a pane sorted by
    ///   status still reads project by project inside a section rather than in an order nobody
    ///   chose.
    /// - Parameter status: the verdict for one workspace, which only the app layer can answer
    ///   because it alone knows what is running and what GitHub said.
    /// - Parameter holding: a workspace to list as unread although its flag has cleared. See
    ///   `SidebarReadingHold`.
    public static func build(
        workspaces: [Workspace],
        holding: WorkspaceID? = nil,
        status: (Workspace) -> WorkspaceStatus
    ) -> SidebarStatusListing {
        var byGroup: [SidebarStatusGroup: [Workspace]] = [:]
        for workspace in workspaces {
            let unread = workspace.unread || workspace.id == holding
            byGroup[SidebarStatusGroup.of(status(workspace), unread: unread), default: []].append(workspace)
        }

        return SidebarStatusListing(
            sections: SidebarStatusGroup.allCases.compactMap { group in
                guard let rows = byGroup[group], !rows.isEmpty else { return nil }
                return Section(group: group, workspaces: group.ranksByRecency ? byRecency(rows) : rows)
            }
        )
    }

    /// Most recently touched first, with the order they arrived in as the tiebreak.
    ///
    /// `lastActivityAt` rather than a "when was this read" of its own, because it is the column
    /// that already moves when anything happens to a workspace: a turn ending writes it, and
    /// reading that turn is the thing that immediately follows. A row leaving Ready to read
    /// therefore lands at the top of Idle, which is what it is for.
    private static func byRecency(_ workspaces: [Workspace]) -> [Workspace] {
        workspaces.enumerated()
            .sorted { lhs, rhs in
                if lhs.element.lastActivityAt != rhs.element.lastActivityAt {
                    return lhs.element.lastActivityAt > rhs.element.lastActivityAt
                }
                return lhs.offset < rhs.offset
            }
            .map(\.element)
    }
}
