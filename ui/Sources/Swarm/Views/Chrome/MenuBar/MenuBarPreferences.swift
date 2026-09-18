import AppKit
import SwiftUI
import SwarmCore

/// What the menu bar item shows, and what somebody arranged in Settings ▸ Menu Bar.
///
/// One instance for the life of the process, shared by the menu and the status item.
///
/// **What used to be here and is not any more.** This was `UsageMenuModel`, and most of it was
/// about usage figures: which provider's meters rode in the bar, in what order, starred how, drawn
/// as text or as bars. Swarm no longer reports usage at all, because Jellow already does, so what
/// is left is the three switches that were never about usage in the first place. The keys keep
/// their old names so that nobody's existing choices are forgotten on the upgrade, including the
/// `usage` in `showsCup`.
@MainActor
@Observable
final class MenuBarPreferences {
    static let shared = MenuBarPreferences()

    /// Whether a cup is drawn while the Mac is being kept awake.
    var showsCup: Bool {
        didSet { defaults.set(showsCup, forKey: MenuBarPreferenceKey.showsCup) }
    }
    /// Whether the waiting and finished counts are drawn beside the mark. Off hides one from the
    /// item only; see `MenuBarSummary.segments` for what still names it.
    var showsWaitingCount: Bool {
        didSet { defaults.set(showsWaitingCount, forKey: MenuBarPreferenceKey.showsWaitingCount) }
    }
    var showsUnreadCount: Bool {
        didSet { defaults.set(showsUnreadCount, forKey: MenuBarPreferenceKey.showsUnreadCount) }
    }

    @ObservationIgnored private let defaults: UserDefaults

    init(defaults: UserDefaults = .standard) {
        self.defaults = defaults
        showsCup = defaults.object(forKey: MenuBarPreferenceKey.showsCup) as? Bool ?? true
        showsWaitingCount = defaults.object(forKey: MenuBarPreferenceKey.showsWaitingCount) as? Bool ?? true
        showsUnreadCount = defaults.object(forKey: MenuBarPreferenceKey.showsUnreadCount) as? Bool ?? true
    }
}
