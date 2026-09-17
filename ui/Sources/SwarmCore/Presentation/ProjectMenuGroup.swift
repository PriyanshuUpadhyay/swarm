import Foundation

/// The sections of the create window's project picker: the projects most recently started in,
/// then every project alphabetically.
///
/// **No visible and hidden split.** It used to be two groups by `Repo.hidden`, which made the
/// owner read past every visible project to reach a hidden one he had just been working in.
/// Hiding is about the sidebar (see `ProjectVisibility`); what this picker is asked is "which
/// project", and recency answers that better.
///
/// A project can appear in both sections, so a row is identified by its section as well as its
/// project. See `Choice`.
public struct ProjectMenuGroup: Identifiable, Sendable {
    public enum Kind: Hashable, Sendable {
        case recent
        case all
    }

    /// What a row is tagged with. A picker whose rows share a tag has no single row to tick, so
    /// the project alone is not enough once it can be listed twice.
    public struct Choice: Hashable, Sendable {
        public let kind: Kind
        public let repoID: RepoID

        public init(kind: Kind, repoID: RepoID) {
            self.kind = kind
            self.repoID = repoID
        }
    }

    public let kind: Kind
    public let repos: [Repo]

    public var id: Kind { kind }
    public var title: String { kind == .recent ? "Recent" : "All projects" }

    /// - Parameter recent: project ids newest first, as `RecentProjects.ordered` gives them.
    ///   Resolved against `repos` so a rename shows at once and a removed project drops out.
    public static func grouped(_ repos: [Repo], recent: [RepoID]) -> [ProjectMenuGroup] {
        let recentRepos = recent.compactMap { id in repos.first { $0.id == id } }
        let sorted = repos.sorted {
            $0.name.localizedStandardCompare($1.name) == .orderedAscending
        }

        return [
            ProjectMenuGroup(kind: .recent, repos: recentRepos),
            ProjectMenuGroup(kind: .all, repos: sorted)
        ].filter { !$0.repos.isEmpty }
    }

    /// The row that carries the tick for the chosen project: its Recent row when it has one, so
    /// the tick is at the top of the menu rather than halfway down the full list.
    ///
    /// No project yet is an empty id in the full list, which no row carries.
    public static func choice(for repoID: RepoID?, in groups: [ProjectMenuGroup]) -> Choice {
        let id = repoID ?? RepoID("")
        let inRecent = groups.contains { $0.kind == .recent && $0.repos.contains { $0.id == id } }
        return Choice(kind: inRecent ? .recent : .all, repoID: id)
    }
}
