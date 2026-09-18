import Foundation

/// Where the menu bar item's switches are kept in `UserDefaults`.
///
/// **The `usage` in two of these names is history rather than meaning.** This was
/// `UsagePreferenceKey`, and it held the layout, the meter style, the icon style and the figures
/// switch as well. Swarm no longer reports usage, because Jellow already does. The three keys left
/// keep their original strings so that nobody's existing choices are forgotten on the upgrade.
public enum MenuBarPreferenceKey {
    /// Whether a cup is drawn while the Mac is being kept awake.
    public static let showsCup = "menuBar.usage.showsCup"
    public static let showsWaitingCount = "menuBar.showsWaitingCount"
    public static let showsUnreadCount = "menuBar.showsUnreadCount"
}
