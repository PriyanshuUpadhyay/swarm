/// What a workspace row shows at its trailing edge.
///
/// Conductor shows how long ago the work last moved, and that is the default. The diff counts the
/// sidebar used to show are one choice away, because some people read a sidebar by size of change.
public enum SidebarRowDetail: String, CaseIterable, Sendable, Identifiable {
    case time
    case changes
    case both

    public static let storageKey = "sidebar.rowDetail"

    public var id: String { rawValue }

    public var title: String {
        switch self {
        case .time: "Last activity"
        case .changes: "Lines changed"
        case .both: "Both"
        }
    }

    public var showsTime: Bool { self != .changes }
    public var showsChanges: Bool { self != .time }
}
